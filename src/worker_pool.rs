use std::sync::{mpsc, Arc, Mutex};
use std::thread;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TaskResult {
    Ok,
    Terminate,
}

enum WorkerMessage<F: FnOnce(usize) -> TaskResult> {
    Task(F),
    Poke,
}

struct Worker {
    thread: Option<thread::JoinHandle<()>>,
}

impl Worker {
    fn new<F: FnOnce(usize) -> TaskResult>(
        id: usize,
        num_workers: usize,
        rx: Arc<Mutex<mpsc::Receiver<WorkerMessage<F>>>>,
        tx: Arc<Mutex<mpsc::Sender<WorkerMessage<F>>>>,
        terminate: Arc<Mutex<bool>>,
    ) -> Worker
    where
        F: FnOnce(usize) -> TaskResult + Send + 'static,
    {
        let thread = Some(thread::spawn(move || loop {
            let msg = rx.lock().unwrap().recv().unwrap();

            if *terminate.lock().unwrap() {
                break;
            }

            match msg {
                WorkerMessage::Task(task) => match task(id) {
                    TaskResult::Ok => {}
                    TaskResult::Terminate => {
                        *terminate.lock().unwrap() = true;
                        let tx = tx.lock().unwrap();
                        for _ in 1..num_workers {
                            tx.send(WorkerMessage::Poke).ok();
                        }
                        break;
                    }
                },
                WorkerMessage::Poke => {
                    break;
                }
            }
        }));

        Worker { thread }
    }

    fn join(&mut self) {
        if let Some(thread) = self.thread.take() {
            thread.join().ok();
        }
    }
}

pub struct WorkerPool<F: FnOnce(usize) -> TaskResult>
where
    F: FnOnce(usize) -> TaskResult + Send + 'static,
{
    workers: Vec<Worker>,
    tx: Arc<Mutex<mpsc::Sender<WorkerMessage<F>>>>,
    terminate: Arc<Mutex<bool>>,
}

impl<F: FnOnce(usize) -> TaskResult> WorkerPool<F>
where
    F: FnOnce(usize) -> TaskResult + Send + 'static,
{
    pub fn new(num_workers: usize) -> WorkerPool<F> {
        assert!(num_workers > 0);

        let (tx, rx) = mpsc::channel();
        let tx = Arc::new(Mutex::new(tx));
        let rx = Arc::new(Mutex::new(rx));

        let terminate = Arc::new(Mutex::new(false));

        let mut workers = Vec::with_capacity(num_workers);

        for id in 0..num_workers {
            workers.push(Worker::new(
                id,
                num_workers,
                rx.clone(),
                tx.clone(),
                terminate.clone(),
            ));
        }

        WorkerPool {
            workers,
            tx,
            terminate,
        }
    }

    /// Submits a task to the pool. The task will be executed by one of the workers.
    pub fn submit_task(&self, task_fn: F) {
        self.tx
            .lock()
            .unwrap()
            .send(WorkerMessage::Task(task_fn))
            .unwrap();
    }

    fn send_poke(&self) -> Result<(), mpsc::SendError<WorkerMessage<F>>> {
        let tx = self.tx.lock().unwrap();
        for _ in 0..self.workers.len() {
            tx.send(WorkerMessage::Poke)?;
        }
        Ok(())
    }

    /// Waits for all submitted tasks to finish. Returns `TaskResult::Terminate` if any task returned `TaskResult::Terminate`.
    pub fn wait(&mut self) -> TaskResult {
        self.send_poke().ok();
        for worker in &mut self.workers {
            worker.join();
        }

        if *self.terminate.lock().unwrap() {
            TaskResult::Terminate
        } else {
            TaskResult::Ok
        }
    }

    /// Terminates all workers. Currently ongoing tasks will be finished.
    #[allow(dead_code)]
    pub fn terminate(&mut self) {
        *self.terminate.lock().unwrap() = true;
        self.send_poke().ok();
    }
}

// Assure that all workers are joined when the pool is dropped.
impl<F: FnOnce(usize) -> TaskResult> Drop for WorkerPool<F>
where
    F: FnOnce(usize) -> TaskResult + Send + 'static,
{
    fn drop(&mut self) {
        self.wait();
    }
}

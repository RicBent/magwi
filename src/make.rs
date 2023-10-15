use std::collections::HashMap;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::process::Command;

use binrw::{BinReaderExt, BinWriterExt};
use enum_map::enum_map;
use object::read::*;

use super::{
    exheader::{self, Exheader},
    hook::{self, HookExtraPos, HookInfo, HookKind, HookLocation, HookWriter},
    job_env::JobEnv,
    jobs::{find_jobs, Job, JobKind},
    worker_pool::{TaskResult, WorkerPool},
};

#[derive(Debug)]
struct PrePostEntry {
    extra_pos: HookExtraPos,
    pre: Vec<(u32, HookLocation)>,
    post: Vec<(u32, HookLocation)>,
}

#[derive(Debug, thiserror::Error)]
pub enum MakeError {
    #[error("Compilation Failed")]
    CompilationFailed,

    #[error("Linking Failed")]
    LinkingFailed,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Binrw error: {0}")]
    Binrw(#[from] binrw::Error),

    #[error("Object parsing error: {0}")]
    Object(#[from] object::read::Error),

    #[error("Hook error: {0}")]
    HookLocation(HookLocation, String),

    #[error("Hook error: {0}")]
    Hook(#[from] hook::Error),
}

pub type MakeResult<T> = core::result::Result<T, MakeError>;

struct Make {
    project_path: PathBuf,
    writer: HookWriter,
    exheader: Exheader,
    jobs: Vec<Job>,
    loader_address: u32,
    loader_max_size: u32,
    custom_text_address: u32,
    pre_post_entries: Vec<PrePostEntry>,
    symtab_index: HashMap<String, u32>,
}

macro_rules! hook_error {
    ($loc:expr, $($arg:tt)*) => {
        return Err(MakeError::HookLocation($loc, format!($($arg)*)));
    };
}

impl Make {
    pub fn new(project_path: impl AsRef<Path>) -> MakeResult<Self> {
        let project_path = project_path.as_ref().to_path_buf();
        std::env::set_current_dir(&project_path)?;

        let writer = HookWriter::new(0x100000, std::fs::read("original/code.bin")?);

        let exheader: Exheader = std::fs::File::open("original/exheader.bin")?.read_ne()?;

        let loader_address =
            exheader.info.sci.text_section.address + exheader.info.sci.text_section.size;
        let loader_max_size = exheader.info.sci.text_section.num_pages * exheader::PAGE_SIZE
            - exheader.info.sci.text_section.size;
        let custom_text_address = exheader.info.sci.data_section.address
            + exheader.info.sci.data_section.num_pages * exheader::PAGE_SIZE
            + exheader.info.sci.bss_size;

        let jobs = find_jobs("source", "build/obj", "build/dep", true)?;

        Ok(Self {
            project_path,
            writer,
            exheader,
            jobs,
            loader_address,
            loader_max_size,
            custom_text_address,
            pre_post_entries: Vec::new(),
            symtab_index: HashMap::new(),
        })
    }

    pub fn run(&mut self) -> MakeResult<()> {
        self.compile()?;
        self.pre_link()?;
        self.link()?;
        self.sym_hooks()?;
        self.patch_exheader()?;
        Ok(())
    }

    fn compile(&mut self) -> MakeResult<()> {
        let job_env = std::sync::Arc::from(JobEnv {
            cwd: self.project_path.clone(),
            compiler: enum_map! {
                JobKind::C   => "arm-none-eabi-gcc",
                JobKind::CPP => "arm-none-eabi-g++",
                JobKind::ASM => "arm-none-eabi-gcc",
            },
            flags: enum_map! {
                JobKind::C   => vec![
                    "-iquote", "include", "-isystem", "include/sys", "-isystem", "include/sys/clib",
                    "-march=armv6k+fp", "-mtune=mpcore", "-mfloat-abi=hard", "-mtp=soft",
                    "-fdiagnostics-color", "-Wall", "-O3", "-mword-relocations", "-fshort-wchar", "-fomit-frame-pointer", "-ffunction-sections", "-nostdinc"
                ],
                JobKind::CPP => vec![
                    "-iquote", "include", "-isystem", "include/sys", "-isystem", "include/sys/clib",
                    "-march=armv6k+fp", "-mtune=mpcore", "-mfloat-abi=hard", "-mtp=soft",
                    "-fdiagnostics-color", "-Wall", "-O3", "-mword-relocations", "-fshort-wchar", "-fomit-frame-pointer", "-ffunction-sections", "-nostdinc",
                    "-fno-exceptions", "-fno-rtti"
                ],
                JobKind::ASM => vec![
                    "-iquote", "include", "-isystem", "include/sys", "-isystem", "include/sys/clib",
                    "-march=armv6k+fp", "-mtune=mpcore", "-mfloat-abi=hard", "-mtp=soft",
                    "-fdiagnostics-color", "-x", "assembler-with-cpp"
                ],
            },
        });

        self.jobs.iter_mut().for_each(|job| {
            job.update_build_reason();
        });

        let todo_jobs: Vec<&Job> = self
            .jobs
            .iter()
            .filter(|job| job.build_required())
            .collect();

        let pb_root = indicatif::MultiProgress::new();

        let pb = indicatif::ProgressBar::new(todo_jobs.len() as u64);
        pb.set_style(
            indicatif::ProgressStyle::with_template(
                "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} ({eta})",
            )
            .expect("Progress style template should be valid")
            .progress_chars("=>."),
        );
        pb_root.add(pb.clone());
        pb.inc(0);

        let spinner_style = indicatif::style::ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .expect("Progress style template should be valid");

        let num_workers = num_cpus::get();
        let spinners = (0..num_workers)
            .map(|_| {
                let pb = pb_root.add(indicatif::ProgressBar::new_spinner());
                pb.set_style(spinner_style.clone());
                pb.set_message(format!("waiting..."));
                pb
            })
            .collect::<Vec<_>>();

        let mut pool = WorkerPool::new(num_workers);

        for job in todo_jobs {
            let pb = pb.clone();
            let spinners = spinners.clone();
            let job = job.clone();
            let job_env: std::sync::Arc<JobEnv<'_>> = job_env.clone();

            pool.submit_task(move |thread_idx| {
                let spinner = &spinners[thread_idx];
                spinner.enable_steady_tick(std::time::Duration::from_millis(100));
                spinner.set_message(job.src_path.display().to_string());

                match job_env.execute_job(&job) {
                    Ok(_) => {
                        pb.inc(1);
                        TaskResult::Ok
                    }
                    Err(e) => {
                        pb.println(e.to_string());
                        TaskResult::Terminate
                    }
                }
            });
        }

        let pool_result = pool.wait();

        pb.finish_and_clear();
        for spinner in spinners {
            spinner.finish_and_clear();
        }
        pb_root.clear().ok();

        if pool_result != TaskResult::Ok {
            return Err(MakeError::CompilationFailed);
        }

        Ok(())
    }

    fn pre_link(&mut self) -> MakeResult<()> {
        let mut linker_file = std::fs::File::create("build/linker.ld")?;

        linker_file.write("SECTIONS\n{\n    /* Hook Generated Sections */\n".as_bytes())?;

        for job in &self.jobs {
            let elf_data = std::fs::read(&job.obj_path)?;
            let elf_file = object::File::parse(elf_data.as_slice())?;

            for section in elf_file.sections() {
                let Ok(name) = section.name() else {
                    continue;
                };

                match HookInfo::from_section_str(name) {
                    Ok(hi) => {
                        match hi.kind {
                            HookKind::Replace(repl_addr) => {
                                linker_file
                                    .write(
                                        format!("    {name} 0x{repl_addr:x} : {{ *({name}); }}\n")
                                            .as_bytes(),
                                    )
                                    .unwrap();
                            }
                            // Invalid kinds are discarded
                            _ => {
                                hook_error!(hi.location, "Invalid hook kind for section hook");
                            }
                        }
                    }
                    Err(hook::Error::InvalidPrefix) => {}
                    Err(hook::Error::ParsingError(e, loc)) => {
                        hook_error!(loc, "{}", e);
                    }

                    Err(e) => {
                        return Err(e.into());
                    }
                }
            }
        }

        linker_file.write(
            format!(
                "\n    .mw_loader_text 0x{:x} : {{ *(.mw_loader_text); *(.mw_loader_text.*); }}\n",
                self.loader_address
            )
            .as_bytes(),
        )?;
        linker_file.write(format!("    .text 0x{:x} :\n", self.custom_text_address).as_bytes())?;
        linker_file.write(LINKER_SCRIPT_SECTIONS.as_bytes())?;

        linker_file.write("}\n".as_bytes()).unwrap();

        Ok(())
    }

    fn link(&self) -> MakeResult<()> {
        let mut output = Command::new("arm-none-eabi-g++")
            .current_dir(&self.project_path)
            .args(vec![
                "-nodefaultlibs",
                "-nostartfiles",
                "-march=armv6k+fp",
                "-mtune=mpcore",
                "-mfloat-abi=hard",
                "-mtp=soft",
                "-T",
                "symbols.ld",
                "-T",
                "build/linker.ld",
                "-Wl,-Map=build/out.map",
                "-fdiagnostics-color",
            ])
            .args(self.jobs.iter().map(|job| &job.obj_path))
            .arg("-o")
            .arg("build/out.elf")
            .output()?;

        let err = String::from_utf8_lossy(&output.stderr);
        if !err.is_empty() {
            println!("{}", err);
        }
        if !output.status.success() {
            return Err(MakeError::LinkingFailed);
        }

        Ok(())
    }

    fn sym_hooks(&mut self) -> MakeResult<()> {
        let elf_data = std::fs::read("build/out.elf")?;
        let elf_file = object::File::parse(elf_data.as_slice())?;

        let Some(symtab) = elf_file.symbol_table() else {
            return Ok(());
        };

        for sym in symtab.symbols() {
            let Ok(name) = sym.name() else {
                continue;
            };

            let address = sym.address() as u32;

            self.symtab_index.insert(name.into(), address);
            if let Ok(demangled_sym) = cpp_demangle::Symbol::new(name) {
                self.symtab_index.insert(demangled_sym.to_string(), address);
            }
        }

        Ok(())
    }

    fn patch_exheader(&mut self) -> MakeResult<()> {
        self.exheader.info.sci.text_section.size =
            self.exheader.info.sci.text_section.num_pages * exheader::PAGE_SIZE;
        self.exheader.info.sci.data_section.size =
            self.writer.end_address() - self.exheader.info.sci.data_section.address;
        self.exheader.info.sci.data_section.num_pages =
            exheader::page_count(self.exheader.info.sci.data_section.size);
        self.exheader.info.sci.bss_size = 0;

        std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open("build/exheader.bin")?
            .write_le(&self.exheader)?;

        Ok(())
    }
}

const LINKER_SCRIPT_SECTIONS: &str = r#"    {
    __mw_text_start = .;
    *(.text);
    *(.text.*);
    *(.rodata);
    *(.rodata.*);
    __init_array_start = .;
    *(.init_array);
    *(.init_array.*);
    __init_array_end = .;
    __fini_array_start = .;
    *(.fini_array);
    *(.fini_array.*);
    __fini_array_end = .;
    *(.data);
    *(.data.*);
    *(.bss);
    *(.bss.*);
    __mw_text_end = .;
}
"#;

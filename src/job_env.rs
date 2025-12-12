use super::jobs::{Job, JobKind};
use enum_map::EnumMap;
use std::path::PathBuf;

use std::process::Command;
use crate::hook::symbol_safe::path_to_symbol_safe;


pub struct JobEnv<'a> {
    pub cwd: PathBuf,
    pub compiler: EnumMap<JobKind, &'a str>,
    pub flags: EnumMap<JobKind, Vec<&'a str>>,
}

impl JobEnv<'_> {
        pub fn execute_job(&self, job: &Job) -> Result<String, std::io::Error> {
        if !job.build_required() {
            return Ok(String::new());
        }

        std::fs::create_dir_all(job.obj_path.parent().unwrap()).unwrap();
        std::fs::create_dir_all(job.dep_path.parent().unwrap()).unwrap();

        let compiler = self.compiler[job.kind];

        let output = Command::new(compiler)
            .current_dir(&self.cwd)
            .arg("-MMD")
            .arg("-MF")
            .arg(&job.dep_path)
            .args(&self.flags[job.kind])
            .arg(format!("-D__mw_symbol_safe_filename={}", path_to_symbol_safe(&job.src_path)))
            .arg("-c")
            .arg(&job.src_path)
            .arg("-o")
            .arg(&job.obj_path)
            .output()?;

        let output_string = String::from_utf8_lossy(&output.stderr);

        if !output.status.success() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                output_string,
            ));
        }

        Ok(output_string.into_owned())
    }
}

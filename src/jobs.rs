use std::path::{Path, PathBuf, StripPrefixError};

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum BuildReason {
    Forced,
    ObjMissing,
    SrcMissing,
    SrcNewer,
    DependencyNewer,
    DependencyMissing,
    NoDependencyFile,
}

fn dep_requires_rebuild(
    obj_time: std::time::SystemTime,
    dep_path: impl AsRef<Path>,
) -> Option<BuildReason> {
    if !dep_path.as_ref().exists() {
        return Some(BuildReason::NoDependencyFile);
    }

    let Ok(dep_file) = std::fs::read_to_string(dep_path) else {
        return Some(BuildReason::NoDependencyFile);
    };

    for line in dep_file.lines() {
        for part in line.trim().split_ascii_whitespace() {
            let part = part.trim();

            if part == "\\" || part.ends_with(":") {
                continue;
            }

            let Ok(part_meta) = std::fs::metadata(part) else {
                return Some(BuildReason::DependencyMissing);
            };

            let Ok(part_time) = part_meta.modified() else {
                return Some(BuildReason::DependencyMissing);
            };

            if part_time > obj_time {
                return Some(BuildReason::DependencyNewer);
            }
        }
    }

    None
}

#[derive(Debug, PartialEq, Clone, Copy, enum_map::Enum)]
pub enum JobKind {
    C,
    CPP,
    ASM,
}

impl JobKind {
    fn from_ext(ext: &str) -> Option<Self> {
        let ext = ext.to_ascii_lowercase();
        match ext.as_str() {
            "c" => Some(Self::C),
            "cpp" => Some(Self::CPP),
            "s" => Some(Self::ASM),
            _ => None,
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct Job {
    pub kind: JobKind,

    pub src_path: PathBuf,
    pub obj_path: PathBuf,
    pub dep_path: PathBuf,

    pub build_reason: Option<BuildReason>,
}

impl Job {
    fn calc_build_reason(&self) -> Option<BuildReason> {
        let Ok(src_meta) = std::fs::metadata(&self.src_path) else {
            return Some(BuildReason::SrcMissing);
        };
        let Ok(obj_meta) = std::fs::metadata(&self.obj_path) else {
            return Some(BuildReason::ObjMissing);
        };

        let Ok(src_time) = src_meta.modified() else {
            return Some(BuildReason::SrcMissing);
        };
        let Ok(obj_time) = obj_meta.modified() else {
            return Some(BuildReason::ObjMissing);
        };

        if src_time > obj_time {
            return Some(BuildReason::SrcNewer);
        }

        dep_requires_rebuild(obj_time, &self.dep_path)
    }

    #[allow(dead_code)]
    pub fn update_build_reason(&mut self) {
        self.build_reason = self.calc_build_reason();
    }

    pub fn build_required(&self) -> bool {
        self.build_reason.is_some()
    }
}

fn path_replace_prefix_add_suffix(
    path: impl AsRef<Path>,
    from: impl AsRef<Path>,
    to: impl AsRef<Path>,
    suffix: &str,
) -> Result<PathBuf, StripPrefixError> {
    let mut buf = path
        .as_ref()
        .strip_prefix(from)
        .map(|p| to.as_ref().join(p))?
        .into_os_string();
    buf.push(suffix);
    Ok(buf.into())
}

fn find_jobs_impl(
    current_src_path: impl AsRef<Path>,
    src_path: impl AsRef<Path>,
    obj_path: impl AsRef<Path>,
    dep_path: impl AsRef<Path>,
    recursive: bool,
) -> std::io::Result<Vec<Job>> {
    let mut jobs = Vec::new();

    for entry in std::fs::read_dir(current_src_path)? {
        let entry = entry?;
        let entry_type = entry.file_type()?;
        let entry_path = entry.path();

        if recursive && entry_type.is_dir() {
            let mut sub_jobs = find_jobs_impl(
                &entry_path,
                src_path.as_ref(),
                obj_path.as_ref(),
                dep_path.as_ref(),
                recursive,
            )?;
            jobs.append(&mut sub_jobs);
        } else if entry_type.is_file() {
            if let Some(ext) = entry_path.extension() {
                if let Some(kind) = JobKind::from_ext(ext.to_str().unwrap()) {
                    let job = Job {
                        kind,
                        src_path: entry_path.clone(),
                        obj_path: path_replace_prefix_add_suffix(
                            &entry_path,
                            src_path.as_ref(),
                            obj_path.as_ref(),
                            ".o",
                        )
                        .expect("replacing src prefix should always work"),
                        dep_path: path_replace_prefix_add_suffix(
                            &entry_path,
                            src_path.as_ref(),
                            dep_path.as_ref(),
                            ".d",
                        )
                        .expect("replacing src prefix should always work"),
                        build_reason: Some(BuildReason::Forced),
                    };

                    jobs.push(job);
                }
            }
        }
    }

    Ok(jobs)
}

pub fn find_jobs(
    src_path: impl AsRef<Path>,
    obj_path: impl AsRef<Path>,
    dep_path: impl AsRef<Path>,
    recursive: bool,
) -> std::io::Result<Vec<Job>> {
    find_jobs_impl(
        src_path.as_ref(),
        src_path.as_ref(),
        obj_path,
        dep_path,
        recursive,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use filetime::set_file_mtime;

    #[test]
    fn test_find_jobs() {
        let tempdir = tempfile::tempdir().unwrap();
        std::env::set_current_dir(&tempdir).unwrap();

        std::fs::create_dir_all("src/sub").unwrap();
        std::fs::create_dir_all("obj/sub").unwrap();
        std::fs::create_dir_all("dep/sub").unwrap();

        let t3 = std::time::SystemTime::now();
        let t2 = t3 - std::time::Duration::from_secs(1);
        let t1 = t2 - std::time::Duration::from_secs(1);

        println!("t1: {:?}", t1);
        println!("t2: {:?}", t2);

        // No rebuild
        std::fs::write("src/a.c", "").unwrap();
        set_file_mtime("src/a.c", t1.into()).unwrap();
        std::fs::write("src/a1.h", "").unwrap();
        set_file_mtime("src/a1.h", t1.into()).unwrap();
        std::fs::write("src/a2.h", "").unwrap();
        set_file_mtime("src/a2.h", t1.into()).unwrap();
        std::fs::write("src/a3.h", "").unwrap();
        set_file_mtime("src/a3.h", t1.into()).unwrap();
        std::fs::write("obj/a.c.o", "").unwrap();
        set_file_mtime("obj/a.c.o", t2.into()).unwrap();
        std::fs::write("dep/a.c.d", "src/a.c: src/a1.h \\\n src/a2.h \\\n src/a3.h").unwrap();
        set_file_mtime("dep/a.c.d", t2.into()).unwrap();

        // Rebuild: No obj file
        std::fs::write("dep/b.cpp.d", "").unwrap();
        set_file_mtime("dep/b.cpp.d", t1.into()).unwrap();
        std::fs::write("src/b.cpp", "").unwrap();
        set_file_mtime("src/b.cpp", t2.into()).unwrap();

        // Rebuild: Obj file older than src file
        std::fs::write("obj/c.s.o", "").unwrap();
        set_file_mtime("obj/c.s.o", t1.into()).unwrap();
        std::fs::write("dep/c.s.d", "").unwrap();
        set_file_mtime("dep/c.s.d", t1.into()).unwrap();
        std::fs::write("src/c.s", "").unwrap();
        set_file_mtime("src/c.s", t2.into()).unwrap();

        // Rebuild: Dep file (src/sub/d3.h) newer than obj file
        std::fs::write("src/sub/d.c", "").unwrap();
        set_file_mtime("src/sub/d.c", t1.into()).unwrap();
        std::fs::write("obj/sub/d.c.o", "").unwrap();
        set_file_mtime("obj/sub/d.c.o", t2.into()).unwrap();
        std::fs::write(
            "dep/sub/d.c.d",
            "src/sub/d.c: src/sub/d1.h \\\n src/sub/d2.h \\\n src/sub/d3.h",
        )
        .unwrap();
        set_file_mtime("dep/sub/d.c.d", t2.into()).unwrap();
        std::fs::write("src/sub/d1.h", "").unwrap();
        set_file_mtime("src/sub/d1.h", t1.into()).unwrap();
        std::fs::write("src/sub/d2.h", "").unwrap();
        set_file_mtime("src/sub/d2.h", t1.into()).unwrap();
        std::fs::write("src/sub/d3.h", "").unwrap();
        set_file_mtime("src/sub/d3.h", t3.into()).unwrap();

        let job_a = Job {
            kind: JobKind::C,
            src_path: PathBuf::from("src/a.c"),
            obj_path: PathBuf::from("obj/a.c.o"),
            dep_path: PathBuf::from("dep/a.c.d"),
            build_reason: None,
        };

        let job_b = Job {
            kind: JobKind::CPP,
            src_path: PathBuf::from("src/b.cpp"),
            obj_path: PathBuf::from("obj/b.cpp.o"),
            dep_path: PathBuf::from("dep/b.cpp.d"),
            build_reason: Some(BuildReason::ObjMissing),
        };

        let job_c = Job {
            kind: JobKind::ASM,
            src_path: PathBuf::from("src/c.s"),
            obj_path: PathBuf::from("obj/c.s.o"),
            dep_path: PathBuf::from("dep/c.s.d"),
            build_reason: Some(BuildReason::SrcNewer),
        };

        let job_d = Job {
            kind: JobKind::C,
            src_path: PathBuf::from("src/sub/d.c"),
            obj_path: PathBuf::from("obj/sub/d.c.o"),
            dep_path: PathBuf::from("dep/sub/d.c.d"),
            build_reason: Some(BuildReason::DependencyNewer),
        };

        let mut jobs = find_jobs("src", "obj", "dep", false).unwrap();
        jobs.iter_mut().for_each(|job| job.update_build_reason());
        jobs.sort_by(|a, b| a.src_path.cmp(&b.src_path));
        assert_eq!(jobs.len(), 3);
        assert_eq!(jobs[0], job_a);
        assert_eq!(jobs[1], job_b);
        assert_eq!(jobs[2], job_c);

        let mut jobs = find_jobs("src", "obj", "dep", true).unwrap();
        jobs.iter_mut().for_each(|job| job.update_build_reason());
        jobs.sort_by(|a, b| a.src_path.cmp(&b.src_path));
        assert_eq!(jobs.len(), 4);
        assert_eq!(jobs[0], job_a);
        assert_eq!(jobs[1], job_b);
        assert_eq!(jobs[2], job_c);
        assert_eq!(jobs[3], job_d);
    }
}

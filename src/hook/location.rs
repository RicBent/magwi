use std::fmt::Display;
use std::path::PathBuf;

#[derive(Debug, PartialEq, Clone)]
pub struct HookLocation {
    pub file: PathBuf,
    pub line: u32,
}

impl Display for HookLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.file.to_string_lossy(), self.line)
    }
}

impl AsRef<HookLocation> for HookLocation {
    fn as_ref(&self) -> &HookLocation {
        self
    }
}

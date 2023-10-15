use super::error::*;
use super::symbol_safe::symbol_safe_to_path;
use super::HookLocation;

pub struct HookMeta<'a> {
    pub kind_str: &'a str,
    pub arg_str: &'a str,
    pub location: HookLocation,
    pub counter: u32,
}

impl<'a> HookMeta<'a> {
    pub fn from_str(s: &'a str) -> Result<Self, MetaParsingError> {
        if s.is_empty() {
            return Err(MetaParsingError::MissingKind);
        }

        let mut split = s.split('$');

        let kind_str = split.next().ok_or(MetaParsingError::MissingKind)?;
        let arg_str = split.next().ok_or(MetaParsingError::MissingArgument)?;
        let file_str = split.next().ok_or(MetaParsingError::MissingFile)?;
        let line_str = split.next().ok_or(MetaParsingError::MissingLine)?;
        let counter_str = split.next().ok_or(MetaParsingError::MissingCounter)?;

        let file = symbol_safe_to_path(file_str).map_err(MetaParsingError::InvalidFile)?;
        let line = line_str
            .parse()
            .map_err(|_| MetaParsingError::InvalidLine(line_str.to_string()))?;
        let counter = counter_str
            .parse()
            .map_err(|_| MetaParsingError::InvalidCounter(counter_str.to_string()))?;

        Ok(HookMeta {
            kind_str,
            arg_str,
            location: HookLocation { file, line },
            counter,
        })
    }
}

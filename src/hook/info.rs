use super::error::*;
use super::{HookKind, HookLocation, HookMeta};

#[derive(Debug, PartialEq)]
pub struct HookInfo {
    pub kind: HookKind,
    pub location: HookLocation,
    pub counter: u32,
}

impl AsRef<HookLocation> for HookInfo {
    fn as_ref(&self) -> &HookLocation {
        &self.location
    }
}

impl HookInfo {
    fn from_str(input: impl AsRef<str>) -> Result<Self, Error> {
        let meta = HookMeta::from_str(input.as_ref()).map_err(|e| Error::MetaParsingError(e))?;
        let kind = HookKind::from_str(meta.kind_str, meta.arg_str)
            .map_err(|e| Error::ParsingError(e, meta.location.clone()))?;

        Ok(HookInfo {
            kind,
            location: meta.location,
            counter: meta.counter,
        })
    }

    pub const SECTION_PREFIX: &'static str = ".__mw_hook_";

    pub fn from_section_str(section_str: impl AsRef<str>) -> Result<Self, Error> {
        let section_str = section_str.as_ref();

        if section_str.starts_with(Self::SECTION_PREFIX) {
            Self::from_str(&section_str[Self::SECTION_PREFIX.len()..])
        } else {
            Err(Error::InvalidPrefix)
        }
    }

    pub const SYMBOL_PREFIX: &'static str = "__mw_hook_";

    pub fn from_symbol_str(symbol_str: impl AsRef<str>) -> Result<Self, Error> {
        let symbol_str = symbol_str.as_ref();

        if symbol_str.starts_with(Self::SYMBOL_PREFIX) {
            let end_index = symbol_str.rfind('@').unwrap_or_else(|| symbol_str.len());
            Self::from_str(&symbol_str[Self::SYMBOL_PREFIX.len()..end_index])
        } else {
            Err(Error::InvalidPrefix)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::{
        arm::{ArmBranch, ArmCondition},
        symbol_safe::{self, path_to_symbol_safe},
    };

    use std::path::PathBuf;

    #[test]
    fn test_hook_info() {
        let file = PathBuf::from("src/main.cpp");
        assert_eq!(
            HookInfo::from_str(format!("pre$0x1234${}$10$0", path_to_symbol_safe(&file))),
            Ok(HookInfo {
                kind: HookKind::Pre(0x1234),
                location: HookLocation { file, line: 10 },
                counter: 0,
            })
        );

        let file = PathBuf::from("src/sub/test_file.s");
        assert_eq!(
            HookInfo::from_str(format!("post$0x1234${}$10$1", path_to_symbol_safe(&file))),
            Ok(HookInfo {
                kind: HookKind::Post(0x1234),
                location: HookLocation { file, line: 10 },
                counter: 1,
            })
        );

        let file = PathBuf::from("src/main.cpp");
        assert_eq!(
            HookInfo::from_str(format!("b$0x1234${}$42$2", path_to_symbol_safe(&file))),
            Ok(HookInfo {
                kind: HookKind::Branch(ArmBranch {
                    condition: ArmCondition::AL,
                    link: false,
                    from_addr: 0x1234
                }),
                location: HookLocation { file, line: 42 },
                counter: 2,
            })
        );

        assert_eq!(
            HookInfo::from_str(""),
            Err(Error::MetaParsingError(MetaParsingError::MissingKind))
        );
        assert_eq!(
            HookInfo::from_str("b"),
            Err(Error::MetaParsingError(MetaParsingError::MissingArgument))
        );
        assert_eq!(
            HookInfo::from_str("b$0x1234"),
            Err(Error::MetaParsingError(MetaParsingError::MissingFile))
        );
        assert_eq!(
            HookInfo::from_str("b$0x1234$src/main.cpp"),
            Err(Error::MetaParsingError(MetaParsingError::MissingLine))
        );
        assert_eq!(
            HookInfo::from_str("b$0x1234$src/main.cpp$10"),
            Err(Error::MetaParsingError(MetaParsingError::MissingCounter))
        );
        assert_eq!(
            HookInfo::from_str("pre$0x1234$a$10$0"),
            Err(Error::MetaParsingError(MetaParsingError::InvalidFile(
                symbol_safe::DecodeError::InvalidBase32
            )))
        );
    }

    #[test]
    fn test_hook_from_symbol() {
        let file = PathBuf::from("src/main.cpp");
        assert_eq!(
            HookInfo::from_symbol_str(format!(
                "__mw_hook_bl$0x00${}$10$0",
                path_to_symbol_safe(&file)
            )),
            Ok(HookInfo {
                kind: HookKind::Branch(ArmBranch {
                    condition: ArmCondition::AL,
                    link: true,
                    from_addr: 0x00
                }),
                location: HookLocation { file, line: 10 },
                counter: 0
            })
        );

        let file = PathBuf::from("src/sub/test_file.s");
        assert_eq!(
            HookInfo::from_symbol_str(format!(
                "__mw_hook_bl$0x00${}$42$0@0",
                path_to_symbol_safe(&file)
            )),
            Ok(HookInfo {
                kind: HookKind::Branch(ArmBranch {
                    condition: ArmCondition::AL,
                    link: true,
                    from_addr: 0x00
                }),
                location: HookLocation { file, line: 42 },
                counter: 0
            })
        );

        assert_eq!(HookInfo::from_symbol_str("xyz"), Err(Error::InvalidPrefix));
    }

    #[test]
    fn test_hook_from_section() {
        let file = PathBuf::from("src/main.cpp");
        assert_eq!(
            HookInfo::from_section_str(format!(
                ".__mw_hook_bl$0x00${}$10$0",
                path_to_symbol_safe(&file)
            )),
            Ok(HookInfo {
                kind: HookKind::Branch(ArmBranch {
                    condition: ArmCondition::AL,
                    link: true,
                    from_addr: 0x00
                }),
                location: HookLocation { file, line: 10 },
                counter: 0
            })
        );
        assert_eq!(HookInfo::from_section_str("xyz"), Err(Error::InvalidPrefix));
    }
}

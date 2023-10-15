use super::error::*;
use super::arm::ArmBranch;
use super::util::parse_address;

#[derive(Debug, PartialEq)]
pub enum HookKind {
    Pre(u32),
    Post(u32),
    Branch(ArmBranch),
    Replace(u32),
    Symptr(u32)
}

impl HookKind {
    pub fn from_str(kind_str: &str, arg_str: &str) -> Result<Self, ParsingError> {
        let kind_str_lowercase = kind_str.to_ascii_lowercase();
        match kind_str_lowercase.as_str() {
            "pre" => Ok(HookKind::Pre(parse_address(arg_str)?)),
            "post" => Ok(HookKind::Post(parse_address(arg_str)?)),
            "replace" => Ok(HookKind::Replace(parse_address(arg_str)?)),
            "symptr" => Ok(HookKind::Symptr(parse_address(arg_str)?)),
            _ => {
                let branch = ArmBranch::from_str(&kind_str_lowercase, arg_str).map_err(|e| {
                    match e {
                        ParsingError::InvalidBranch(_) => ParsingError::InvalidKind(kind_str.to_string()),
                        _ => e
                    }
                })?;
                Ok(HookKind::Branch(branch))
            }
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use super::super::arm::ArmCondition;

    #[test]
    fn test_hook_kind() {
        assert_eq!(
            HookKind::from_str("pre", "0x1234"),
            Ok(HookKind::Pre(0x1234))
        );
        assert_eq!(
            HookKind::from_str("post", "0x1234"),
            Ok(HookKind::Post(0x1234))
        );
        assert_eq!(
            HookKind::from_str("bleq", "0x1234"),
            Ok(HookKind::Branch(ArmBranch {
                condition: ArmCondition::EQ,
                link: true,
                from_addr: 0x1234
            }))
        );
        assert_eq!(
            HookKind::from_str("xyz", ""),
            Err(ParsingError::InvalidKind("xyz".to_string()).into())
        );
    }
}
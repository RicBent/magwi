use super::error::*;
use super::util::parse_address;

use std::str::FromStr;

#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum ArmCondition {
    EQ = 0x0,
    NE = 0x1,
    CS = 0x2,
    CC = 0x3,
    MI = 0x4,
    PL = 0x5,
    VS = 0x6,
    VC = 0x7,
    HI = 0x8,
    LS = 0x9,
    GE = 0xA,
    LT = 0xB,
    GT = 0xC,
    LE = 0xD,
    AL = 0xE,
    NV = 0xF,
}

impl FromStr for ArmCondition {
    type Err = ParsingError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "eq" => Ok(ArmCondition::EQ),
            "ne" => Ok(ArmCondition::NE),
            "cs" => Ok(ArmCondition::CS),
            "hs" => Ok(ArmCondition::CS),
            "cc" => Ok(ArmCondition::CC),
            "lo" => Ok(ArmCondition::CC),
            "mi" => Ok(ArmCondition::MI),
            "pl" => Ok(ArmCondition::PL),
            "vs" => Ok(ArmCondition::VS),
            "vc" => Ok(ArmCondition::VC),
            "hi" => Ok(ArmCondition::HI),
            "ls" => Ok(ArmCondition::LS),
            "ge" => Ok(ArmCondition::GE),
            "lt" => Ok(ArmCondition::LT),
            "gt" => Ok(ArmCondition::GT),
            "le" => Ok(ArmCondition::LE),
            "al" => Ok(ArmCondition::AL),
            "nv" => Ok(ArmCondition::NV),
            "" => Ok(ArmCondition::AL),
            _ => Err(Self::Err::InvalidCondition(s.to_string())),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct ArmBranch {
    pub condition: ArmCondition,
    pub link: bool,
    pub from_addr: u32,
}

impl ArmBranch {
    pub fn to_u32(&self, to_addr: u32) -> Option<u32> {
        let offset = (to_addr as i64 / 4) - (self.from_addr as i64 / 4) - 2;
        if offset < -0x1000000 || offset > 0xFFFFFF {
            return None;
        }
        let offset = (offset & 0xFFFFFF) as u32;

        let mut result = 0b101u32 << 25;
        result |= (self.condition as u32) << 28;
        result |= (self.link as u32) << 24;
        result |= offset;

        Some(result)
    }
}

impl ArmBranch {
    pub fn from_str(s: &str, from_addr_str: &str) -> Result<Self, ParsingError> {
        let l = s.len();
        let s = s.to_ascii_lowercase();

        if l == 1 || l == 3 {
            if s.starts_with("b") {
                return Ok(ArmBranch {
                    condition: ArmCondition::from_str(&s[1..])?,
                    link: false,
                    from_addr: parse_address(from_addr_str)?,
                });
            }
        } else if l == 2 || l == 4 {
            if s.starts_with("bl") {
                return Ok(ArmBranch {
                    condition: ArmCondition::from_str(&s[2..])?,
                    link: true,
                    from_addr: parse_address(from_addr_str)?,
                });
            }
        }

        Err(ParsingError::InvalidBranch(s.to_string()))
    }
}

pub fn make_branch_u32(
    link: bool,
    from_addr: u32,
    to_addr: u32,
    condition: ArmCondition,
) -> Option<u32> {
    ArmBranch {
        condition,
        link,
        from_addr,
    }
    .to_u32(to_addr)
}

pub fn make_push_u32(registers_bitfield: u16, cond: ArmCondition) -> u32 {
    0x092D0000u32 | (cond as u32) << 28 | registers_bitfield as u32
}

pub fn make_pop_u32(registers_bitfield: u16, cond: ArmCondition) -> u32 {
    0x08BD0000u32 | (cond as u32) << 28 | registers_bitfield as u32
}

pub fn relocate_u32(val: u32, src_address: u32, dest_address: u32) -> Option<u32> {
    let mut r = val;

    let nybble14 = (val >> 24) & 0xF;

    // b/bl
    if nybble14 == 0xA || nybble14 == 0xB {
        r &= 0xFF000000;

        let old_offset = ((val as i64 & 0xFFFFFF) + 2) * 4;
        let b_dest_address = src_address as i64 + old_offset;
        let new_offset = (b_dest_address / 4) - (dest_address as i64 / 4) - 2;

        if new_offset < -0x1000000 || new_offset > 0xFFFFFF {
            return None;
        }

        r |= (new_offset & 0xFFFFFF) as u32;
    }

    Some(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_condition() {
        assert_eq!(ArmCondition::from_str("eq"), Ok(ArmCondition::EQ));
        assert_eq!(ArmCondition::from_str("ne"), Ok(ArmCondition::NE));
        assert_eq!(ArmCondition::from_str("cs"), Ok(ArmCondition::CS));
        assert_eq!(ArmCondition::from_str("hs"), Ok(ArmCondition::CS));
        assert_eq!(ArmCondition::from_str("cc"), Ok(ArmCondition::CC));
        assert_eq!(ArmCondition::from_str("lo"), Ok(ArmCondition::CC));
        assert_eq!(ArmCondition::from_str("mi"), Ok(ArmCondition::MI));
        assert_eq!(ArmCondition::from_str("pl"), Ok(ArmCondition::PL));
        assert_eq!(ArmCondition::from_str("vs"), Ok(ArmCondition::VS));
        assert_eq!(ArmCondition::from_str("vc"), Ok(ArmCondition::VC));
        assert_eq!(ArmCondition::from_str("hi"), Ok(ArmCondition::HI));
        assert_eq!(ArmCondition::from_str("ls"), Ok(ArmCondition::LS));
        assert_eq!(ArmCondition::from_str("ge"), Ok(ArmCondition::GE));
        assert_eq!(ArmCondition::from_str("lt"), Ok(ArmCondition::LT));
        assert_eq!(ArmCondition::from_str("gt"), Ok(ArmCondition::GT));
        assert_eq!(ArmCondition::from_str("le"), Ok(ArmCondition::LE));
        assert_eq!(ArmCondition::from_str("al"), Ok(ArmCondition::AL));
        assert_eq!(ArmCondition::from_str("nv"), Ok(ArmCondition::NV));
        assert_eq!(ArmCondition::from_str(""), Ok(ArmCondition::AL));
        assert_eq!(ArmCondition::from_str("EQ"), Ok(ArmCondition::EQ));
        assert_eq!(ArmCondition::from_str("Ne"), Ok(ArmCondition::NE));
        assert_eq!(ArmCondition::from_str("cS"), Ok(ArmCondition::CS));
        assert_eq!(
            ArmCondition::from_str("xyz"),
            Err(ParsingError::InvalidCondition("xyz".to_string()))
        );
    }

    #[test]
    fn test_parse_branch() {
        assert_eq!(
            ArmBranch::from_str("b", "0x0"),
            Ok(ArmBranch {
                condition: ArmCondition::AL,
                link: false,
                from_addr: 0x0
            })
        );
        assert_eq!(
            ArmBranch::from_str("bl", "0x4"),
            Ok(ArmBranch {
                condition: ArmCondition::AL,
                link: true,
                from_addr: 0x4
            })
        );
        assert_eq!(
            ArmBranch::from_str("beq", "0x8"),
            Ok(ArmBranch {
                condition: ArmCondition::EQ,
                link: false,
                from_addr: 0x8
            })
        );
        assert_eq!(
            ArmBranch::from_str("blt", "0xC"),
            Ok(ArmBranch {
                condition: ArmCondition::LT,
                link: false,
                from_addr: 0xC
            })
        );
        assert_eq!(
            ArmBranch::from_str("bllt", "512"),
            Ok(ArmBranch {
                condition: ArmCondition::LT,
                link: true,
                from_addr: 512
            })
        );
        assert_eq!(
            ArmBranch::from_str("blltx", "0"),
            Err(ParsingError::InvalidBranch("blltx".to_string()))
        );
        assert_eq!(
            ArmBranch::from_str("b", "xyz"),
            Err(ParsingError::InvalidAddress("xyz".to_string()))
        );
    }
}

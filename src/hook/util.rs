use super::error::*;

pub fn parse_address(s: &str) -> Result<u32, ParsingError> {
    if s.starts_with("0x") || s.starts_with("0X") {
        u32::from_str_radix(&s[2..], 16)
    } else {
        u32::from_str_radix(&s, 10)
    }
    .map_err(|_| ParsingError::InvalidAddress(s.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_address() {
        assert_eq!(parse_address("0x1234"), Ok(0x1234));
        assert_eq!(parse_address("0X1234"), Ok(0x1234));
        assert_eq!(parse_address("1234"), Ok(1234));
        assert_eq!(
            parse_address("0x"),
            Err(ParsingError::InvalidAddress("0x".to_string()))
        );
        assert_eq!(
            parse_address("0X"),
            Err(ParsingError::InvalidAddress("0X".to_string()))
        );
        assert_eq!(
            parse_address(""),
            Err(ParsingError::InvalidAddress("".to_string()))
        );
        assert_eq!(
            parse_address("0x1234x"),
            Err(ParsingError::InvalidAddress("0x1234x".to_string()))
        );
        assert_eq!(
            parse_address("0X1234X"),
            Err(ParsingError::InvalidAddress("0X1234X".to_string()))
        );
        assert_eq!(
            parse_address("1234x"),
            Err(ParsingError::InvalidAddress("1234x".to_string()))
        );
    }
}

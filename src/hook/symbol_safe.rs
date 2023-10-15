use std::path::{Path, PathBuf};

/// Converts a path to a symbol-safe (i.e. valid in C/C++ code) string.
pub fn path_to_symbol_safe(path: impl AsRef<Path>) -> String {
    data_encoding::BASE32_NOPAD.encode(path.as_ref().to_string_lossy().as_bytes())
}

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum DecodeError {
    #[error("Invalid base32")]
    InvalidBase32,

    #[error("Invalid UTF-8")]
    InvalidUtf8,
}

/// Reverses the effect of `path_to_symbol_safe`.
#[allow(dead_code)]
pub fn symbol_safe_to_path(s: impl AsRef<str>) -> Result<PathBuf, DecodeError> {
    let data = data_encoding::BASE32_NOPAD
        .decode(s.as_ref().as_bytes())
        .map_err(|_| DecodeError::InvalidBase32)?;
    let s = std::str::from_utf8(&data).map_err(|_| DecodeError::InvalidUtf8)?;
    Ok(PathBuf::from(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode() {
        let paths = vec![
            PathBuf::from("/home/user/My Project/src/main.cpp"),
            PathBuf::from("C:\\Users\\user\\My Project\\src\\main.cpp"),
            PathBuf::from("/_abcABC/.././test.s"),
        ];

        for path in paths {
            let encoded = path_to_symbol_safe(&path);
            let decoded = symbol_safe_to_path(&encoded).unwrap();
            assert_eq!(path, decoded);
        }
    }

    #[test]
    fn test_base32_error() {
        let inputs = vec!["a", "z", "_", "W", "="];

        for input in inputs {
            let result = symbol_safe_to_path(input);
            assert_eq!(result, Err(DecodeError::InvalidBase32));
        }
    }

    #[test]
    fn test_utf8_error() {
        let inputs = vec![b"\x80", b"\xbf", b"\xfe", b"\xff"];

        for input in inputs {
            let b32 = data_encoding::BASE32_NOPAD.encode(input);
            let result = symbol_safe_to_path(b32);
            assert_eq!(result, Err(DecodeError::InvalidUtf8));
        }
    }
}

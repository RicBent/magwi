use std::collections::HashMap;
use std::io::BufRead;
use std::path::Path;

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum HksError {
    #[error("Invalid title")]
    InvalidTitleLine(usize),

    #[error("Invalid property syntax")]
    InvalidKeyValueLine(usize),

    #[error("Missing property key")]
    EmptyKey(usize),

    #[error("Missing property value")]
    EmptyValue(usize),

    #[error("Duplicate property key \"{1}\"")]
    DuplicateKey(usize, String),
}

impl HksError {
    pub fn line(&self) -> usize {
        match self {
            HksError::InvalidTitleLine(line)
            | HksError::InvalidKeyValueLine(line)
            | HksError::EmptyKey(line)
            | HksError::EmptyValue(line)
            | HksError::DuplicateKey(line, _) => *line,
        }
    }
}


#[derive(Debug, PartialEq, thiserror::Error)]
pub enum HksParseError {
    #[error("Missing key: {0}")]
    MissingKey(String),

    #[error("Invalid {0} value: {1}")]
    InvalidTypeValue(String, String),
}

#[derive(Debug, PartialEq)]
pub struct HksEntry {
    title: String,
    line: usize,
    kv: HashMap<String, String>,
}

impl HksEntry {
    pub fn line(&self) -> usize {
        self.line
    }

    pub fn is_done(&self) -> bool {
        self.kv.is_empty()
    }

    pub fn remaining_keys(&self) -> impl Iterator<Item = &str> {
        self.kv.keys().map(|s| s.as_str())
    }

    pub fn has(&self, key: &str) -> bool {
        self.kv.contains_key(key)
    }

    pub fn get(&mut self, key: &str) -> Result<String, HksParseError> {
        if let Some(value) = self.kv.remove(key) {
            return Ok(value);
        }

        Err(HksParseError::MissingKey(key.into()))
    }

    pub fn get_bool(&mut self, key: &str) -> Result<bool, HksParseError> {
        let value = self.get(key)?;

        match value.as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(HksParseError::InvalidTypeValue("bool".into(), value.into())),
        }
    }

    pub fn get_address(&mut self, key: &str) -> Result<u32, HksParseError> {
        let value = self.get(key)?;
        super::util::parse_address(value.as_str())
            .map_err(|_| HksParseError::InvalidTypeValue("address".into(), value.into()))
    }
}
pub struct HksReader<T>
where
    T: BufRead,
{
    reader_lines: std::io::Lines<T>,
    line_i: usize,
    next_title: Option<(String, usize)>,
}

impl<T> HksReader<T>
where
    T: BufRead,
{
    pub fn new(reader: T) -> Self {
        Self {
            reader_lines: reader.lines(),
            line_i: 0,
            next_title: None,
        }
    }

    fn next_line(&mut self) -> Option<Result<String, std::io::Error>> {
        let r = self.reader_lines.next();
        if r.is_some() {
            self.line_i += 1;
        }
        r
    }

    fn line_strip_comment_and_truncate_end(line: &mut String) {
        if let Some(comment_start) = line.find('#') {
            line.truncate(comment_start);
        }
        line.truncate(line.trim_end().len());
    }
}

impl<T> Iterator for HksReader<T>
where
    T: BufRead,
{
    type Item = Result<HksEntry, HksError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next_title.is_none() {
            loop {
                let Some(Ok(mut line)) = self.next_line() else {
                    break;
                };

                Self::line_strip_comment_and_truncate_end(&mut line);

                if line.is_empty() {
                    continue;
                }

                let first_c = line.chars().next().expect("line is not empty");

                if first_c.is_whitespace() {
                    return Some(Err(HksError::InvalidTitleLine(self.line_i)));
                }

                if line.ends_with(':') {
                    line.pop();
                }

                self.next_title = Some((line, self.line_i));
                break;
            }
        }

        let Some((title, title_line_i)) = self.next_title.take() else {
            return None;
        };

        let mut kv = HashMap::new();

        loop {
            let Some(Ok(mut line)) = self.next_line() else {
                break;
            };

            Self::line_strip_comment_and_truncate_end(&mut line);

            if line.is_empty() {
                continue;
            }

            let first_c = line.chars().next().expect("line is not empty");

            if !first_c.is_whitespace() {
                if line.ends_with(':') {
                    line.pop();
                }
                self.next_title = Some((line, self.line_i));
                break;
            }

            let Some(split_i) = line.find(":") else {
                return Some(Err(HksError::InvalidKeyValueLine(self.line_i)));
            };

            let key = line[..split_i].trim().to_string().to_ascii_lowercase();
            let value = line[split_i + 1..].trim().to_string();

            if key.is_empty() {
                return Some(Err(HksError::EmptyKey(self.line_i)));
            }
            if value.is_empty() {
                return Some(Err(HksError::EmptyValue(self.line_i)));
            }
            if kv.contains_key(&key) {
                return Some(Err(HksError::DuplicateKey(self.line_i, key)));
            }

            kv.insert(key, value);
        }

        Some(Ok(HksEntry {
            title,
            kv,
            line: title_line_i,
        }))
    }
}

pub fn open_file(
    path: impl AsRef<Path>,
) -> Result<HksReader<std::io::BufReader<std::fs::File>>, std::io::Error> {
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    Ok(HksReader::new(reader))
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! make_kv {
        ($($key:expr => $value:expr),* $(,)?) => {
            [
                $(
                    ($key.to_string(), $value.to_string()),
                )*
            ].iter().cloned().collect()
        };
    }

    #[test]
    fn test_read() {
        let mut reader = HksReader::new(std::io::Cursor::new(
            r#"test:
    a: 1
    b: 2
    c: 3

test2:
    a: 1
test3:
    b: 1:2:3
"#,
        ));
        assert_eq!(
            reader.next().unwrap().unwrap(),
            HksEntry {
                title: "test".into(),
                line: 1,
                kv: make_kv! {
                    "a" => "1",
                    "b" => "2",
                    "c" => "3",
                },
            }
        );

        assert_eq!(
            reader.next().unwrap().unwrap(),
            HksEntry {
                title: "test2".into(),
                line: 6,
                kv: make_kv! {
                    "a" => "1",
                },
            }
        );

        assert_eq!(
            reader.next().unwrap().unwrap(),
            HksEntry {
                title: "test3".into(),
                line: 8,
                kv: make_kv! {
                    "b" => "1:2:3",
                },
            }
        );

        assert!(reader.next().is_none());
    }

    #[test]
    fn test_read_errors() {
        let mut reader = HksReader::new(std::io::Cursor::new(" a: 1"));
        assert_eq!(
            reader.next().unwrap().unwrap_err(),
            HksError::InvalidTitleLine(0)
        );

        let mut reader = HksReader::new(std::io::Cursor::new("test:\n a\n"));
        assert_eq!(
            reader.next().unwrap().unwrap_err(),
            HksError::InvalidKeyValueLine(1)
        );

        let mut reader = HksReader::new(std::io::Cursor::new("test:\n :a\n"));
        assert_eq!(
            reader.next().unwrap().unwrap_err(),
            HksError::EmptyKey(1)
        );

        let mut reader = HksReader::new(std::io::Cursor::new("test:\n a:\n"));
        assert_eq!(
            reader.next().unwrap().unwrap_err(),
            HksError::EmptyValue(1)
        );
    }
}

use super::HookLocation;
use super::symbol_safe;

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum MetaParsingError {
    #[error("Missing kind")]
    MissingKind,

    #[error("Missing argument")]
    MissingArgument,

    #[error("Missing file")]
    MissingFile,

    #[error("Missing line")]
    MissingLine,

    #[error("Missing counter")]
    MissingCounter,

    #[error("Invalid file: \"{0}\"")]
    InvalidFile(symbol_safe::DecodeError),

    #[error("Invalid line: \"{0}\"")]
    InvalidLine(String),

    #[error("Invalid counter: \"{0}\"")]
    InvalidCounter(String),
}

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum ParsingError {
    #[error("Invalid kind: \"{0}\"")]
    InvalidKind(String),

    #[error("Invalid address: \"{0}\"")]
    InvalidAddress(String),

    #[error("Invalid branch: \"{0}\"")]
    InvalidBranch(String),

    #[error("Invalid instruction condition: \"{0}\"")]
    InvalidCondition(String),
}

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum WriterError {
    #[error("Out of bounds read at 0x{0:x} with size 0x{1:x}")]
    OutOfBoundsRead(u32, usize),

    #[error("Out of bounds write at 0x{0:x} with size 0x{1:x}")]
    OutOfBoundsWrite(u32, usize),

    #[error("Resize below base address: 0x{0:x}")]
    ResizeBelowBaseAddress(u32),

    #[error("Loader extra data address not set")]
    LoaderExtraAddressNotSet,

    #[error("Duplicate write at 0x{0:x} with size 0x{1:x}")]
    DuplicateWrite(u32, usize),
}

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
    #[error("Invalid prefix")]
    InvalidPrefix,

    #[error("{0}")]
    MetaParsingError(#[from] MetaParsingError),

    #[error("{0}")]
    ParsingError(ParsingError, HookLocation),
}

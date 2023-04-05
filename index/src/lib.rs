use std::fmt;
use std::fmt::Formatter;

pub mod index;

#[derive(Debug)]
pub enum LittIndexError {
    CreationError(String),
    OpenError(String),
    WriteError(String),
    PdfParseError(String)
}

impl fmt::Display for LittIndexError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self {
            LittIndexError::CreationError(s) => write!(f, "Index Creation Error: {}", s),
            LittIndexError::OpenError(s) => write!(f, "Error opening existing index: {}", s),
            LittIndexError::WriteError(s) => write!(f, "Index Write Error: {}", s),
            LittIndexError::PdfParseError(s) => write!(f, "Error parsing PDF: {}", s)
        }
    }
}

pub type Result<T> = std::result::Result<T, LittIndexError>;

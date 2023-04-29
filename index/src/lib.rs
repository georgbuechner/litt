use std::fmt;
use std::fmt::Formatter;

pub mod index;

#[derive(Debug)]
pub enum LittIndexError {
    CreationError(String),
    UpdateError(String),
    OpenError(String),
    ReloadError(String),
    WriteError(String),
    PdfParseError(String),
    PdfNotFoundError(String),
}

impl fmt::Display for LittIndexError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self {
            LittIndexError::CreationError(s) => write!(f, "Index Creation Error: {}", s),
            LittIndexError::OpenError(s) => write!(f, "Error opening existing index: {}", s),
            LittIndexError::UpdateError(s) => write!(f, "Error updating the index: {}", s),
            LittIndexError::WriteError(s) => write!(f, "Index Write Error: {}", s),
            LittIndexError::PdfParseError(s) => write!(f, "Error parsing PDF: {}", s),
            LittIndexError::PdfNotFoundError(s) => write!(f, "PDF Not found: {}", s),
            LittIndexError::ReloadError(s) => write!(f, "Error reloading index writer: {}", s),
        }
    }
}

pub type Result<T> = std::result::Result<T, LittIndexError>;

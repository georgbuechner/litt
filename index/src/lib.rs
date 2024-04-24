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
    StateError(String),
    ReadError(String),
    PdfParseError(String),
    TxtParseError(String),
}

impl fmt::Display for LittIndexError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self {
            LittIndexError::CreationError(s) => write!(f, "Index Creation Error: {}", s),
            LittIndexError::OpenError(s) => write!(f, "Error opening existing index: {}", s),
            LittIndexError::UpdateError(s) => write!(f, "Error updating the index: {}", s),
            LittIndexError::WriteError(s) => write!(f, "Index Write Error: {}", s),
            LittIndexError::ReadError(s) => write!(f, "Index Read Error: {}", s),
            LittIndexError::StateError(s) => write!(f, "Index is not in assumed state: {}", s),
            LittIndexError::PdfParseError(s) => write!(f, "Error parsing PDF: {}", s),
            LittIndexError::TxtParseError(s) => write!(f, "Error parsing txt-file: {}", s),
            LittIndexError::ReloadError(s) => write!(f, "Error reloading index writer: {}", s),
        }
    }
}

pub type Result<T> = std::result::Result<T, LittIndexError>;

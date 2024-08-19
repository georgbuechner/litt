use thiserror::Error;

pub mod index;

#[derive(Debug, Error)]
pub enum LittIndexError {
    #[error("Index Creation Error: `{0}`")]
    CreationError(String),
    #[error("Error updating the index: `{0}`")]
    UpdateError(String),
    #[error("Error opening existing index: `{0}`")]
    OpenError(String),
    #[error("Error reloading index writer: `{0}`")]
    ReloadError(String),
    #[error("Index Write Error: `{0}`")]
    WriteError(String),
    #[error("Index is not in assumed state: `{0}`")]
    StateError(String),
    #[error("Index Read Error: `{0}`")]
    ReadError(String),
    #[error("Error parsing PDF: `{0}`")]
    PdfParseError(String),
    #[error("Error parsing txt-file: `{0}`")]
    TxtParseError(String),
}

pub type Result<T> = std::result::Result<T, LittIndexError>;

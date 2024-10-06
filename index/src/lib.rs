use std::io;
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
    #[error(transparent)]
    IoError(#[from] io::Error),
    #[error(transparent)]
    TantivyError(#[from] tantivy::TantivyError),
    #[error(transparent)]
    StripPrefixError(#[from] std::path::StripPrefixError),
    #[error(transparent)]
    SerdeJsonError(#[from] serde_json::error::Error),
    #[error("One of the locks is poisoned: {0}")]
    LockPoisoned(String),
}

impl<T> From<std::sync::PoisonError<T>> for LittIndexError {
    fn from(error: std::sync::PoisonError<T>) -> Self {
        Self::LockPoisoned(error.to_string())
    }
}

pub type Result<T> = std::result::Result<T, LittIndexError>;

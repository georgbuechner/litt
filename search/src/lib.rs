use thiserror::Error;

pub mod search;

#[derive(Debug, Error)]
pub enum LittSearchError {
    #[error("Error initializing new search: `{0}`")]
    InitError(String),
    #[error("Error during search: `{0}`")]
    SearchError(String),
}

pub type Result<T> = std::result::Result<T, LittSearchError>;

use std::io;
use thiserror::Error;

pub mod search;

#[derive(Debug, Error)]
pub enum LittSearchError {
    #[error("Error initializing new search: `{0}`")]
    InitError(String),
    #[error("Error during search: `{0}`")]
    SearchError(String),
    #[error(transparent)]
    IoError(#[from] io::Error),
    #[error(transparent)]
    TantivyError(#[from] tantivy::TantivyError),
    #[error(transparent)]
    LittIndexError(#[from] litt_index::LittIndexError),
    #[error(transparent)]
    QueryParserError(#[from] tantivy::query::QueryParserError),
}

pub type Result<T> = std::result::Result<T, LittSearchError>;

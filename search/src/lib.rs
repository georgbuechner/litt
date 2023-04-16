use std::fmt;
use std::fmt::Formatter;

pub mod search;

#[derive(Debug)]
pub enum LittSearchError {
    InitError(String),
    SearchError(String),
}

impl fmt::Display for LittSearchError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self {
            LittSearchError::InitError(s) => write!(f, "Error initializing new search: {}", s),
            LittSearchError::SearchError(s) => write!(f, "Error during search: {}", s),
        }
    }
}

pub type Result<T> = std::result::Result<T, LittSearchError>;

pub mod search;

#[derive(Debug)]
pub enum LittSearchError {
    InitError(String),
    SearchError(String)
}

pub type Result<T> = std::result::Result<T, LittSearchError>;


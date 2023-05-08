use std::collections::HashMap;
use std::fmt::Formatter;
use std::path::{Path, PathBuf};
use std::{fmt, fs};

#[derive(Debug)]
pub enum LittIndexTrackerError {
    UnknownError(String),
    NotFound(String),
    SaveError(String),
}

impl fmt::Display for LittIndexTrackerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self {
            LittIndexTrackerError::UnknownError(s) => {
                write!(f, "Unknown error reading from index-config: {}", s)
            }
            LittIndexTrackerError::NotFound(s) => {
                write!(f, "The given index {} does not exist", s)
            }
            LittIndexTrackerError::SaveError(s) => {
                write!(f, "The index-config could not be stored: {}", s)
            }
        }
    }
}

pub type Result<T> = std::result::Result<T, LittIndexTrackerError>;

pub struct IndexTracker {
    indices: HashMap<String, PathBuf>,
}

impl IndexTracker {
    pub fn create(_path: String) -> Result<Self> {
        // TODO use path buf and join paths (windows file system compatibility)
        let litt_root = shellexpand::tilde("~/.litt/").to_string();
        let litt_json = shellexpand::tilde("~/.litt/indices.json").to_string();

        // Check if stored litt indices json already exists
        if Path::new(&litt_json).exists() {
            // load json
            let data = fs::read_to_string(litt_json)
                .map_err(|e| LittIndexTrackerError::UnknownError(e.to_string()))?;
            let indices: HashMap<String, PathBuf> = serde_json::from_str(&data)
                .map_err(|e| LittIndexTrackerError::UnknownError(e.to_string()))?;
            Ok(Self { indices })
        // Otherwise create path first
        } else {
            _ = fs::create_dir_all(litt_root)
                .map_err(|e| LittIndexTrackerError::UnknownError(e.to_string()));
            let indices = HashMap::new();
            Ok(Self { indices })
        }
    }

    pub fn exists(&self, name: &str) -> bool {
        self.indices.contains_key(name)
    }

    pub fn path_exists(&self, path_str: &str) -> Option<bool> {
        let path = &PathBuf::from(path_str);
        self.indices
            .iter()
            .find_map(|(_, val)| if val == path { Some(true) } else { None })
    }

    pub fn add(mut self, name: String, path: impl AsRef<Path>) -> Result<()> {
        let litt_json = shellexpand::tilde("~/.litt/indices.json").to_string();
        let documents_path = PathBuf::from(path.as_ref());
        self.indices.insert(name, documents_path);
        // TODO get rid of unwrap
        std::fs::write(litt_json, serde_json::to_string(&self.indices).unwrap())
            .map_err(|e| LittIndexTrackerError::SaveError(e.to_string()))
    }

    pub fn remove(mut self, name: String) -> Result<()> {
        self.indices.remove(&name);
        let litt_json = shellexpand::tilde("~/.litt/indices.json").to_string();
        // TODO get rid of unwrap
        std::fs::write(litt_json, serde_json::to_string(&self.indices).unwrap())
            .map_err(|e| LittIndexTrackerError::SaveError(e.to_string()))
    }

    pub fn get_path(&self, name: &str) -> Result<PathBuf> {
        if self.exists(name) {
            // TODO get rid of unwrap
            Ok(self.indices.get(name).unwrap().into())
        } else {
            Err(LittIndexTrackerError::NotFound(name.into()))
        }
    }

    pub fn get_name(&self, path_str: &str) -> Option<String> {
        let path = &PathBuf::from(path_str);
        self.indices.iter().find_map(|(key, val)| {
            if val == path {
                Some(key.to_string())
            } else {
                None
            }
        })
    }

    pub fn all(self) -> Result<HashMap<String, PathBuf>> {
        Ok(self.indices)
    }
}

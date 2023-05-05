use std::collections::HashMap;
use std::fmt::Formatter;
use std::path::{Path, PathBuf};
use std::{fmt, fs};
use shellexpand;

#[derive(Debug)]
pub enum LittIndexTrackerError {
    UnkownError(String),
    NonExists(String),
    SaveError(String),
}

impl fmt::Display for LittIndexTrackerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self {
            LittIndexTrackerError::UnkownError(s) => {
                write!(f, "Unkown error reading from index-config: {}", s)
            }
            LittIndexTrackerError::NonExists(s) => {
                write!(f, "The given index {} does not exists", s)
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
        let litt_root = shellexpand::tilde("~/.litt/").to_string();
        let litt_json = shellexpand::tilde("~/.litt/indices.json").to_string();

        // Check if stored litt indices json already exists
        if Path::new(&litt_json).exists() {
            // load json
            let data = fs::read_to_string(litt_json)
                .map_err(|e| LittIndexTrackerError::UnkownError(e.to_string()))?;
            let indices: HashMap<String, PathBuf> = serde_json::from_str(&data)
                .map_err(|e| LittIndexTrackerError::UnkownError(e.to_string()))?;
            Ok(Self { indices, })
        // Otherwise create path first
        } else {
            _ = fs::create_dir_all(litt_root)
                .map_err(|e| LittIndexTrackerError::UnkownError(e.to_string()));
            let indices = HashMap::new();
            Ok(Self { indices, })
        }
    }

    pub fn exists(&self, name: &str) -> bool {
        return self.indices.contains_key(name);
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
        std::fs::write(litt_json, serde_json::to_string(&self.indices).unwrap())
            .map_err(|e| LittIndexTrackerError::SaveError(e.to_string()))
    }

    pub fn remove(mut self, name: String) -> Result<()> {
        self.indices.remove(&name);
        let litt_json = shellexpand::tilde("~/.litt/indices.json").to_string();
        std::fs::write(litt_json, serde_json::to_string(&self.indices).unwrap())
            .map_err(|e| LittIndexTrackerError::SaveError(e.to_string()))
    }


    pub fn get_path(&self, name: &str) -> Result<PathBuf> {
        if self.exists(name) {
            Ok(self.indices.get(name).unwrap().into())
        } else {
            Err(LittIndexTrackerError::NonExists(name.into()))
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

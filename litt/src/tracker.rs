use std::collections::HashMap;
use std::fmt::Formatter;
use std::path::{Path, PathBuf};
use std::{fmt, fs};

use litt_shared::LITT_DIRECTORY_NAME;

const INDICIES_FILENAME: &str = "indices.json";
const FAST_RESULTS_FILENAME: &str = "last_results.json";

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
        let base_path = PathBuf::new().join("~/").join(LITT_DIRECTORY_NAME);
        let litt_root = shellexpand::tilde(&base_path.to_string_lossy().to_string()).to_string();
        let json_path = shellexpand::tilde(
            &base_path
                .join(INDICIES_FILENAME)
                .to_string_lossy()
                .to_string(),
        )
        .to_string();

        // Check if stored litt indices json already exists
        if Path::new(&json_path).exists() {
            // load json
            let data = fs::read_to_string(json_path)
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

    pub fn path_exists(&self, path: &PathBuf) -> Option<bool> {
        self.indices
            .iter()
            .find_map(|(_, val)| if val == path { Some(true) } else { None })
    }

    pub fn add(&mut self, name: String, path: impl AsRef<Path>) -> Result<()> {
        let documents_path = PathBuf::from(path.as_ref());
        self.indices.insert(name, documents_path);
        self.store_indicies()
    }

    pub fn remove(mut self, name: String) -> Result<()> {
        self.indices.remove(&name);
        self.store_indicies()
    }

    pub fn get_path(&self, name: &str) -> Result<PathBuf> {
        match self.indices.get(name) {
            Some(path) => Ok(path.into()),
            None => Err(LittIndexTrackerError::NotFound(name.into())),
        }
    }

    pub fn get_name(&self, path: &PathBuf) -> Option<String> {
        self.indices.iter().find_map(|(key, val)| {
            if val == path {
                Some(key.to_string())
            } else {
                None
            }
        })
    }

    pub fn all(&self) -> Result<HashMap<String, PathBuf>> {
        Ok(self.indices.clone())
    }

    pub fn store_fast_results(
        &self,
        fast_results: HashMap<u32, (String, u32, String)>,
    ) -> Result<()> {
        let base_path = PathBuf::new()
            .join("~/")
            .join(LITT_DIRECTORY_NAME)
            .join(FAST_RESULTS_FILENAME);
        let json_path = shellexpand::tilde(&base_path.to_string_lossy().to_string()).to_string();
        let json_str = serde_json::to_string(&fast_results)
            .map_err(|e| LittIndexTrackerError::SaveError(e.to_string()))?;
        std::fs::write(json_path, json_str)
            .map_err(|e| LittIndexTrackerError::SaveError(e.to_string()))
    }

    pub fn load_fast_results(&self) -> Result<HashMap<u32, (String, u32, String)>> {
        let base_path = PathBuf::new()
            .join("~/")
            .join(LITT_DIRECTORY_NAME)
            .join(FAST_RESULTS_FILENAME);
        let json_path = shellexpand::tilde(&base_path.to_string_lossy().to_string()).to_string();
        let data = fs::read_to_string(json_path)
            .map_err(|e| LittIndexTrackerError::UnknownError(e.to_string()))?;
        let fast_results: HashMap<u32, (String, u32, String)> = serde_json::from_str(&data)
            .map_err(|e| LittIndexTrackerError::UnknownError(e.to_string()))?;
        Ok(fast_results)
    }

    fn store_indicies(&self) -> Result<()> {
        let base_path = PathBuf::new()
            .join("~/")
            .join(LITT_DIRECTORY_NAME)
            .join(INDICIES_FILENAME);
        let json_path = shellexpand::tilde(&base_path.to_string_lossy().to_string()).to_string();
        let json_str = serde_json::to_string(&self.indices)
            .map_err(|e| LittIndexTrackerError::SaveError(e.to_string()))?;
        std::fs::write(json_path, json_str)
            .map_err(|e| LittIndexTrackerError::SaveError(e.to_string()))
    }
}

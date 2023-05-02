use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

const LITT_ROOT_PATH: &str = "~/.litt";
const LITT_INDICIES_JSON: &str = "~/.litt/indices.json";

#[derive(Debug)]
pub enum LittIndexTrackerError {
    UnkownError(String),
}

#[derive(Serialize, Deserialize)]
struct Indices {
    indices: HashMap<String, PathBuf>,
}
impl Indices {
    fn new() -> Self {
        let indices: HashMap<String, PathBuf> = HashMap::new();
        Self { indices }
    }
}

type Result<T> = std::result::Result<T, LittIndexTrackerError>;

pub struct IndexTracker {
    // TODO (fux): do we need to have an extra struct or can we have the HashMap here?
    indices: Indices
}

impl IndexTracker {
    pub fn create(_path: String) -> Result<Self>  {
        // Check if stored litt indices json already exists
        if Path::new(LITT_INDICIES_JSON).exists() {
            let data = fs::read_to_string(LITT_INDICIES_JSON)
                .map_err(|e| LittIndexTrackerError::UnkownError(e.to_string()))?;
            let indices: Indices = serde_json::from_str(&data)
                .map_err(|e| LittIndexTrackerError::UnkownError(e.to_string()))?;
            Ok(Self { indices })
        }
        else {
            fs::create_dir_all("/some/dir")
                .map_err(|e| LittIndexTrackerError::UnkownError(e.to_string()));
            let indices = Indices::new();
            Ok(Self { indices })
        }
    }

    pub fn exists(&self, name: &str) -> bool {
        return self.indices.indices.contains_key(name)
    }

    pub fn add(mut self, name: String, path: impl AsRef<Path>) {
        let documents_path = PathBuf::from(path.as_ref());
        self.indices.indices.insert(name, documents_path);
    }

    pub fn get_path(self, name: &str) -> &PathBuf {
        // TODO (fux): get path from `indices` return error if it does not exist.
        return self.indices.indices.get(name).unwrap()
    }

    pub fn get_name(&self, _path: impl AsRef<Path>) -> String {
        // TODO (fux): get name from `indices` by given path.
        String::from("")
    }

    pub fn all(self) -> Result<HashMap<String, PathBuf>> {
        Ok( self.indices.indices )
    }
}

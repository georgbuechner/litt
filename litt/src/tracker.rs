
use std::collections::{HashMap};
use std::path::{Path, PathBuf};

pub struct IndexTracker {
    indecis: HashMap<String, PathBuf>
}

impl IndexTracker  {

    pub fn create(path: String) -> Self {
        let indecis: HashMap<String, PathBuf> = HashMap::new();
        Self { indecis }
    }

    pub fn exists(self, name: String) -> bool {
        true
    }

    pub fn add(self, name: String, path: PathBuf) {
        ()
    }

    pub fn get_path(self, name: &String) -> PathBuf {
        let index_path = PathBuf::from(name);
        index_path
    }

    pub fn all(self) -> HashMap<String, PathBuf> {
        return self.indecis
        
    }
}

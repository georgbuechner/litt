use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct IndexTracker {
    indecis: HashMap<String, PathBuf>,
}

impl IndexTracker {
    pub fn create(_path: String) -> Self {
        let indecis: HashMap<String, PathBuf> = HashMap::new();
        // TODO (fux): read json at `path` and fill indecis
        Self { indecis }
    }

    pub fn exists(&self, _name: &str) -> bool {
        // TODO (fux): check if `name` exists in indecis
        true
    }

    pub fn add(self, _name: String, _path: impl AsRef<Path>) {
        // TODO (fux): add indecis-entry (`name: path`)
    }

    pub fn get_path(self, name: &str) -> PathBuf {
        // TODO (fux): get path from `indecis` return error if it does not exist.
        PathBuf::from(name)
    }

    pub fn get_name(&self, _path: impl AsRef<Path>) -> String {
        // TODO (fux): get name from `indecis` by given path.
        String::from("")
    }

    pub fn all(self) -> HashMap<String, PathBuf> {
        self.indecis
    }
}

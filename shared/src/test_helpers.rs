use std::fs::{remove_dir_all, remove_file};
use std::path::PathBuf;

use crate::LITT_DIRECTORY_NAME;

pub fn cleanup_dir_and_file(directory: &str, file_name: &str) {
    remove_dir_all(directory).unwrap_or_else(|_| panic!("Failed to remove directory: {directory}"));
    // remove if exists, drop result
    let _ = remove_file(file_name);
}

pub fn cleanup_litt_files(directory: &str) {
    let index_path = PathBuf::from(directory).join(LITT_DIRECTORY_NAME);
    _ = std::fs::remove_dir_all(index_path);
}

use std::fs::{remove_dir_all, remove_file};
use std::panic;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;

use crate::LITT_DIRECTORY_NAME;
pub const TEST_DIR_NAME: &str = "resources";
pub const TEST_FILE_PATH: &str = "test.pdf";

pub fn cleanup_dir_and_file(directory: &str, file_name: &str) {
    remove_dir_all(directory).unwrap_or_else(|_| panic!("Failed to remove directory: {directory}"));
    // remove if exists, drop result
    let _ = remove_file(file_name);
}

pub fn cleanup_litt_files(directory: &str) {
    let index_path = PathBuf::from(directory).join(LITT_DIRECTORY_NAME);
    _ = std::fs::remove_dir_all(index_path);
}

fn teardown() {
    cleanup_dir_and_file(TEST_DIR_NAME, TEST_FILE_PATH);
}
pub fn run_test<T>(test: T)
    where
        T: FnOnce() -> std::pin::Pin<Box<dyn std::future::Future<Output = ()>>> + panic::UnwindSafe,
{
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        runtime.block_on(test())
    }));

    teardown();

    assert!(result.is_ok())
}

use crate::LittIndexError::{
    CreationError, OpenError, PdfParseError, ReloadError, UpdateError, WriteError,
};
use crate::Result;
use litt_shared::search_schema::SearchSchema;
use litt_shared::LITT_DIRECTORY_NAME;
use std::collections::HashMap;
use std::convert::AsRef;
use std::fs::create_dir_all;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;
use tantivy::query::QueryParser;
use tantivy::schema::{Document as TantivyDocument, Schema};
use tantivy::{Index as TantivyIndex, IndexReader, IndexWriter, ReloadPolicy, Searcher};
use uuid::Uuid;
use walkdir::{DirEntry, WalkDir};

const INDEX_DIRECTORY_NAME: &str = "index";
const PAGES_DIRECTORY_NAME: &str = "pages";
const CHECK_SUM_MAP_FILENAME: &str = "checksum.json";

/// The total target memory usage that will be split between a given number of threads
const TARGET_MEMORY_BYTES: usize = 100_000_000;

pub struct Index {
    documents_path: PathBuf,
    index: TantivyIndex,
    reader: IndexReader,
    writer: IndexWriter,
    schema: SearchSchema,
}

impl Index {
    pub fn create(path: impl AsRef<Path>, schema: SearchSchema) -> Result<Self> {
        let documents_path = PathBuf::from(path.as_ref());
        let index_path = documents_path
            .join(LITT_DIRECTORY_NAME)
            .join(INDEX_DIRECTORY_NAME);
        create_dir_all(&index_path).map_err(|e| CreationError(e.to_string()))?;
        let index = Self::create_index(&index_path, schema.schema.clone())?;
        let reader = Self::build_reader(&index)?;
        let writer = Self::build_writer(&index)?;
        Ok(Self {
            documents_path,
            index,
            reader,
            writer,
            schema,
        })
    }

    pub fn open_or_create(path: impl AsRef<Path>, schema: SearchSchema) -> Result<Self> {
        let documents_path = PathBuf::from(path.as_ref());
        let index_path = documents_path
            .join(LITT_DIRECTORY_NAME)
            .join(INDEX_DIRECTORY_NAME);
        create_dir_all(&index_path).map_err(|e| CreationError(e.to_string()))?;
        let index = Self::create_index(&index_path, schema.schema.clone())
            .unwrap_or(Self::open_index(&index_path)?);
        let reader = Self::build_reader(&index)?;
        let writer = Self::build_writer(&index)?;
        // TODO make search schema parameter optional and load schema from existing index
        Ok(Self {
            documents_path,
            index,
            reader,
            writer,
            schema,
        })
    }

    /// Add all PDF documents in located in the path this index was created for (see [create()](Self::create)).
    pub fn add_all_pdf_documents(&mut self) -> Result<()> {
        let mut checksum_map = self.open_or_create_checksum_map()?;
        for path in self.get_pdf_dir_entries() {
            let relative_path = path.path()
                .strip_prefix(&self.documents_path)
                .map_err(|e| CreationError(e.to_string()))?;

            let str_path = &path.path().to_string_lossy().to_string();
            if !self
                .compare_checksum(str_path, &checksum_map)
                .unwrap_or(false)
            {
                println!("Adding document: {}", relative_path.to_string_lossy().to_string());
                self.add_pdf_document_pages(&path)?;
                self.update_checksum(str_path, &mut checksum_map)?;
            }
            else {
                println!("Skipped (already exists): {}", relative_path.to_string_lossy().to_string());
            }
        }
        self.store_checksum_map(&checksum_map)?;

        // We need to call .commit() explicitly to force the
        // index_writer to finish processing the documents in the queue,
        // flush the current index to the disk, and advertise
        // the existence of new documents.

        self.writer
            .commit()
            .map_err(|e| WriteError(e.to_string()))?;

        self.reader.reload().map_err(|e| ReloadError(e.to_string()))
    }

    /// For now, just delete existing index and index the documents again.
    pub fn reload(&mut self) -> Result<()> {
        self.writer
            .delete_all_documents()
            .map_err(|e| UpdateError(e.to_string()))?;
        let checksum_map = PathBuf::from(&self.documents_path)
            .join(LITT_DIRECTORY_NAME)
            .join(CHECK_SUM_MAP_FILENAME);
        _ = std::fs::remove_file(checksum_map);
        self.add_all_pdf_documents()
    }

    pub fn searcher(&self) -> Searcher {
        self.reader.searcher()
    }

    pub fn query_parser(&self) -> QueryParser {
        QueryParser::for_index(&self.index, self.schema.default_fields())
    }

    fn create_index(path: &PathBuf, schema: Schema) -> Result<TantivyIndex> {
        TantivyIndex::create_in_dir(path, schema).map_err(|e| CreationError(e.to_string()))
    }

    fn open_index(path: &PathBuf) -> Result<TantivyIndex> {
        TantivyIndex::open_in_dir(path).map_err(|e| OpenError(e.to_string()))
    }

    fn build_reader(index: &TantivyIndex) -> Result<IndexReader> {
        index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()
            .map_err(|e| CreationError(e.to_string()))
    }

    fn build_writer(index: &TantivyIndex) -> Result<IndexWriter> {
        index
            .writer(TARGET_MEMORY_BYTES)
            .map_err(|e| CreationError(e.to_string()))
    }

    fn get_pdf_dir_entries(&self) -> Vec<DirEntry> {
        let walk_dir = WalkDir::new(&self.documents_path);
        walk_dir
            .follow_links(true)
            .into_iter()
            .filter_map(|entry_result| entry_result.ok())
            .filter(|entry| entry.file_name().to_string_lossy().ends_with("pdf"))
            .collect::<Vec<_>>()
    }

    /// Add a tantivy document to the index for each page of the pdf document.
    fn add_pdf_document_pages(&self, dir_entry: &DirEntry) -> Result<()> {
        // Create custom directory to store all pages:
        let doc_id = Uuid::new_v4();
        let pages_path = self
            .documents_path
            .join(LITT_DIRECTORY_NAME)
            .join(PAGES_DIRECTORY_NAME)
            .join(doc_id.to_string());
        create_dir_all(&pages_path).map_err(|e| CreationError(e.to_string()))?;
        let full_path_to_pdf = dir_entry.path();

        // loop over pages
        let mut pdf_to_text_successful = true;
        let mut page_number = 0;

        while pdf_to_text_successful {
            page_number += 1;
            // finalize page output path (to the location where all pages are stored)
            let mut page_path = pages_path.join(page_number.to_string());
            page_path.set_extension("txt");
            // get page body
            let mut pdf_to_text_call = Command::new("pdftotext");
            pdf_to_text_call
                .arg("-f")
                .arg(format!("{}", page_number))
                .arg("-l")
                .arg(format!("{}", page_number))
                .arg(full_path_to_pdf.to_string_lossy().to_string())
                .arg(page_path.to_string_lossy().to_string());

            let pdf_to_text_output = pdf_to_text_call.output().map_err(|_| {
                PdfParseError("Make sure pdftotext is set up correctly and installed (usually part of xpdf (Windows) or poppler (Linux/Mac))".into())
            })?;
            pdf_to_text_successful = pdf_to_text_output.status.success();

            if pdf_to_text_successful {
                // read page-body from generated .txt file
                let page_body = std::fs::read_to_string(&page_path)
                    .map_err(|e| PdfParseError(e.to_string()))?;
                self.add_pdf_page(dir_entry.path(), page_number, &page_path, &page_body)?;
            }
        }

        println!(
            "{} loaded {} page{} at {}",
            dir_entry.path().to_string_lossy(),
            page_number,
            if page_number != 1 { "s" } else { "" },
            full_path_to_pdf.to_string_lossy()
        );

        Ok(())
    }

    fn add_pdf_page(
        &self,
        full_path: &Path,
        page_number: u64,
        page_path: &Path,
        page_body: &str,
    ) -> Result<()> {
        let relative_path = full_path
            .strip_prefix(&self.documents_path)
            .map_err(|e| CreationError(e.to_string()))?;
        // documents_path base from path
        let mut tantivy_document = TantivyDocument::new();

        // add fields to tantivy document
        tantivy_document.add_text(self.schema.path, page_path.to_string_lossy());
        tantivy_document.add_text(self.schema.title, relative_path.to_string_lossy());
        tantivy_document.add_u64(self.schema.page, page_number);
        tantivy_document.add_text(self.schema.body, page_body);
        self.writer
            .add_document(tantivy_document)
            .map_err(|e| WriteError(e.to_string()))?;
        Ok(())
    }

    fn open_or_create_checksum_map(&self) -> Result<HashMap<String, (u64, SystemTime)>> {
        let path = self
            .documents_path
            .join(LITT_DIRECTORY_NAME)
            .join(CHECK_SUM_MAP_FILENAME);
        if Path::new(&path).exists() {
            let data = std::fs::read_to_string(path).map_err(|e| CreationError(e.to_string()))?;

            Ok(serde_json::from_str(&data).map_err(|e| CreationError(e.to_string()))?)
        } else {
            Ok(HashMap::new())
        }
    }

    fn store_checksum_map(&self, checksum_map: &HashMap<String, (u64, SystemTime)>) -> Result<()> {
        let path = self
            .documents_path
            .join(LITT_DIRECTORY_NAME)
            .join(CHECK_SUM_MAP_FILENAME);
        std::fs::write(path, serde_json::to_string(&checksum_map).unwrap())
            .map_err(|e| CreationError(e.to_string()))
    }

    fn update_checksum(
        &self,
        path: &str,
        checksum_map: &mut HashMap<String, (u64, SystemTime)>,
    ) -> Result<()> {
        let file = std::fs::File::open(path).map_err(|e| CreationError(e.to_string()))?;
        let metadata = file.metadata().map_err(|e| CreationError(e.to_string()))?;
        let modified = metadata
            .modified()
            .map_err(|e| CreationError(e.to_string()))?;

        checksum_map.insert(path.to_string(), (metadata.len(), modified));
        Ok(())
    }

    fn compare_checksum(
        &self,
        path: &str,
        checksum_map: &HashMap<String, (u64, SystemTime)>,
    ) -> Result<bool> {
        let file = std::fs::File::open(path).map_err(|e| CreationError(e.to_string()))?;
        let metadata = file.metadata().map_err(|e| CreationError(e.to_string()))?;
        let modified = metadata
            .modified()
            .map_err(|e| CreationError(e.to_string()))?;

        if let Some((len, last_modified)) = checksum_map.get(path) {
            Ok(*len == metadata.len() && *last_modified == modified)
        } else {
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use litt_shared::test_helpers::{cleanup_dir_and_file, save_fake_pdf_document};
    use once_cell::sync::Lazy;
    use serial_test::serial;
    use std::panic;

    const TEST_DIR_NAME: &str = "resources";
    const TEST_FILE_PATH: &str = "test.pdf";

    static SEARCH_SCHEMA: Lazy<SearchSchema> = Lazy::new(SearchSchema::default);

    fn setup() {
        save_fake_pdf_document(TEST_DIR_NAME, TEST_FILE_PATH, vec!["Hello, world".into()]);
    }

    fn teardown() {
        cleanup_dir_and_file(TEST_DIR_NAME, TEST_FILE_PATH);
    }

    fn run_test<T>(test: T)
    where
        T: FnOnce() + panic::UnwindSafe,
    {
        setup();

        let result = panic::catch_unwind(test);

        teardown();

        assert!(result.is_ok())
    }

    #[test]
    #[serial]
    fn test_create() {
        run_test(|| {
            let index = Index::create(TEST_DIR_NAME, SEARCH_SCHEMA.clone()).unwrap();
            assert_eq!(SEARCH_SCHEMA.clone().schema, index.schema.schema);
            assert_eq!(PathBuf::from(TEST_DIR_NAME), index.documents_path);
        });
    }

    #[test]
    #[serial]
    fn test_open_or_create() {
        run_test(|| {
            Index::create(TEST_DIR_NAME, SEARCH_SCHEMA.clone()).unwrap();

            let opened_index = Index::open_or_create(TEST_DIR_NAME, SEARCH_SCHEMA.clone()).unwrap();

            assert_eq!(SEARCH_SCHEMA.clone().schema, opened_index.schema.schema);
            assert_eq!(PathBuf::from(TEST_DIR_NAME), opened_index.documents_path);
            assert!(Path::new(TEST_DIR_NAME)
                .join(LITT_DIRECTORY_NAME)
                .join(INDEX_DIRECTORY_NAME)
                .is_dir())
        });
    }

    #[test]
    #[serial]
    fn test_get_pdf_file_paths() {
        run_test(|| {
            let index = Index::create(TEST_DIR_NAME, SEARCH_SCHEMA.clone()).unwrap();
            let dir_entries = index.get_pdf_dir_entries();
            let file_name = dir_entries.first().unwrap().file_name().to_str().unwrap();

            assert_eq!(1, dir_entries.len());
            assert_eq!(TEST_FILE_PATH, file_name);
        });
    }

    #[test]
    #[serial]
    fn test_add_all_documents() {
        run_test(|| {
            let mut index = Index::create(TEST_DIR_NAME, SEARCH_SCHEMA.clone()).unwrap();
            index.add_all_pdf_documents().unwrap();
            let segments = index.index.searchable_segments().unwrap();
            assert_eq!(1, segments.len());
        });
    }

    #[test]
    #[serial]
    fn test_reload() {
        run_test(|| {
            let mut index = Index::create(TEST_DIR_NAME, SEARCH_SCHEMA.clone()).unwrap();
            // index 1st test document
            index.add_all_pdf_documents().unwrap();
            assert_eq!(1, index.searcher().num_docs());

            // save 2nd document and update
            save_fake_pdf_document(TEST_DIR_NAME, "test2.pdf", vec!["Hello, world 2".into()]);
            index.reload().unwrap();

            assert_eq!(2, index.searcher().num_docs());
        });
    }

    #[test]
    #[serial]
    fn test_update() {
        run_test(|| {
            let mut index = Index::create(TEST_DIR_NAME, SEARCH_SCHEMA.clone()).unwrap();
            // index 1st test document
            index.add_all_pdf_documents().unwrap();
            assert_eq!(1, index.searcher().num_docs());

            // save 2nd document and update
            save_fake_pdf_document(TEST_DIR_NAME, "test2.pdf", vec!["Hello, world 2".into()]);
            index.add_all_pdf_documents().unwrap();

            assert_eq!(2, index.searcher().num_docs());
        });
    }
}

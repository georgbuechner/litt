use crate::LittIndexError::{
    CreationError, OpenError, PdfNotFoundError, PdfParseError, ReloadError, UpdateError, WriteError,
};
use crate::Result;
use litt_shared::search_schema::SearchSchema;
use litt_shared::LITT_DIRECTORY_NAME;
use lopdf::Document as PdfDocument;
use std::convert::AsRef;
use std::fs::create_dir_all;
use std::path::{Path, PathBuf};
use tantivy::query::QueryParser;
use tantivy::schema::{Document as TantivyDocument, Schema};
use tantivy::{Index as TantivyIndex, IndexReader, IndexWriter, ReloadPolicy, Searcher};
use walkdir::{DirEntry, WalkDir};

const INDEX_DIRECTORY_NAME: &str = "index";
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
        println!("[open_or_create] Successfully opened index with {} document pages.", reader.searcher().num_docs());
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
        for path in self.get_pdf_dir_entries() {
            let (path, pdf_document_result) = Self::get_path_and_pdf_document(&path);
            match pdf_document_result {
                Err(e) => {
                    eprintln!("Error reading document ({}): {}", path, e)
                }
                Ok(pdf_document) => {
                    println!("Adding document: {}", path);
                    self.add_pdf_document_pages(&self.writer, pdf_document, path)?;
                }
            }

        }
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
    pub fn update(&mut self) -> Result<()> {
        self.writer
            .delete_all_documents()
            .map_err(|e| UpdateError(e.to_string()))?;
        self.add_all_pdf_documents()
    }

    pub fn searcher(&self) -> Searcher {
        self.reader.searcher()
    }

    pub fn query_parser(&self) -> QueryParser {
        QueryParser::for_index(&self.index, self.schema.default_fields())
    }

    pub fn get_page_body(&self, page: u32, path: impl AsRef<Path>) -> Result<String> {
        let doc = PdfDocument::load(self.documents_path.join(path.as_ref())).map_err(|_e| {
            PdfNotFoundError(self.documents_path.join(path).to_string_lossy().to_string())
        })?;
        let text = doc
            .extract_text(&[page])
            .map_err(|e| PdfParseError(e.to_string()))?;
        Ok(text)
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

    fn get_path_and_pdf_document(dir_entry: &DirEntry) -> (String, Result<PdfDocument>) {
        let pdf_path = dir_entry.path().to_owned();
        (
            dir_entry.file_name().to_string_lossy().to_string(),
            PdfDocument::load(pdf_path).map_err(|e| PdfParseError(e.to_string())),
        )
    }

    /// Add a tantivy document to the index for each page of the pdf document.
    fn add_pdf_document_pages(
        &self,
        index_writer: &IndexWriter,
        pdf_document: PdfDocument,
        path: String,
    ) -> Result<()> {
        let title_option = path.strip_suffix(".pdf");
        for i in 0..pdf_document.get_pages().len() {
            let mut tantivy_document = TantivyDocument::new();
            let page_number = i as u64 + 1;
            let page_body = pdf_document
                .extract_text(&[page_number as u32])
                .map_err(|e| PdfParseError(e.to_string()))?;
            tantivy_document.add_text(self.schema.path, path.clone());

            if let Some(title) = title_option {
                tantivy_document.add_text(self.schema.title, title)
            }
            tantivy_document.add_u64(self.schema.page, page_number);
            tantivy_document.add_text(self.schema.body, page_body);
            index_writer
                .add_document(tantivy_document)
                .map_err(|e| WriteError(e.to_string()))?;
        }
        Ok(())
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
    fn test_update() {
        run_test(|| {
            let mut index = Index::create(TEST_DIR_NAME, SEARCH_SCHEMA.clone()).unwrap();
            // index 1st test document
            index.add_all_pdf_documents().unwrap();
            assert_eq!(1, index.searcher().num_docs());

            // save 2nd document and update
            save_fake_pdf_document(TEST_DIR_NAME, "test2.pdf", vec!["Hello, world 2".into()]);
            index.update().unwrap();

            assert_eq!(2, index.searcher().num_docs());
        });
    }
}

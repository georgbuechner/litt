use crate::LittIndexError::{
    CreationError, OpenError, PdfNotFoundError, PdfParseError, ReloadError, WriteError,
};
use crate::Result;
use litt_shared::search_schema::SearchSchema;
use lopdf::Document as PdfDocument;
use std::fs::create_dir_all;
use std::path::{Path, PathBuf};
use tantivy::query::QueryParser;
use tantivy::schema::{Document as TantivyDocument, Schema};
use tantivy::{Index as TantivyIndex, IndexReader, IndexWriter, ReloadPolicy, Searcher};
use walkdir::{DirEntry, WalkDir};

const INDEX_DIRECTORY_NAME: &str = ".litt-index";

pub struct Index {
    documents_path: PathBuf,
    index: TantivyIndex,
    reader: IndexReader,
    schema: SearchSchema,
}

impl Index {
    pub fn create<P: AsRef<Path>>(path: P, schema: SearchSchema) -> Result<Self> {
        let documents_path = PathBuf::from(path.as_ref());
        let index_path = documents_path.join(INDEX_DIRECTORY_NAME);
        create_dir_all(&index_path).map_err(|e| CreationError(e.to_string()))?;
        let index = Self::create_index(&index_path, schema.schema.clone())?;
        let reader = Self::build_reader(&index)?;
        Ok(Self {
            documents_path,
            index,
            reader,
            schema,
        })
    }

    pub fn open_or_create<P: AsRef<Path>>(path: P, schema: SearchSchema) -> Result<Self> {
        let documents_path = PathBuf::from(path.as_ref());
        let index_path = documents_path.join(INDEX_DIRECTORY_NAME);
        create_dir_all(&index_path).map_err(|e| CreationError(e.to_string()))?;
        let index = Self::create_index(&index_path, schema.schema.clone())
            .unwrap_or(Self::open_index(&index_path)?);
        let reader = Self::build_reader(&index)?;
        // TODO make search schema parameter optional and load schema from existing index
        Ok(Self {
            documents_path,
            index,
            reader,
            schema,
        })
    }

    /// Add all PDF documents in located in the path this index was created for (see [new()](Self::create)).
    pub fn add_all_documents(&self) -> Result<()> {
        let mut index_writer = self
            .index
            .writer(100_000_000)
            .map_err(|e| WriteError(e.to_string()))?;

        let pdf_paths_with_document_results = self.load_pdf_docs();

        for (path, pdf_document_result) in pdf_paths_with_document_results {
            match pdf_document_result {
                Err(e) => {
                    eprintln!("Error reading document ({}): {}", path, e)
                }
                Ok(pdf_document) => {
                    self.add_document(&mut index_writer, pdf_document, path)?;
                }
            }
        }

        // We need to call .commit() explicitly to force the
        // index_writer to finish processing the documents in the queue,
        // flush the current index to the disk, and advertise
        // the existence of new documents.

        index_writer
            .commit()
            .map_err(|e| WriteError(e.to_string()))?;
        Ok(())
    }

    pub fn index(&self) -> &TantivyIndex {
        &self.index
    }

    pub fn searcher(&self) -> Result<Searcher> {
        self.reader
            .reload()
            .map_err(|e| ReloadError(e.to_string()))?;
        Ok(self.reader.searcher())
    }

    pub fn query_parser(&self) -> QueryParser {
        QueryParser::for_index(&self.index, self.schema.default_fields())
    }

    pub fn schema(self) -> SearchSchema {
        self.schema
    }

    pub fn get_page_body(&self, page: u32, path: &str) -> Result<String> {
        let doc = PdfDocument::load(self.documents_path.join(path)).map_err(|_e| {
            PdfNotFoundError(self.documents_path.join(path).to_string_lossy().to_string())
        })?;
        let text = doc.extract_text(&[page]).unwrap();
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
            .reload_policy(ReloadPolicy::OnCommit)
            .try_into()
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

    fn load_pdf_docs(&self) -> Vec<(String, Result<PdfDocument>)> {
        let dir_entries = self.get_pdf_dir_entries();
        let pdf_paths_with_document_results = dir_entries
            .iter()
            .map(|dir_entry| {
                let pdf_path = dir_entry.path().to_owned();
                (
                    dir_entry.file_name().to_string_lossy().to_string(),
                    PdfDocument::load(pdf_path).map_err(|e| PdfParseError(e.to_string())),
                )
            })
            .collect::<Vec<(String, std::result::Result<PdfDocument, _>)>>();
        pdf_paths_with_document_results
    }

    fn add_document(
        &self,
        index_writer: &mut IndexWriter,
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
            let index = Index::create(TEST_DIR_NAME, SEARCH_SCHEMA.clone()).unwrap();
            index.add_all_documents().unwrap();
            let segments = index.index.searchable_segments().unwrap();
            assert_eq!(1, segments.len());
        });
    }
}

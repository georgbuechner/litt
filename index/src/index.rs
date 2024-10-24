use crate::LittIndexError::{PdfParseError, StateError};
use crate::Result;
use litt_shared::search_schema::SearchSchema;
use litt_shared::LITT_DIRECTORY_NAME;
use rayon::prelude::*;
use std::collections::HashMap;
use std::convert::AsRef;
use std::fs::{self, create_dir_all, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use tantivy::query::QueryParser;
use tantivy::schema::{Schema, TantivyDocument};
use tantivy::{Index as TantivyIndex, IndexReader, IndexWriter, ReloadPolicy, Searcher};
use unicode_segmentation::UnicodeSegmentation;
use uuid::Uuid;
use walkdir::{DirEntry, WalkDir};

const INDEX_DIRECTORY_NAME: &str = "index";
const PAGES_DIRECTORY_NAME: &str = "pages";
const CHECK_SUM_MAP_FILENAME: &str = "checksum.json";

/// The total target memory usage that will be split between a given number of threads
const TARGET_MEMORY_BYTES: usize = 100_000_000;

pub enum Index {
    Writing {
        index: TantivyIndex,
        schema: SearchSchema,
        documents_path: PathBuf,
        writer: IndexWriter,
    },
    Reading {
        index: TantivyIndex,
        schema: SearchSchema,
        reader: IndexReader,
        documents_path: PathBuf,
        failed_documents: Vec<String>,
    },
}

pub type PageIndex = HashMap<String, Vec<(u32, u32)>>;

impl Index {
    pub fn create(path: impl AsRef<Path>, schema: SearchSchema) -> Result<Self> {
        let documents_path = PathBuf::from(path.as_ref());
        let index_path = documents_path
            .join(LITT_DIRECTORY_NAME)
            .join(INDEX_DIRECTORY_NAME);
        create_dir_all(&index_path)?;
        let index = Self::create_index(&index_path, schema.schema.clone())?;
        let writer = Self::build_writer(&index)?;
        Ok(Self::Writing {
            documents_path,
            index,
            writer,
            schema,
        })
    }

    pub fn open(path: impl AsRef<Path>, schema: SearchSchema) -> Result<Self> {
        let documents_path = PathBuf::from(path.as_ref());
        let index_path = documents_path
            .join(LITT_DIRECTORY_NAME)
            .join(INDEX_DIRECTORY_NAME);
        let index = Self::open_tantivy_index(&index_path)?;
        let reader = Self::build_reader(&index)?;
        Ok(Self::Reading {
            index,
            schema,
            reader,
            documents_path,
            failed_documents: vec![],
        })
    }

    pub fn open_or_create(path: impl AsRef<Path>, schema: SearchSchema) -> Result<Self> {
        // TODO make search schema parameter optional and load schema from existing index
        let documents_path = PathBuf::from(path.as_ref());
        let index_path = documents_path
            .join(LITT_DIRECTORY_NAME)
            .join(INDEX_DIRECTORY_NAME);
        create_dir_all(&index_path)?;
        let index_create_result = Self::create_index(&index_path, schema.schema.clone());
        match index_create_result {
            Ok(index) => {
                let writer = Self::build_writer(&index)?;
                Ok(Self::Writing {
                    documents_path,
                    index,
                    writer,
                    schema,
                })
            }
            Err(_) => Self::open(path, schema),
        }
    }

    /// Add all PDF documents in located in the path this index was created for (see [create()](Self::create)).
    pub fn add_all_documents(mut self) -> Result<Self> {
        let checksum_map = self.open_checksum_map().ok();
        let dir_entries = self.collect_document_files();

        let failed_documents: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));

        let new_checksum_map: HashMap<_, _> = dir_entries
            .par_iter()
            .filter_map(|path| {
                let key = path.path().to_string_lossy().to_string();
                let existing_checksum = checksum_map.as_ref().and_then(|map| map.get(&key));
                match self.process_file(path, existing_checksum) {
                    Ok(success) => Some(success),
                    Err(e) => failed_documents.lock().ok().and_then(|mut failed_files| {
                        failed_files.push(format!("path: {}, error: {}", path.path().display(), e));
                        None
                    }),
                }
            })
            .collect();

        self.store_checksum_map(new_checksum_map)?;

        // We need to call .commit() explicitly to force the
        // index_writer to finish processing the documents in the queue,
        // flush the current index to the disk, and advertise
        // the existence of new documents.
        if let Index::Writing {
            index,
            schema,
            documents_path,
            mut writer,
        } = self
        {
            writer.commit()?;
            let reader = Self::build_reader(&index)?;
            reader.reload()?;
            self = Index::Reading {
                index,
                schema,
                reader,
                documents_path,
                failed_documents: failed_documents.lock()?.to_vec(),
            };
            Ok(self)
        } else {
            Err(StateError("Writing".to_string()))
        }
    }

    pub fn update(mut self) -> Result<Self> {
        if let Index::Reading {
            index,
            documents_path,
            schema,
            ..
        } = self
        {
            let writer = Self::build_writer(&index)?;
            self = Index::Writing {
                index,
                schema,
                documents_path,
                writer,
            };
            self.add_all_documents()
        } else {
            Err(StateError("Reading".to_string()))
        }
    }

    pub fn failed_documents(&self) -> Result<Vec<String>> {
        if let Index::Reading {
            failed_documents, ..
        } = self
        {
            Ok(failed_documents.to_vec())
        } else {
            Err(StateError("Reading".to_string()))
        }
    }

    pub fn process_file(
        &self,
        path: &DirEntry,
        existing_checksum: Option<&(u64, SystemTime)>,
    ) -> Result<(String, (u64, SystemTime))> {
        if let Index::Writing { documents_path, .. } = &self {
            let relative_path = path.path().strip_prefix(documents_path)?;

            let str_path = path.path().to_string_lossy().to_string();
            if !Self::checksum_is_equal(&str_path, existing_checksum).unwrap_or(false) {
                println!("Adding document: {}", relative_path.to_string_lossy());
                self.add_document(path)?;
                Self::calculate_checksum(&str_path)
            } else {
                println!(
                    "Skipped (already exists): {}",
                    relative_path.to_string_lossy()
                );
                // can unwrap because this arm is only entered when existing checksum is not None
                Ok((str_path, *(existing_checksum.unwrap())))
            }
        } else {
            Err(StateError("Writing".to_string()))
        }
    }

    /// For now, just delete existing index and index the documents again.
    pub fn reload(self) -> Result<Self> {
        if let Index::Reading {
            ref index,
            ref documents_path,
            ..
        } = self
        {
            let writer = Self::build_writer(index)?;
            writer.delete_all_documents()?;
            let checksum_map = PathBuf::from(documents_path)
                .join(LITT_DIRECTORY_NAME)
                .join(CHECK_SUM_MAP_FILENAME);
            _ = std::fs::remove_file(checksum_map);
            self.add_all_documents()
        } else {
            Err(StateError("Reading".to_string()))
        }
    }

    pub fn searcher(&self) -> Result<Searcher> {
        if let Index::Reading { reader, .. } = self {
            Ok(reader.searcher())
        } else {
            Err(StateError("Reading".to_string()))
        }
    }

    pub fn query_parser(&self) -> Result<QueryParser> {
        if let Index::Reading { index, schema, .. } = self {
            Ok(QueryParser::for_index(index, schema.default_fields()))
        } else {
            Err(StateError("Reading".to_string()))
        }
    }

    pub fn page_index(&self, path: &str) -> Result<PageIndex> {
        let mut path = PathBuf::from(path);
        path.set_extension("pageindex");
        let data_str = fs::read_to_string(path.to_string_lossy().to_string())?;
        let fast_results: PageIndex = serde_json::from_str(&data_str)?;
        Ok(fast_results)
    }

    fn create_index(path: &PathBuf, schema: Schema) -> Result<TantivyIndex> {
        TantivyIndex::create_in_dir(path, schema).map_err(Into::into)
    }

    fn open_tantivy_index(path: &PathBuf) -> Result<TantivyIndex> {
        TantivyIndex::open_in_dir(path).map_err(Into::into)
    }

    fn build_reader(index: &TantivyIndex) -> Result<IndexReader> {
        index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()
            .map_err(Into::into)
    }

    fn build_writer(index: &TantivyIndex) -> Result<IndexWriter> {
        index.writer(TARGET_MEMORY_BYTES).map_err(Into::into)
    }

    fn collect_document_files(&self) -> Vec<DirEntry> {
        let documents_path = match self {
            Index::Writing { documents_path, .. } => documents_path,
            Index::Reading { documents_path, .. } => documents_path,
        };
        let walk_dir = WalkDir::new(documents_path);
        walk_dir
            .follow_links(true)
            .into_iter()
            .filter_map(|entry_result| entry_result.ok())
            .filter(|entry| {
                entry.file_name().to_string_lossy().ends_with("pdf")
                    || entry.file_name().to_string_lossy().ends_with("md")
                    || entry.file_name().to_string_lossy().ends_with("txt")
            })
            .collect::<Vec<_>>()
    }

    /// Add a tantivy document to the index for each page of the document.
    fn add_document(&self, dir_entry: &DirEntry) -> Result<()> {
        if let Index::Writing { documents_path, .. } = self {
            // Create custom directory to store all pages:
            let doc_id = Uuid::new_v4();
            let pages_path = documents_path
                .join(LITT_DIRECTORY_NAME)
                .join(PAGES_DIRECTORY_NAME)
                .join(doc_id.to_string());
            create_dir_all(&pages_path)?;
            let full_path = dir_entry.path();

            // Check filetype (pdf/ txt)
            let num = if full_path.to_string_lossy().ends_with("pdf") {
                self.add_pdf_document(dir_entry, pages_path, full_path)?
            } else {
                self.add_txt_document(dir_entry, pages_path, full_path)?
            };
            println!(
                "{} loaded {} page{} at {}",
                dir_entry.path().to_string_lossy(),
                num,
                if num != 1 { "s" } else { "" },
                full_path.to_string_lossy()
            );
            Ok(())
        } else {
            Err(StateError("Writing".to_string()))
        }
    }

    fn add_pdf_document(
        &self,
        dir_entry: &DirEntry,
        pages_path: PathBuf,
        full_path: &Path,
    ) -> Result<u64> {
        // loop over pages
        let mut pdf_to_text_successful = true;
        let mut page_number = 0;

        while pdf_to_text_successful {
            page_number += 1;
            // finalize page output path (to the location where all pages are stored)
            let mut page_path = pages_path.join(page_number.to_string());
            page_path.set_extension("pageinfo");
            // get page body
            let mut pdf_to_text_call = Command::new("pdftotext");
            pdf_to_text_call
                .arg("-f")
                .arg(format!("{}", page_number))
                .arg("-l")
                .arg(format!("{}", page_number))
                .arg(full_path.to_string_lossy().to_string())
                .arg(page_path.to_string_lossy().to_string());

            let pdf_to_text_output = pdf_to_text_call.output().map_err(|_| {
                PdfParseError("Make sure pdftotext is set up correctly and installed (usually part of xpdf (Windows) or poppler (Linux/Mac))".into())
            })?;
            pdf_to_text_successful = pdf_to_text_output.status.success();

            if pdf_to_text_successful {
                // read page-body from generated .txt file
                let page_body = std::fs::read_to_string(&page_path)?;
                self.add_page(dir_entry.path(), page_number, &page_path, &page_body)?;
                Self::store_page_index(&page_path.clone(), Self::create_page_index(&page_body)?)?;
            }
        }

        Ok(page_number)
    }

    fn add_txt_document(
        &self,
        dir_entry: &DirEntry,
        pages_path: PathBuf,
        full_path: &Path,
    ) -> Result<u64> {
        let page_number = 1;
        let mut page_path = pages_path.join(page_number.to_string());
        page_path.set_extension("pageinfo");
        // Open the file in read-only mode
        let mut file = File::open(full_path)?;
        // Store as page seperatly
        let mut destination_file = File::create(page_path.clone())?;
        io::copy(&mut file, &mut destination_file)?;
        // Read the contents of the file into a string
        let mut file = File::open(full_path)?;
        let mut body = String::new();
        file.read_to_string(&mut body)?;
        // Finally, add page
        self.add_page(dir_entry.path(), page_number, &page_path, &body)?;
        Self::store_page_index(&page_path.clone(), Self::create_page_index(&body)?)?;
        Ok(page_number)
    }

    fn add_page(
        &self,
        full_path: &Path,
        page_number: u64,
        page_path: &Path,
        page_body: &str,
    ) -> Result<()> {
        if let Index::Writing {
            documents_path,
            schema,
            writer,
            ..
        } = self
        {
            let relative_path = full_path.strip_prefix(documents_path)?;
            // documents_path base from path
            let mut tantivy_document = TantivyDocument::new();

            // add fields to tantivy document
            tantivy_document.add_text(schema.path, page_path.to_string_lossy());
            tantivy_document.add_text(schema.title, relative_path.to_string_lossy());
            tantivy_document.add_u64(schema.page, page_number);
            tantivy_document.add_text(schema.body, page_body);
            writer.add_document(tantivy_document)?;
            Ok(())
        } else {
            Err(StateError("Writing".to_string()))
        }
    }

    fn open_checksum_map(&self) -> Result<HashMap<String, (u64, SystemTime)>> {
        if let Index::Writing { documents_path, .. } = self {
            let path = documents_path
                .join(LITT_DIRECTORY_NAME)
                .join(CHECK_SUM_MAP_FILENAME);
            let data = std::fs::read_to_string(path)?;
            Ok(serde_json::from_str(&data)?)
        } else {
            Err(StateError("Writing".to_string()))
        }
    }

    fn store_checksum_map(&self, checksum_map: HashMap<String, (u64, SystemTime)>) -> Result<()> {
        if let Index::Writing { documents_path, .. } = self {
            let path = documents_path
                .join(LITT_DIRECTORY_NAME)
                .join(CHECK_SUM_MAP_FILENAME);
            std::fs::write(path, serde_json::to_string(&checksum_map)?).map_err(Into::into)
        } else {
            Err(StateError("Writing".to_string()))
        }
    }

    /// Calculates the checksum of a file that consists of the metadata length and last modified time
    fn calculate_checksum(path: &str) -> Result<(String, (u64, SystemTime))> {
        let file = File::open(path)?;
        let metadata = file.metadata()?;
        let modified = metadata.modified()?;

        let result = (path.to_string(), (metadata.len(), modified));
        Ok(result)
    }

    fn checksum_is_equal(path: &str, checksum: Option<&(u64, SystemTime)>) -> Result<bool> {
        if let Some((len, last_modified)) = checksum {
            let file = File::open(path)?;
            let metadata = file.metadata()?;
            let modified = metadata.modified()?;
            Ok(*len == metadata.len() && *last_modified == modified)
        } else {
            Ok(false)
        }
    }

    fn store_page_index(path: &Path, pindex: PageIndex) -> Result<()> {
        // Create reversed index map
        let path = path.with_extension("pageindex");
        let json_str = serde_json::to_string(&pindex)?;
        std::fs::write(path, json_str)?;
        Ok(())
    }

    fn create_page_index(body: &str) -> Result<PageIndex> {
        let mut pindex: PageIndex = HashMap::new();
        let mut i = 0;
        let graphemes: Vec<&str> = body.graphemes(true).collect();
        while i < graphemes.len() {
            let mut buffer: String = "".to_string();
            let mut j = i;
            while j < graphemes.len() {
                if graphemes[j].chars().all(|c| c.is_alphanumeric()) {
                    buffer += graphemes[j];
                } else {
                    pindex
                        .entry(buffer.clone())
                        .or_default()
                        .push((i as u32, j as u32));
                    i = j;
                    break;
                }
                j += 1;
            }
            i += 1;
        }
        Ok(pindex)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use litt_shared::test_helpers::cleanup_dir_and_file;
    use once_cell::sync::Lazy;
    use serial_test::serial;
    use std::panic;

    const TEST_DIR_NAME: &str = "resources";
    const TEST_FILE_PATH: &str = "test.pdf";

    static SEARCH_SCHEMA: Lazy<SearchSchema> = Lazy::new(SearchSchema::default);

    fn teardown() {
        cleanup_dir_and_file(TEST_DIR_NAME, TEST_FILE_PATH);
    }

    fn run_test<T>(test: T)
    where
        T: FnOnce() + panic::UnwindSafe,
    {
        let result = panic::catch_unwind(test);

        teardown();

        assert!(result.is_ok())
    }

    #[test]
    #[serial]
    fn test_create() {
        run_test(|| {
            let index = Index::create(TEST_DIR_NAME, SEARCH_SCHEMA.clone()).unwrap();
            let (index_schema, index_path) = match index {
                Index::Writing {
                    schema,
                    documents_path,
                    ..
                } => (schema, documents_path),
                Index::Reading { .. } => panic!("Wrong index state"),
            };
            assert_eq!(SEARCH_SCHEMA.clone().schema, index_schema.schema);
            assert_eq!(PathBuf::from(TEST_DIR_NAME), index_path);
        });
    }

    #[test]
    #[serial]
    fn test_open_or_create() {
        run_test(|| {
            Index::create(TEST_DIR_NAME, SEARCH_SCHEMA.clone()).unwrap();

            let opened_index = Index::open_or_create(TEST_DIR_NAME, SEARCH_SCHEMA.clone()).unwrap();

            let (index_schema, index_path) = match opened_index {
                Index::Reading {
                    schema,
                    documents_path,
                    ..
                } => (schema, documents_path),
                Index::Writing { .. } => panic!("Wrong index state"),
            };

            assert_eq!(SEARCH_SCHEMA.clone().schema, index_schema.schema);
            assert_eq!(PathBuf::from(TEST_DIR_NAME), index_path);
            assert!(Path::new(TEST_DIR_NAME)
                .join(LITT_DIRECTORY_NAME)
                .join(INDEX_DIRECTORY_NAME)
                .is_dir())
        });
    }

    #[test]
    fn test_() {
        let text = "Hello*&%&^%, beautiful\n\rWörld!";
        let result = Index::create_page_index(text).unwrap_or_default();
        assert!(result.contains_key("Hello"));
        assert!(result.contains_key("beautiful"));
        assert!(result.contains_key("Wörld"));
    }
}

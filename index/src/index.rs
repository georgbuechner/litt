use tantivy::schema::{STORED, TEXT};

use crate::{LittIndexError, Result};
use crate::LittIndexError::{PdfParseError, WriteError};
use lopdf::Document as PdfDocument;
use tantivy::{IndexWriter, Index as TantivyIndex};
use tantivy::schema::{Document as TantivyDocument, Schema};
use walkdir::{DirEntry, WalkDir};

pub struct Index {
    path: String,
    index: TantivyIndex,
    schema: Schema,
}

impl Index {
    pub fn new(path: &str) -> Result<Self> {
        let schema = Self::build_schema();
        let index = Self::create_index(path.to_string(), schema.clone())?;
        Ok(Self {
            path: path.to_string(),
            index,
            schema,
        })
    }

    /// Add all PDF documents in located in the path this index was created for (see [new()](Self::new)).
    pub fn add_all_documents(&self) -> Result<()> {
        let mut index_writer = self.index.writer(100_000_000)
            .map_err(|e| WriteError(e.to_string()))?;

        let pdf_paths_with_document_results = self.load_pdf_docs();

        for (path, pdf_document_result) in pdf_paths_with_document_results {
            match pdf_document_result {
                Err(e) => {
                    eprintln!("Error reading document ({}): {}", path, e)
                }
                Ok(pdf_document) => {
                    // TODO extract title from file name?
                    self.add_document(&mut index_writer, pdf_document, path)?;
                }
            }
        }

        // We need to call .commit() explicitly to force the
        // index_writer to finish processing the documents in the queue,
        // flush the current index to the disk, and advertise
        // the existence of new documents.

        index_writer.commit().map_err(|e| WriteError(e.to_string()))?;
        Ok(())
    }

    fn build_schema() -> Schema {
        let mut schema_builder = Schema::builder();
        schema_builder.add_text_field("title", TEXT | STORED);
        schema_builder.add_text_field("page", TEXT | STORED);
        schema_builder.add_text_field("body", TEXT);
        schema_builder.build()
    }

    fn create_index(path: String, schema: Schema) -> Result<TantivyIndex> {
        TantivyIndex::create_in_dir(path, schema)
            .map_err(|e| LittIndexError::CreationError(e.to_string()))
    }

    fn get_pdf_dir_entries(&self) -> Vec<DirEntry> {
        let walk_dir = WalkDir::new(self.path.clone());
            walk_dir.follow_links(true)
            .into_iter()
            .filter_map(|entry_result| entry_result.ok())
            .filter(|entry| {
                entry.file_name().to_string_lossy().ends_with("pdf")})
            .collect::<Vec<_>>()
    }

    fn load_pdf_docs(&self) -> Vec<(String, Result<PdfDocument>)> {
        let dir_entries = self.get_pdf_dir_entries();
        let pdf_paths_with_document_results = dir_entries
            .iter()
            .map(|dir_entry| {
                let pdf_path = dir_entry.path().to_owned();
                (dir_entry.file_name().to_string_lossy().to_string(), PdfDocument::load(pdf_path)
                .map_err(|e| PdfParseError(e.to_string())))
            })
            .collect::<Vec<(String, std::result::Result<PdfDocument, _>)>>();
        pdf_paths_with_document_results
    }

    fn add_document(&self, index_writer: &mut IndexWriter, pdf_document: PdfDocument, title: String) -> Result<()> {
        // Let's index one documents!
        println!("Indexing document");
        let mut tantivy_document = TantivyDocument::new();

        for (i, _) in pdf_document.page_iter().enumerate() {
            let page_number = i as u32 + 1;
             for (field, entry) in self.schema.fields() {
                match entry.name() {
                    "title" => tantivy_document.add_text(field, title.clone()),
                    "page" => tantivy_document.add_text(field, page_number),
                    "body" =>
                        tantivy_document.add_text(
                            field,
                            pdf_document.extract_text(&[page_number])
                                .map_err(|e| PdfParseError(e.to_string()))?),
                    _ => {}
                }
            }
        }

        index_writer
            .add_document(tantivy_document)
            .map_err(|e| WriteError(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create new index with mock methods, asserting that
    /// * [SchemaBuilder::add_text_field()](tantivy::schema::SchemaBuilder::add_text_field) is called 3 times
    /// * [SchemaBuilder::build()](tantivy::schema::SchemaBuilder::build) is called once
    /// * [index::create_in_dir()](tantivy::Index::create_in_dir) is called once
    #[test]
    fn test_new() {
        Index::new("test").unwrap();
    }

    #[test]
    fn test_get_all_documents() {
        let index = Index::new("test").unwrap();
        index.add_all_documents().unwrap();
    }
}

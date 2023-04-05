use std::panic;
use std::fs::{create_dir_all, remove_dir_all, remove_file};

use lopdf::Document;
use tantivy::schema::{Schema, TEXT, STORED};
use tantivy::{Index, doc};
use litt_shared::test_helpers::generate_fake_pdf_document;
extern crate litt_search;
use litt_search::search::{Search, SearchSchema};

const TEST_DIR_NAME: &str = "resources";
const TEST_FILE_NAME: &str = "test";
const TEST_FILE_PATH: &str = "test.pdf";

fn setup() {
    create_dir_all(TEST_DIR_NAME).expect(&*format!("Failed to create directory: {}", TEST_DIR_NAME));
    let mut doc = generate_fake_pdf_document();
    doc.save(TEST_FILE_PATH).expect(&*format!("Failed to save test document: {}", TEST_FILE_NAME));
}

fn teardown() {
    remove_dir_all(TEST_DIR_NAME).expect(&*format!("Failed to remove directory: {}", TEST_DIR_NAME));
    remove_file(TEST_FILE_PATH).expect(&*format!("Failed to save test document: {}", TEST_FILE_NAME));
}

fn run_test<T>(test: T) -> ()
    where T: FnOnce() -> () + panic::UnwindSafe {
    setup();

    let result = panic::catch_unwind(|| {
        test()
    });

    teardown();

    assert!(result.is_ok())
}


#[test]
fn test_index_and_search() {
    run_test(|| {
        println!("--- LITT ---");

        println!("Parsing document");
        let doc = Document::load(TEST_FILE_PATH).unwrap();

        // First we need to define a schema ...

        // `TEXT` means the field should be tokenized and indexed,
        // along with its term frequency and term positions.
        //
        // `STORED` means that the field will also be saved
        // in a compressed, row-oriented key-value store.
        // This store is useful to reconstruct the
        // documents that were selected during the search phase.
        let mut schema_builder = Schema::builder();
        let title = schema_builder.add_text_field("title", TEXT | STORED);
        let path = schema_builder.add_text_field("path", TEXT | STORED);
        let page = schema_builder.add_u64_field("page", STORED);
        let body = schema_builder.add_text_field("body", TEXT);
        let schema = schema_builder.build();

        // Indexing documents
        let index_path = String::from(TEST_DIR_NAME);
        let index = Index::create_in_dir(index_path, schema.clone()).unwrap();

        // Here we use a buffer of 100MB that will be split
        // between indexing threads.
        let mut index_writer = index.writer(100_000_000).unwrap();

        // Let's index one documents!
        println!("Indexing document");
        // Fake document has just 1 page
        const PAGE: u32 = 1;
        let text = doc.extract_text(&[PAGE]).unwrap();
        index_writer.add_document(doc!(
            title => TEST_FILE_NAME,
            path => TEST_FILE_PATH,
            page => u64::from(PAGE),
            body => text)
        ).unwrap();



        // We need to call .commit() explicitly to force the
        // index_writer to finish processing the documents in the queue,
        // flush the current index to the disk, and advertise
        // the existence of new documents.
        index_writer.commit().unwrap();

        // # Searching

        // init search
        let search_scheama = SearchSchema::new(title, path, page, body).unwrap();
        let search = Search::new(index, search_scheama).unwrap();

        // do seach: expect 1 results
        let searched_word = String::from("Hello");
        let results = search.search(&searched_word).unwrap();

        for (title, pages) in &results {
            println!("\"{}\". Pages: {:?}", title, pages);
            for page in pages {
                let text = doc.extract_text(&[*page]).unwrap();
                let preview_index = text.find(&searched_word).expect("Searched word not found on page!");
                let start = if preview_index > 50 { preview_index - 50 } else { 0 };
                let end = if (preview_index + searched_word.len() + 50) < text.len() { preview_index + searched_word.len() + 50 } else { text.len() };
                let preview = &text[start..end];
                println!("- {}: \"{}\"", page, preview);
            }
        }

        println!("Found \"{}\" in {} documents: ", searched_word, results.len());

        assert!(results.contains_key(TEST_FILE_NAME));
        assert_eq!(results.get(TEST_FILE_NAME).unwrap().len(), 1);
    });
}

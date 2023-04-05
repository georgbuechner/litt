use std::panic;

use lopdf::Document;

use litt_shared::test_helpers::{save_fake_pdf_document, cleanup_dir_and_file};
extern crate litt_search;
use litt_search::search::{Search};
use litt_index::index::Index;
use litt_shared::search_schema::SearchSchema;

const TEST_DIR_NAME: &str = "resources";
const TEST_FILE_NAME: &str = "test";
const TEST_FILE_PATH: &str = "test.pdf";

#[test]
fn test_index_and_search() {
    run_test(|| {
        let doc = Document::load(format!("{}/{}", TEST_DIR_NAME, TEST_FILE_PATH)).unwrap();

        println!("--- LITT ---");
        let search_schema = SearchSchema::default();

        let index = Index::create(TEST_DIR_NAME, search_schema.clone()).unwrap();
        index.add_all_documents().unwrap();

        // # Searching

        // init search
        let search = Search::new(index.index(), search_schema).unwrap();

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

fn setup() {
    save_fake_pdf_document(TEST_DIR_NAME, TEST_FILE_PATH);
}

fn teardown() {
    cleanup_dir_and_file(TEST_DIR_NAME, TEST_FILE_PATH);
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

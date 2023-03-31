use std::collections::{HashMap, LinkedList};
use std::panic;
use std::fs::{create_dir_all, remove_dir_all, remove_file};

use lopdf::Document;
use tantivy::schema::{Schema, TEXT, STORED};
use tantivy::query::QueryParser;
use tantivy::{Index, doc, Score, DocAddress};
use tantivy::collector::TopDocs;
use crate::helpers::generate_fake_pdf_document;

mod helpers;

const TEST_DIR_NAME: &str = "resources";
const TEST_FILE_NAME: &str = "test.pdf";

fn setup() {
    create_dir_all(TEST_DIR_NAME).expect(&*format!("Failed to create directory: {}", TEST_DIR_NAME));
    let mut doc = generate_fake_pdf_document();
    doc.save(TEST_FILE_NAME).expect(&*format!("Failed to save test document: {}", TEST_FILE_NAME));
}

fn teardown() {
    remove_dir_all(TEST_DIR_NAME).expect(&*format!("Failed to remove directory: {}", TEST_DIR_NAME));
    remove_file(TEST_FILE_NAME).expect(&*format!("Failed to save test document: {}", TEST_FILE_NAME));
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
        let doc = Document::load("test.pdf").unwrap();

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
        let page = schema_builder.add_text_field("page", TEXT | STORED);
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
        index_writer.add_document(doc!(title => TEST_FILE_NAME, page => PAGE.to_string(), body => text)).unwrap();

        // We need to call .commit() explicitly to force the
        // index_writer to finish processing the documents in the queue,
        // flush the current index to the disk, and advertise
        // the existence of new documents.
        index_writer.commit().unwrap();

        // # Searching

        let reader = index.reader().unwrap();

        let searcher = reader.searcher();

        let query_parser = QueryParser::for_index(&index, vec![title, body]);

        // QueryParser may fail if the query is not in the right
        // format. For user facing applications, this can be a problem.
        // A ticket has been opened regarding this problem.
        let searched_word = "Hello";
        let query = query_parser.parse_query(searched_word).unwrap();

        println!("searching document");
        // Perform search.
        // `topdocs` contains the 10 most relevant doc ids, sorted by decreasing scores...
        let top_docs: Vec<(Score, DocAddress)> = searcher.search(&query, &TopDocs::with_limit(10)).unwrap();

        // Assemble results
        let mut results: HashMap<String, LinkedList<String>> = HashMap::new();
        for (_score, doc_address) in top_docs {
            // Retrieve the actual content of documents given its `doc_address`.
            let retrieved_doc = searcher.doc(doc_address).unwrap();
            let cur_title = retrieved_doc.get_first(title).unwrap().as_text().unwrap();
            let cur_page = retrieved_doc.get_first(page).unwrap().as_text().unwrap();
            results.entry(cur_title.to_string()).and_modify(|pages| pages.push_back(cur_page.to_string())).or_insert(LinkedList::from([cur_page.to_string()]));
        }

        println!("Found \"{}\" in {} documents: ", searched_word, results.len());
        for (title, pages) in results {
            println!("\"{}\". Pages: {:?}", title, pages);
            for page in pages {
                let p: u32 = page.trim().parse().expect("Page is not a number!");
                let text = doc.extract_text(&[p]).unwrap();
                let preview_index = text.find(&searched_word).expect("Searched word not found on page!");
                let start = if preview_index > 50 { preview_index - 50 } else { 0 };
                let end = if (preview_index + searched_word.len() + 50) < text.len() { preview_index + searched_word.len() + 50 } else { text.len() };
                let preview = &text[start..end];
                println!("- {}: \"{}\"", page, preview);
            }
        }
    });
}

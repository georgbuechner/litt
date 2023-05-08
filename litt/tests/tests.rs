use std::panic;

extern crate litt_search;
use litt_index::index::Index;
use litt_search::search::Search;
use litt_shared::search_schema::SearchSchema;
use litt_shared::test_helpers::cleanup_litt_files;

const TEST_DIR_NAME: &str = "../resources";
const TEST_FILE_NAME: &str = "test.pdf";

#[test]
fn test_index_and_search() {
    run_test(|| {
        let search_schema = SearchSchema::default();

        let mut index = Index::create(TEST_DIR_NAME, search_schema.clone()).unwrap();
        index.add_all_pdf_documents().unwrap();

        // # Searching

        // init search
        let search = Search::new(index, search_schema);

        // do seach: expect 1 results
        let searched_word = String::from("Hello");
        let results = search.search(&searched_word, 0, 10).unwrap();

        for (title, pages) in &results {
            assert_eq!(title, TEST_FILE_NAME);
            for search_result in pages {
                let preview = search.get_preview(search_result, &searched_word).unwrap();
                assert!(!preview.is_empty());
                assert!(
                    preview
                        .to_lowercase()
                        .find(&searched_word.to_lowercase())
                        .unwrap_or_default()
                        > 0
                );
                assert!(preview.find("**").unwrap_or_default() > 0);
            }
        }

        assert!(results.contains_key(TEST_FILE_NAME));
        assert_eq!(results.get(TEST_FILE_NAME).unwrap().len(), 1);
    });
}

fn teardown() {
    cleanup_litt_files(TEST_DIR_NAME)
}

fn run_test<T>(test: T)
where
    T: FnOnce() + panic::UnwindSafe,
{
    let result = panic::catch_unwind(test);

    teardown();

    assert!(result.is_ok())
}

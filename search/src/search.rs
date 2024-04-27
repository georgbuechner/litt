use std::collections::{HashMap, LinkedList};
use std::fs;
use tantivy::collector::TopDocs;
use tantivy::schema::Value;
use tantivy::{DocAddress, Snippet, SnippetGenerator, TantivyDocument};

extern crate litt_index;
use litt_index::index::Index;
use litt_shared::message_display::MessageDisplay;
use litt_shared::search_schema::SearchSchema;

use crate::LittSearchError::SearchError;
use crate::Result;

#[derive(Debug, Clone, Copy)]
#[cfg_attr(test, derive(PartialEq))]
pub struct SearchResult {
    pub page: u32,
    pub score: f32,
    segment_ord: u32,
    doc_id: u32,
}

impl SearchResult {
    pub fn new(page: u32, score: f32, segment_ord: u32, doc_id: u32) -> Self {
        Self {
            page,
            score,
            segment_ord,
            doc_id,
        }
    }
}

pub struct Search<'a, T: MessageDisplay> {
    index: Index<'a, T>,
    schema: SearchSchema,
}

pub enum SearchTerm {
    Fuzzy(String, u8),
    Exact(String),
}

impl<'a, T: MessageDisplay> Search<'a, T> {
    pub fn new(index: Index<'a, T>, schema: SearchSchema) -> Self {
        Self { index, schema }
    }

    pub fn search(
        &self,
        input: &SearchTerm,
        offset: usize,
        limit: usize,
    ) -> Result<HashMap<String, LinkedList<SearchResult>>> {
        let searcher = self
            .index
            .searcher()
            .map_err(|e| SearchError(e.to_string()))?;

        let (query_parser, term) = match input {
            SearchTerm::Fuzzy(term, distance) => {
                let mut query_parser = self
                    .index
                    .query_parser()
                    .map_err(|e| SearchError(e.to_string()))?;
                query_parser.set_field_fuzzy(self.schema.body, true, *distance, true);
                (query_parser, term)
            }
            SearchTerm::Exact(term) => (
                self.index
                    .query_parser()
                    .map_err(|e| SearchError(e.to_string()))?,
                term,
            ),
        };

        let query = query_parser
            .parse_query(term)
            .map_err(|e| SearchError(e.to_string()))?;
        let top_docs = searcher
            .search(&query, &TopDocs::with_limit(limit).and_offset(offset))
            .map_err(|e| SearchError(e.to_string()))?;

        // Assemble results
        let mut results: HashMap<String, LinkedList<SearchResult>> = HashMap::new();

        for (score, doc_address) in top_docs {
            let segment_ord = doc_address.segment_ord;
            let doc_id = doc_address.doc_id;
            // Retrieve the actual content of documents given its `doc_address`.
            let retrieved_doc: TantivyDocument = searcher
                .doc(doc_address)
                .map_err(|e| SearchError(e.to_string()))?;
            let cur_title = retrieved_doc
                .get_first(self.schema.title)
                .ok_or(SearchError(String::from(
                    "Fatal: Field \"title\" not found!",
                )))?
                .as_str()
                .ok_or(SearchError(String::from(
                    "Fatal: Field \"title\" could not be read as text!",
                )))?;
            let cur_page = retrieved_doc
                .get_first(self.schema.page)
                .ok_or(SearchError(String::from(
                    "Fatal: Field \"page\" not found!",
                )))?
                .as_u64()
                .ok_or(SearchError(String::from(
                    "Fatal: Field \"page\" not a number!",
                )))?;
            let page: u32 = cur_page.try_into().map_err(|_| {
                SearchError(format!(
                    "Fatal: Field \"page\" ({}) is bigger than u32!",
                    cur_page
                ))
            })?;
            let search_result = SearchResult::new(page, score, segment_ord, doc_id);
            results
                .entry(cur_title.to_string())
                .and_modify(|pages| pages.push_back(search_result))
                .or_insert_with(|| LinkedList::from([search_result]));
        }
        Ok(results)
    }

    pub fn get_preview(
        &self,
        search_result: &SearchResult,
        search_term: &SearchTerm,
    ) -> Result<String> {
        // Prepare creating snippet.
        let searcher = self
            .index
            .searcher()
            .map_err(|e| SearchError(e.to_string()))?;
        let (query_parser, term) = match search_term {
            SearchTerm::Fuzzy(_, _) => return Ok("[fuzzy match] No preview. We're sry.".into()),
            SearchTerm::Exact(term) => (self.index.query_parser(), term),
        };
        let query = query_parser
            .map_err(|e| SearchError(e.to_string()))?
            .parse_query(term)
            .map_err(|e| SearchError(e.to_string()))?;
        let mut snippet_generator = SnippetGenerator::create(&searcher, &*query, self.schema.body)
            .map_err(|e| SearchError(e.to_string()))?;
        snippet_generator.set_max_num_chars(70);
        let retrieved_doc: TantivyDocument = searcher
            .doc(DocAddress {
                segment_ord: (search_result.segment_ord),
                doc_id: (search_result.doc_id),
            })
            .map_err(|e| SearchError(e.to_string()))?;

        // Get text on given page
        let path = retrieved_doc
            .get_first(self.schema.path)
            .ok_or(SearchError(String::from(
                "Fatal: Field \"path\" not found!",
            )))?
            .as_str()
            .ok_or(SearchError(String::from(
                "Fatal: Field \"path\" could not be read as text!",
            )))?;
        let text = fs::read_to_string(path).map_err(|e| SearchError(e.to_string()))?;

        // Generate snippet
        let snippet = snippet_generator.snippet(&text);
        // let snippet = snippet_generator.snippet_from_doc(&retrieved_doc);
        Ok(self.highlight(snippet))
    }

    fn highlight(&self, snippet: Snippet) -> String {
        let mut result = String::new();
        let mut start_from = 0;

        for fragment_range in snippet.highlighted() {
            result.push_str(&snippet.fragment()[start_from..fragment_range.start]);
            result.push_str(" **");
            result.push_str(&snippet.fragment()[fragment_range.clone()]);
            result.push_str("** ");
            start_from = fragment_range.end;
        }

        result.push_str(&snippet.fragment()[start_from..]);
        result.replace('\n', " ")
    }
}

#[cfg(test)]
mod tests {
    use litt_shared::message_display::SimpleMessageDisplay;
    use std::panic;

    use litt_shared::test_helpers::cleanup_litt_files;

    use super::*;
    const TEST_DIR_NAME: &str = "../resources";
    const TEST_DOC_NAME: &str = "test.pdf";

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

    fn create_searcher<T: MessageDisplay>(message_display: &T) -> Result<Search<T>> {
        let search_schema = SearchSchema::default();
        let index =
            Index::open_or_create(TEST_DIR_NAME, search_schema.clone(), message_display).unwrap();
        let readable_index = index
            .add_all_documents()
            .map_err(|e| SearchError(e.to_string()))?;
        let searcher = readable_index
            .searcher()
            .map_err(|e| SearchError(e.to_string()))?;
        println!("loaded {} document pages.", searcher.num_docs());
        Ok(Search::new(readable_index, search_schema))
    }

    #[test]
    fn test_search() {
        run_test(|| {
            let message_display = SimpleMessageDisplay;
            let search = create_searcher(&message_display).unwrap();
            test_normal_search(&search);
            test_fuzzy_search(&search);
            test_limit_and_offset(&search);
        })
    }

    fn test_normal_search(search: &Search<SimpleMessageDisplay>) {
        let test_cases: HashMap<&str, Vec<u32>> = HashMap::from([
            ("flooding", vec![2]),
            ("the", vec![1, 2]),
            ("river flooding", vec![1, 2]),
            ("river OR flooding", vec![1, 2]),
            ("river AND flooding", vec![2]),
            ("(river OR valley) AND flooding", vec![2]),
            ("lley si", vec![]),
            ("CARRYING'", vec![2]),
            ("Bär", vec![1]),
            ("Bär Hündin", vec![1]),
            ("\"limbs branches\"", vec![]),
            ("\"limbs branches\"~1", vec![2]),
            ("\"of Sole\"*", vec![1]),
        ]);
        // one-word search returning 1 result with 1 page
        for (search_term, pages) in &test_cases {
            println!("- [exact] searching {}.", search_term);
            let results = search
                .search(&SearchTerm::Exact(search_term.to_string()), 0, 10)
                .unwrap();
            if !pages.is_empty() {
                assert!(results.contains_key(TEST_DOC_NAME));
                let doc_results = results.get(TEST_DOC_NAME).unwrap();
                assert_eq!(pages.len(), doc_results.len());
                for page in pages {
                    assert!(doc_results.iter().any(|result| result.page == *page));
                }
            } else {
                assert!(!results.contains_key(TEST_DOC_NAME));
            }
        }
    }

    fn test_fuzzy_search(search: &Search<SimpleMessageDisplay>) {
        let test_cases: HashMap<&str, Vec<u32>> = HashMap::from([
            ("Hündin", vec![1]),
            ("flooding", vec![2]),
            ("river", vec![1, 2]),
            ("branch", vec![2]),
            ("branch Sole", vec![1, 2]),
            // ("branch Sole", vec![2]), // Does not work. finds Soledad @ page 1
            // ("branch Sole", vec![1]), // Does not work. finds branches @ page 1
            ("Soledad", vec![1]),
            ("Soledud Salinos", vec![1]), // actual fuzzy
                                          // ("Sole AND Sali", vec![1]), // Does not work: searching for ['sole' 'and', 'sali']
        ]);
        // one-word search returning 1 result with 1 page
        for (search_term, pages) in &test_cases {
            println!("- [fuzzy] searching {}.", search_term);
            let results = search
                .search(&SearchTerm::Fuzzy(search_term.to_string(), 2), 0, 10)
                .unwrap();
            if !pages.is_empty() {
                assert!(results.contains_key(TEST_DOC_NAME));
                let doc_results = results.get(TEST_DOC_NAME).unwrap();
                assert_eq!(pages.len(), doc_results.len());
                for page in pages {
                    assert!(doc_results.iter().any(|result| result.page == *page));
                }
            } else {
                assert!(!results.contains_key(TEST_DOC_NAME));
            }
        }
    }

    fn test_limit_and_offset(search: &Search<SimpleMessageDisplay>) {
        // river is contained twice
        let results = search
            .search(&SearchTerm::Exact(String::from("river")), 0, 10)
            .unwrap();
        assert_eq!(results.get(TEST_DOC_NAME).unwrap().len(), 2);
        // By changing limit only one results left:
        let results = search
            .search(&SearchTerm::Exact(String::from("river")), 0, 1)
            .unwrap();
        assert_eq!(results.get(TEST_DOC_NAME).unwrap().len(), 1);
        // Same result when changing offset:
        let results = search
            .search(&SearchTerm::Exact(String::from("river")), 1, 10)
            .unwrap();
        assert_eq!(results.get(TEST_DOC_NAME).unwrap().len(), 1);
        // First match has higher score than second:
        let results = search
            .search(&SearchTerm::Exact(String::from("river")), 0, 10)
            .unwrap();
        assert_eq!(results.get(TEST_DOC_NAME).unwrap().len(), 2);
        assert!(
            results.get(TEST_DOC_NAME).unwrap().front().unwrap().score
                >= results.get(TEST_DOC_NAME).unwrap().back().unwrap().score
        );
    }
}

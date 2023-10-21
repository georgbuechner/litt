use std::collections::{HashMap, LinkedList};
use std::fs;
use tantivy::collector::TopDocs;
use tantivy::{DocAddress, Score, Snippet, SnippetGenerator, Term};
use tantivy::query::FuzzyTermQuery;

extern crate litt_index;
use litt_index::index::Index;
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

pub struct Search {
    index: Index,
    schema: SearchSchema,
}

pub enum SearchTerm {
    Fuzzy(&'static str),
    Exact(&'static str),
}

impl Search {
    pub fn new(index: Index, schema: SearchSchema) -> Self {
        Self { index, schema }
    }

    pub fn search(
        &self,
        input: SearchTerm,
        offset: usize,
        limit: usize,
    ) -> Result<HashMap<String, LinkedList<SearchResult>>> {
        let searcher = self.index.searcher();
        let query_parser = self.index.query_parser();

        let top_docs = match input {
            SearchTerm::Fuzzy(_) => {}
            SearchTerm::Exact(term) => {
                let query = query_parser
                    .parse_query(term)
                    .map_err(|e| SearchError(e.to_string()))?;
                    searcher
                        .search(&query, &TopDocs::with_limit(limit).and_offset(offset))
                        .map_err(|e| SearchError(e.to_string()))?;

            }
        };

        // Assemble results
        let mut results: HashMap<String, LinkedList<SearchResult>> = HashMap::new();



        for (score, doc_address) in top_docs {
            let segment_ord = doc_address.segment_ord;
            let doc_id = doc_address.doc_id;
            // Retrieve the actual content of documents given its `doc_address`.
            let retrieved_doc = searcher
                .doc(doc_address)
                .map_err(|e| SearchError(e.to_string()))?;
            let cur_title = retrieved_doc
                .get_first(self.schema.title)
                .ok_or(SearchError(String::from(
                    "Fatal: Field \"title\" not found!",
                )))?
                .as_text()
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

    pub fn get_preview(&self, search_result: &SearchResult, input: &str) -> Result<String> {
        // Prepare creating snippet.
        let searcher = self.index.searcher();
        let query_parser = self.index.query_parser();
        let query = query_parser
            .parse_query(input)
            .map_err(|e| SearchError(e.to_string()))?;
        let mut snippet_generator = SnippetGenerator::create(&searcher, &*query, self.schema.body)
            .map_err(|e| SearchError(e.to_string()))?;
        snippet_generator.set_max_num_chars(70);
        let retrieved_doc = searcher
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
            .as_text()
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

    fn create_searcher() -> Search {
        let search_schema = SearchSchema::default();
        let mut index = Index::open_or_create(TEST_DIR_NAME, search_schema.clone()).unwrap();
        index.add_all_pdf_documents().unwrap();
        println!("loaded {} document pages.", &index.searcher().num_docs());
        Search::new(index, search_schema)
    }

    #[test]
    fn test_search() {
        run_test(|| {
            let search = create_searcher();
            test_normal_search(&search);
            test_limit_and_offset(&search);
        })
    }

    fn test_normal_search(search: &Search) {
        let test_cases: HashMap<&str, Vec<u32>> = HashMap::from([
            ("flooding", vec![2]),
            ("the", vec![1, 2]),
            ("river flooding", vec![1, 2]),
            ("river OR flooding", vec![1, 2]),
            ("river AND flooding", vec![2]),
            ("(river OR valley) AND flooding", vec![2]),
            ("lley si'", vec![]),
            ("CARRYING'", vec![2]),
            ("\"limbs branches\"", vec![]),
            ("\"limbs branches\"~1", vec![2]),
            ("Bär", vec![1]),
            ("Bär Hündin", vec![1]),
        ]);
        // one-word search returning 1 result with 1 page
        for (search_term, pages) in &test_cases {
            let results = search.search(search_term, 0, 10).unwrap();
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

    fn test_limit_and_offset(search: &Search) {
        // river is contained twice
        let results = search.search(&String::from("river"), 0, 10).unwrap();
        assert_eq!(results.get(TEST_DOC_NAME).unwrap().len(), 2);
        // By changing limit only one results left:
        let results = search.search(&String::from("river"), 0, 1).unwrap();
        assert_eq!(results.get(TEST_DOC_NAME).unwrap().len(), 1);
        // Same result when changing offset:
        let results = search.search(&String::from("river"), 1, 10).unwrap();
        assert_eq!(results.get(TEST_DOC_NAME).unwrap().len(), 1);
        // First match has higher score than second:
        let results = search.search(&String::from("river"), 0, 10).unwrap();
        assert_eq!(results.get(TEST_DOC_NAME).unwrap().len(), 2);
        assert!(
            results.get(TEST_DOC_NAME).unwrap().front().unwrap().score
                >= results.get(TEST_DOC_NAME).unwrap().back().unwrap().score
        );
    }
}

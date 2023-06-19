use std::collections::{HashMap, LinkedList};
use std::fs;
use tantivy::collector::TopDocs;
use tantivy::{DocAddress, Score, Snippet, SnippetGenerator};

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

impl Search {
    pub fn new(index: Index, schema: SearchSchema) -> Self {
        Self { index, schema }
    }

    pub fn search(
        &self,
        input: &str,
        offset: usize,
        limit: usize,
    ) -> Result<HashMap<String, LinkedList<SearchResult>>> {
        let searcher = self.index.searcher();
        let query_parser = self.index.query_parser();

        let query = query_parser
            .parse_query(input)
            .map_err(|e| SearchError(e.to_string()))?;

        // Perform search.
        // `topdocs` contains the 10 most relevant doc ids, sorted by decreasing scores...
        let top_docs: Vec<(Score, DocAddress)> = searcher
            .search(&query, &TopDocs::with_limit(limit).and_offset(offset))
            .map_err(|e| SearchError(e.to_string()))?;

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
        result.replace('\n', "")
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
        })
    }

    fn test_normal_search(search: &Search) {
        // one-word search returning 1 result with 1 page
        let results = search.search(&String::from("flooding"), 0, 10).unwrap();
        println!("Got {} results.", results.len());
        assert!(results.contains_key(TEST_DOC_NAME));
        assert_eq!(1, results.get(TEST_DOC_NAME).unwrap().len());
        let first_result = results.get(TEST_DOC_NAME).unwrap().front().unwrap();
        assert_eq!(2, first_result.page);

        // one-word search returning 1 result with two pages
        let results = search.search(&String::from("the"), 0, 10).unwrap();
        assert!(results.contains_key(TEST_DOC_NAME));
        assert_eq!(results.get(TEST_DOC_NAME).unwrap().len(), 2);

        // two-word search returning 1 result with two pages
        let results = search
            .search(&String::from("river flooding"), 0, 10)
            .unwrap();
        assert!(results.contains_key(TEST_DOC_NAME));
        assert_eq!(results.get(TEST_DOC_NAME).unwrap().len(), 2);

        // two-word search returning 1 result with two pages
        let id_results = search
            .search(&String::from("river OR flooding"), 0, 10)
            .unwrap();
        assert_eq!(id_results, results);

        // two-word search returning 1 result with two pages
        let results = search
            .search(&String::from("river AND flooding"), 0, 10)
            .unwrap();
        assert!(results.contains_key(TEST_DOC_NAME));
        assert_eq!(results.get(TEST_DOC_NAME).unwrap().len(), 1);
        assert_eq!(results.get(TEST_DOC_NAME).unwrap().front().unwrap().page, 2);

        let results = search
            .search(&String::from("(river OR valley) AND flooding"), 0, 10)
            .unwrap();
        assert!(results.contains_key(TEST_DOC_NAME));
        assert_eq!(results.get(TEST_DOC_NAME).unwrap().len(), 1);
        assert_eq!(results.get(TEST_DOC_NAME).unwrap().front().unwrap().page, 2);

        // Cannot find part of words like 'lley si' for 'valley side'
        let results = search.search(&String::from("lley si'"), 0, 10).unwrap();
        assert!(!results.contains_key(TEST_DOC_NAME));

        // Check caseinsensitive
        let results = search.search(&String::from("CARRYING"), 0, 10).unwrap();
        assert!(results.contains_key(TEST_DOC_NAME));
        assert_eq!(results.get(TEST_DOC_NAME).unwrap().len(), 1);
        assert_eq!(results.get(TEST_DOC_NAME).unwrap().front().unwrap().page, 2);

        // Phrase query should not be found (since only "limbs and branches" exists)
        let results = search
            .search(&String::from("\"limbs branches\""), 0, 10)
            .unwrap();
        assert!(!results.contains_key(TEST_DOC_NAME));

        // Phrase query should be found with higher slop
        let results = search
            .search(&String::from("\"limbs branches\"~1"), 0, 10)
            .unwrap();
        assert!(results.contains_key(TEST_DOC_NAME));

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

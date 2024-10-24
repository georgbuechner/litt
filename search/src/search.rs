use std::collections::{HashMap, LinkedList};
use std::fs;
use tantivy::collector::TopDocs;
use tantivy::schema::Value;
use tantivy::{DocAddress, Snippet, SnippetGenerator, TantivyDocument};

extern crate litt_index;
use litt_index::index::{Index, PageIndex};
use litt_shared::search_schema::SearchSchema;

use crate::LittSearchError::SearchError;
use crate::Result;

use levenshtein::levenshtein;

const FUZZY_PREVIEW_NOT_FOUND: &str = "[fuzzy match] No preview. We're sry.";

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
    Fuzzy(String, u8),
    Exact(String),
}

fn get_first_term(query: &str) -> String {
    let parts = query.split(' ').collect::<Vec<_>>();
    if let Some(first_str) = parts.first() {
        if let Some(stripped) = first_str.strip_prefix('\"') {
            return stripped.to_string();
        }
        first_str.to_string()
    } else {
        "".to_string()
    }
}

impl Search {
    pub fn new(index: Index, schema: SearchSchema) -> Self {
        Self { index, schema }
    }

    pub fn search(
        &self,
        input: &SearchTerm,
        offset: usize,
        limit: usize,
    ) -> Result<HashMap<String, LinkedList<SearchResult>>> {
        let searcher = self.index.searcher()?;

        let (query_parser, term) = match input {
            SearchTerm::Fuzzy(term, distance) => {
                let mut query_parser = self.index.query_parser()?;
                query_parser.set_field_fuzzy(self.schema.body, true, *distance, true);
                (query_parser, term)
            }
            SearchTerm::Exact(term) => (self.index.query_parser()?, term),
        };

        let query = query_parser.parse_query(term)?;
        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit).and_offset(offset))?;

        // Assemble results
        let mut results: HashMap<String, LinkedList<SearchResult>> = HashMap::new();

        for (score, doc_address) in top_docs {
            let segment_ord = doc_address.segment_ord;
            let doc_id = doc_address.doc_id;
            // Retrieve the actual content of documents given its `doc_address`.
            let retrieved_doc: TantivyDocument = searcher.doc(doc_address)?;
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
    ) -> Result<(String, String)> {
        // Prepare creating snippet.
        let searcher = self.index.searcher()?;
        let retrieved_doc: TantivyDocument = searcher.doc(DocAddress {
            segment_ord: (search_result.segment_ord),
            doc_id: (search_result.doc_id),
        })?;

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
        let text = fs::read_to_string(path)?;

        match search_term {
            SearchTerm::Fuzzy(term, distance) => {
                for t in term.split(" ").collect::<Vec<&str>>() {
                    if let Ok((prev, matched_term)) =
                        self.get_fuzzy_preview(path, t, distance, &text)
                    {
                        return Ok((prev, matched_term.to_string()));
                    }
                }
                Ok((FUZZY_PREVIEW_NOT_FOUND.to_string(), "".to_string())) // return empty string so
                                                                          // that zathura does not
                                                                          // search
            }
            SearchTerm::Exact(term) => self.get_preview_from_query(term, text),
        }
    }

    fn get_preview_from_query(&self, term: &str, text: String) -> Result<(String, String)> {
        let searcher = self.index.searcher()?;
        let query = self.index.query_parser()?.parse_query(term)?;
        let mut snippet_generator = SnippetGenerator::create(&searcher, &*query, self.schema.body)
            .map_err(|e| SearchError(e.to_string()))?;
        snippet_generator.set_max_num_chars(70);

        let snippet = snippet_generator.snippet(&text);
        // let snippet = snippet_generator.snippet_from_doc(&retrieved_doc);
        Ok((self.highlight(snippet), get_first_term(term)))
    }

    fn get_fuzzy_preview(
        &self,
        path: &str,
        term: &str,
        distance: &u8,
        body: &str,
    ) -> Result<(String, String)> {
        let pindex: PageIndex = self
            .index
            .page_index(path)
            .map_err(|_| SearchError("".to_string()))?;
        let (matched_term, start, end) = self
            .get_fuzzy_match(term, distance, pindex)
            .map_err(|_| SearchError("".to_string()))?;
        // Another safe way to get substrings using char_indices
        let start = body
            .char_indices()
            .nth(start.saturating_sub(20) as usize)
            .unwrap_or((0, ' '))
            .0;
        let end = body
            .char_indices()
            .nth((end + 20) as usize)
            .unwrap_or((body.len() - 1, ' '))
            .0;
        let substring = &format!("...{}...", &body[start..end]);
        let substring = substring
            .to_string()
            .replace(&matched_term, &format!("**{}**", matched_term));
        Ok((substring.replace('\n', " "), matched_term))
    }

    fn get_fuzzy_match(
        &self,
        term: &str,
        distance: &u8,
        pindex: PageIndex,
    ) -> Result<(String, u32, u32)> {
        if pindex.contains_key(term) {
            let (start, end) = pindex.get(term).unwrap().first().unwrap();
            Ok((term.to_string(), *start, *end))
        } else {
            let mut cur: (String, u32, u32) = ("".to_string(), 0, 0);
            let mut min_dist: usize = usize::MAX;
            for (word, matches) in pindex {
                let dist: usize = if word.contains(term) {
                    1
                } else {
                    levenshtein(term, &word)
                };
                if dist < min_dist {
                    min_dist = dist;
                    let (start, end) = matches.first().unwrap_or(&(0, 0));
                    cur = (word.to_string(), *start, *end)
                }
            }
            if min_dist as u8 <= *distance {
                Ok(cur)
            } else {
                Err(SearchError("".to_string()))
            }
        }
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

    fn create_searcher() -> Result<Search> {
        let search_schema = SearchSchema::default();
        let index = Index::open_or_create(TEST_DIR_NAME, search_schema.clone()).unwrap();
        let readable_index = index.add_all_documents()?;
        let searcher = readable_index.searcher()?;
        println!("loaded {} document pages.", searcher.num_docs());
        Ok(Search::new(readable_index, search_schema))
    }

    #[test]
    fn test_search() {
        run_test(|| {
            let search = create_searcher().unwrap();
            test_normal_search(&search);
            test_fuzzy_search(&search);
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
            ("lley si", vec![]),
            ("CARRYING'", vec![2]),
            ("Bär", vec![1]),
            ("Bär Hündin", vec![1]),
            ("\"limbs branches\"", vec![]),
            ("\"limbs branches\"~1", vec![2]),
            ("\"of Sole\"*", vec![1]),
            ("Mystifizierung", vec![1, 2]),
            ("Mystifizierungen", vec![1]),
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

    fn test_fuzzy_search(search: &Search) {
        let test_cases: HashMap<&str, Vec<(u32, &str)>> = HashMap::from([
            ("Hello", vec![(1, "World"), (2, FUZZY_PREVIEW_NOT_FOUND)]),
            ("Hündin", vec![(1, "Bär")]),
            ("flooding", vec![(2, "winter’s")]),
            ("river", vec![(1, "drops"), (2, "foothill")]), // search result
            ("branch", vec![(2, "arch")]),
            ("branch Sole", vec![(1, "Salinas River"), (2, "arch")]),
            // ("branch Sole", vec![2]), // Does not work. finds Soledad @ page 1
            // ("branch Sole", vec![1]), // Does not work. finds branches @ page 1
            ("Soledad", vec![(1, "Salinas")]),
            ("Soledud", vec![(1, "River")]),
            ("Soledud Salinos", vec![(1, "the")]), // actual fuzzy
            // ("Sole AND Sali", vec![1]), // Does not work: searching for ['sole' 'and', 'sali']
            (
                "mystifiziert",
                vec![(1, "Mystifizierung"), (2, "No preview")],
            ),
        ]);
        // one-word search returning 1 result with 1 page
        for (search_term, pages) in &test_cases {
            println!("- [fuzzy] searching {}.", search_term);
            let t_search_term = &SearchTerm::Fuzzy(search_term.to_string(), 2);
            let results = search.search(t_search_term, 0, 10).unwrap();
            if !pages.is_empty() {
                assert!(results.contains_key(TEST_DOC_NAME));
                let doc_results = results.get(TEST_DOC_NAME).unwrap();
                assert_eq!(pages.len(), doc_results.len());
                for (page, _) in pages {
                    assert!(doc_results.iter().any(|result| result.page == *page));
                }
                for page in doc_results {
                    println!(
                        "Getting preview: {} id:{},{}",
                        page.page, page.doc_id, page.segment_ord
                    );
                    let page_num: u32 = page.page;
                    let preview_part = pages
                        .iter()
                        .find(|&&(first, _)| first == page_num)
                        .map_or("pagenotfound", |&(_, part)| part);
                    let preview = match search.get_preview(page, t_search_term) {
                        Ok((preview, _)) => preview,
                        Err(_) => FUZZY_PREVIEW_NOT_FOUND.to_string(),
                    };
                    println!(
                        "Found preview \"{}\" should contain: {}",
                        preview, preview_part
                    );
                    assert!(preview.contains(preview_part));
                    println!("success");
                }
            } else {
                assert!(!results.contains_key(TEST_DOC_NAME));
            }
        }
    }

    fn test_limit_and_offset(search: &Search) {
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

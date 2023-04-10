use std::collections::{HashMap, LinkedList};
use std::fmt::{Pointer, self};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::{DocAddress, Index, IndexReader, ReloadPolicy, Score, Document, SnippetGenerator};

use litt_shared::search_schema::SearchSchema;

use crate::LittSearchError::{InitError, SearchError};
use crate::Result;

#[derive(Debug, Clone, Copy)]
pub struct SearchResult {
    pub page: u32, 
    segment_ord: u32, 
    doc_id: u32
}

impl  SearchResult {
    pub fn new(page: u32, segment_ord: u32, doc_id: u32) -> Result<Self> {
        Ok(Self {
            page,
            segment_ord,
            doc_id
        })
    }
}

impl PartialEq for SearchResult {
    fn eq(&self, other: &Self) -> bool {
        self.page == other.page 
    }
}


pub struct Search {
    index: Index,
    reader: IndexReader,
    schema: SearchSchema,
}

impl Search {
    pub fn new(index: Index, schema: SearchSchema) -> Result<Self> {
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommit)
            .try_into()
            .map_err(|e| InitError(e.to_string()))?;
        Ok(Self {
            index,
            reader,
            schema,
        })
    }

    pub fn search(&self, input: &str) -> Result<HashMap<String, LinkedList<SearchResult>>> {
        let searcher = self.reader.searcher();
        let query_parser = QueryParser::for_index(&self.index, self.schema.default_fields());

        let query = query_parser
            .parse_query(input)
            .map_err(|e| SearchError(e.to_string()))?;

        // Perform search.
        // `topdocs` contains the 10 most relevant doc ids, sorted by decreasing scores...
        let top_docs: Vec<(Score, DocAddress)> = searcher
            .search(&query, &TopDocs::with_limit(10))
            .map_err(|e| SearchError(e.to_string()))?;

        // Assemble results
        let mut results: HashMap<String, LinkedList<SearchResult>> = HashMap::new();
        for (_score, doc_address) in top_docs {
            let segment_ord = doc_address.segment_ord;
            let doc_id = doc_address.doc_id;
            // Retrieve the actual content of documents given its `doc_address`.
            let retrieved_doc = searcher
                .doc(doc_address)
                .map_err(|e| SearchError(e.to_string()))?;
            let cur_title = retrieved_doc
                .get_first(self.schema.title)
                .expect("Fatal: Field \"title\" not found!")
                .as_text()
                .unwrap();
            let cur_page = retrieved_doc
                .get_first(self.schema.page)
                .expect("Fatal: Field \"page\" not found!")
                .as_u64()
                .expect("Fatal: Field \"page\" is not a number!");
            let page: u32 = cur_page.try_into().unwrap_or_else(|_| {
                panic!("Fatal: Field \"page\" ({}) is bigger than u32!", cur_page)
            });
            let search_result = SearchResult::new(page, segment_ord, doc_id)
                .map_err(|e| SearchError(String::from("Failed creating search result!")))?;
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
        input: &str,
    ) -> Result<String> {
        println!("Calling `get_preview` for: query: {} and page: {}", input, search_result.page);
        let searcher = self.reader.searcher();
        let query_parser = QueryParser::for_index(&self.index, self.schema.default_fields());
        let query = query_parser
            .parse_query(input)
            .map_err(|e| SearchError(e.to_string()))?;
        let snippet_generator = SnippetGenerator::create(&searcher, &*query, self.schema.body)
            .map_err(|e| SearchError(e.to_string()))?;
        let retrieved_doc = searcher.doc(DocAddress { 
                segment_ord: (search_result.segment_ord), 
                doc_id: (search_result.doc_id) 
            })
            .map_err(|e| SearchError(e.to_string()))?;
        let cur_title = retrieved_doc
            .get_first(self.schema.title)
            .expect("Fatal: Field \"title\" not found!")
            .as_text()
            .unwrap();
        println!("Trying to get snippet for title: {}", cur_title);

        let snippet = snippet_generator.snippet_from_doc(&retrieved_doc);
        println!("Snippet: {}", snippet.fragment());
        println!("Snippet: {}", snippet.to_html());
        Ok(snippet.to_html())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{create_dir_all, remove_dir_all},
        panic,
    };

    use tantivy::{
        doc,
        schema::{Schema, STORED, TEXT},
        Index,
    };

    use super::*;
    const TEST_DIR_NAME: &str = "resources";
    const TEST_DOC_NAME: &str = "Of Mice and Men";
    const TEST_FILE_PATH: &str = "test.pdf";
    const BODY_1: &str =
        "A few miles south of Soledad, the Salinas River drops in close to the hillside \
        bank and runs deep and green. The water is warm too, for it has slipped twinkling \
        over the yellow sands in the sunlight before reaching the narrow pool.";

    const BODY_2: &str =
        "On one side of the river the golden foothill slopes curve up to the strong and rocky \
        Gabilan Mountains, but on the valley side the water is lined with trees—willows \
        fresh and green with every spring, carrying in their lower leaf junctures the \
        debris of the winter’s flooding; and sycamores with mottled, white, recumbent \
        limbs and branches that arch over the pool";

    fn setup() {
        create_dir_all(TEST_DIR_NAME)
            .unwrap_or_else(|_| panic!("Failed to create directory: {}", TEST_DIR_NAME));
    }

    fn teardown() {
        remove_dir_all(TEST_DIR_NAME)
            .unwrap_or_else(|_| panic!("Failed to remove directory: {}", TEST_DIR_NAME));
    }

    fn run_test<T>(test: T)
    where
        T: FnOnce() + panic::UnwindSafe,
    {
        setup();

        let result = panic::catch_unwind(test);

        teardown();

        assert!(result.is_ok())
    }

    fn create_searcher() -> Search {
        let mut schema_builder = Schema::builder();
        let title = schema_builder.add_text_field("title", TEXT | STORED);
        let path = schema_builder.add_text_field("path", TEXT | STORED);
        let page = schema_builder.add_u64_field("page", STORED);
        let body = schema_builder.add_text_field("body", TEXT);
        let schema = schema_builder.build();

        // Indexing documents
        let index_path = String::from(TEST_DIR_NAME);
        let index = Index::create_in_dir(index_path, schema.clone()).unwrap();
        let mut index_writer = index.writer(100_000_000).unwrap();

        const PAGE_1: u64 = 2;
        index_writer
            .add_document(doc!(
                title => TEST_DOC_NAME,
                path => TEST_FILE_PATH,
                page => PAGE_1,
                body => BODY_1
            ))
            .unwrap();

        const PAGE_2: u64 = 2;
        index_writer
            .add_document(doc!(
                title => TEST_DOC_NAME,
                page => PAGE_2,
                body => BODY_2
            ))
            .unwrap();
        index_writer.commit().unwrap();

        let search_scheama = SearchSchema::new(title, path, page, body, schema);
        Search::new(index, search_scheama).unwrap()
    }

    #[test]
    fn test_search() {
        run_test(|| {
            let search = create_searcher();
            test_normal_search(&search);
            test_preview(&search);
        })
    }

    fn test_normal_search(search: &Search) {
        // one-word search returning 1 result with 1 page
        let results = search.search(&String::from("flooding")).unwrap();
        assert!(results.contains_key(TEST_DOC_NAME));
        assert_eq!(results.get(TEST_DOC_NAME).unwrap().len(), 1);
        assert_eq!(results.get(TEST_DOC_NAME).unwrap().front().unwrap().page, 2);

        // one-word search returning 1 result with two pages
        let results = search.search(&String::from("the")).unwrap();
        assert!(results.contains_key(TEST_DOC_NAME));
        assert_eq!(results.get(TEST_DOC_NAME).unwrap().len(), 2);

        // two-word search returning 1 result with two pages
        let results = search.search(&String::from("river flooding")).unwrap();
        assert!(results.contains_key(TEST_DOC_NAME));
        assert_eq!(results.get(TEST_DOC_NAME).unwrap().len(), 2);

        // two-word search returning 1 result with two pages
        let id_results = search.search(&String::from("river OR flooding")).unwrap();
        assert_eq!(id_results, results);

        // two-word search returning 1 result with two pages
        let results = search.search(&String::from("river AND flooding")).unwrap();
        assert!(results.contains_key(TEST_DOC_NAME));
        assert_eq!(results.get(TEST_DOC_NAME).unwrap().len(), 1);
        assert_eq!(results.get(TEST_DOC_NAME).unwrap().front().unwrap().page, 2);

        let results = search
            .search(&String::from("(river OR valley) AND flooding"))
            .unwrap();
        assert!(results.contains_key(TEST_DOC_NAME));
        assert_eq!(results.get(TEST_DOC_NAME).unwrap().len(), 1);
        assert_eq!(results.get(TEST_DOC_NAME).unwrap().front().unwrap().page, 2);

        // Cannot find part of words like 'lley si' for 'valley side'
        let results = search.search(&String::from("lley si'")).unwrap();
        assert!(!results.contains_key(TEST_DOC_NAME));

        // Check caseinsensitive
        let results = search.search(&String::from("CARRYING")).unwrap();
        assert!(results.contains_key(TEST_DOC_NAME));
        assert_eq!(results.get(TEST_DOC_NAME).unwrap().len(), 1);
        assert_eq!(results.get(TEST_DOC_NAME).unwrap().front().unwrap().page, 2);

        // Phrase query should not be found (since only "limbs and branches" exists)
        let results = search.search(&String::from("\"limbs branches\"")).unwrap();
        assert!(!results.contains_key(TEST_DOC_NAME));

        // Phrase query should be found with higher slop
        let results = search
            .search(&String::from("\"limbs branches\"~1"))
            .unwrap();
        assert!(results.contains_key(TEST_DOC_NAME));
    }

    fn test_preview(search: &Search) {
        let input = String::from("flooding");
        let results = search.search(&input).unwrap();
        let doc_search_results = results.get(TEST_DOC_NAME).unwrap();
        for search_result in doc_search_results {
            let preview = search.get_preview(search_result, &input).unwrap();
            println!("Got preview: {}", preview);
            assert!(preview.len() > 0);
            assert!(preview.to_lowercase().find(&input).unwrap_or_default() > 0);
        }
    }
}

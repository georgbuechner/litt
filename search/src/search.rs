use std::collections::{HashMap, LinkedList};
use tantivy::{Index, Score, DocAddress, IndexReader, ReloadPolicy};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;

use litt_shared::search_schema::SearchSchema;

use crate::Result;
use crate::LittSearchError::{InitError, SearchError};

pub struct Search {
    index: Index,
    reader: IndexReader,
    schema: SearchSchema
}

impl Search {
    pub fn new(index: Index, schema: SearchSchema) -> Result<Self> {
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommit)
            .try_into()
            .map_err(|e| InitError(e.to_string()))?;
        Ok(Self {index, reader, schema})
    }

    pub fn search(&self, input: &str) -> Result<HashMap<String, LinkedList<u32>>> {
        let searcher = self.reader.searcher();
        let query_parser = QueryParser::for_index(&self.index, self.schema.default_fields());

        // QueryParser may fail if the query is not in the right
        // format. For user facing applications, this can be a problem.
        // A ticket has been opened regarding this problem.
        let query = query_parser.parse_query(input).map_err(|e| SearchError(e.to_string()))?;

        // Perform search.
        // `topdocs` contains the 10 most relevant doc ids, sorted by decreasing scores...
        let top_docs: Vec<(Score, DocAddress)> = searcher.search(&query, &TopDocs::with_limit(10))
            .map_err(|e| SearchError(e.to_string()))?;

        // Assemble results
        let mut results: HashMap<String, LinkedList<u32>> = HashMap::new();
        for (_score, doc_address) in top_docs {
            // Retrieve the actual content of documents given its `doc_address`.
            let retrieved_doc = searcher.doc(doc_address)
                .map_err(|e| SearchError(e.to_string()))?;
            let cur_title = retrieved_doc.get_first(self.schema.title)
                .expect("Fatal: Field \"title\" not found!")
                .as_text().unwrap();
            let cur_page = retrieved_doc.get_first(self.schema.page)
                .expect("Fatal: Field \"page\" not found!")
                .as_u64()
                .expect( "Fatal: Field \"page\" is not a number!");
            let page: u32 = cur_page
                .try_into().
                unwrap_or_else(|_| panic!("Fatal: Field \"page\" ({}) is bigger than u32!", cur_page));
            results.entry(cur_title.to_string())
                .and_modify(|pages| pages.push_back(page))
                .or_insert_with(|| LinkedList::from([page]));
        }
        Ok(results)
    }

    pub fn get_preview(&self, text: String, phrase: &String) -> Result<String> {
        let p_index = text.to_lowercase().find(&(phrase.to_lowercase()))
            .expect("Searched word not found on page!");
        let text = format!(
            "{}**{}**{}", 
            &text[0..p_index], 
            &text[p_index..p_index+phrase.len()], 
            &text[p_index+phrase.len()..]
        );
        let start = if p_index > 50 { p_index - 50 } else { 0 };
        let end = if (p_index + phrase.len() + 50) < text.len() { 
            p_index + phrase.len() + 50 
        } else { text.len() };
        let preview = &text[start..end];
        Ok(format!("...{}...", preview))
    }

    pub fn get_preview_on_page(_text: String, _input: &str) -> Result<HashMap<String, String>> {
        let previews: HashMap<String, String> = HashMap::new();
        Ok(previews)
    }

    pub fn get_previews(
        &self, _path: &str, _pages: LinkedList<u32>, _input: &str
    ) -> Result<HashMap<u32, HashMap<String, String>>> {
        let previews: HashMap<u32, HashMap<String, String>> = HashMap::new();
        Ok(previews)
    }
}

#[cfg(test)]
mod tests {
    use std::{fs::{create_dir_all, remove_dir_all}, panic, collections::LinkedList};

    use tantivy::{schema::{Schema, TEXT, STORED}, Index, doc};

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
        create_dir_all(TEST_DIR_NAME).unwrap_or_else(|_| panic!("Failed to create directory: {}", TEST_DIR_NAME));
    }

    fn teardown() {
        remove_dir_all(TEST_DIR_NAME).unwrap_or_else(|_| panic!("Failed to remove directory: {}", TEST_DIR_NAME));
    }

    fn run_test<T>(test: T)
        where T: FnOnce() + panic::UnwindSafe {
        setup();

        let result = panic::catch_unwind(|| {
            test()
        });

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
        index_writer.add_document(doc!(
            title => TEST_DOC_NAME,
            path => TEST_FILE_PATH,
            page => PAGE_1,
            body => BODY_1
        )).unwrap();


        const PAGE_2: u64 = 2;
        index_writer.add_document(doc!(
            title => TEST_DOC_NAME,
            page => PAGE_2,
            body => BODY_2
        )).unwrap();
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
        assert_eq!(results.get(TEST_DOC_NAME).unwrap(), &LinkedList::from([2]));

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
        assert_eq!(results.get(TEST_DOC_NAME).unwrap(), &LinkedList::from([2]));

        let results = search.search(&String::from("(river OR valley) AND flooding")).unwrap();
        assert!(results.contains_key(TEST_DOC_NAME));
        assert_eq!(results.get(TEST_DOC_NAME).unwrap().len(), 1);
        assert_eq!(results.get(TEST_DOC_NAME).unwrap(), &LinkedList::from([2]));
       
        // Cannot find part of words like 'lley si' for 'valley side'
        let results = search.search(&String::from("lley si'")).unwrap();
        assert!(!results.contains_key(TEST_DOC_NAME));

        
        // Check caseinsensitive
        let results = search.search(&String::from("CARRYING")).unwrap();
        assert!(results.contains_key(TEST_DOC_NAME));
        assert_eq!(results.get(TEST_DOC_NAME).unwrap().len(), 1);
        assert_eq!(results.get(TEST_DOC_NAME).unwrap(), &LinkedList::from([2]));
        
        // Phrase query should not be found (since only "limbs and branches" exists)
        let results = search.search(&String::from("\"limbs branches\"")).unwrap();
        assert!(!results.contains_key(TEST_DOC_NAME));
        
        // Phrase query should be found with higher slop
        let results = search.search(&String::from("\"limbs branches\"~1")).unwrap();
        assert!(results.contains_key(TEST_DOC_NAME));
    }

    fn test_preview(search: &Search) {
        let word = String::from("river");
        let preview = search.get_preview(BODY_1.to_string(), &word).unwrap();
        println!("{}: {}", preview, preview.len());
        assert!(preview.len() > word.len() && preview.len() < 110+word.len());
        assert!(preview.to_lowercase().contains(&word.to_lowercase()));

        let word = String::from("deep and green");
        let preview = search.get_preview(BODY_1.to_string(), &word).unwrap();
        println!("{}: {}", preview, preview.len());
        assert!(preview.len() > word.len() && preview.len() < 110+word.len());
        assert!(preview.to_lowercase().contains(&word.to_lowercase()));
    }

}

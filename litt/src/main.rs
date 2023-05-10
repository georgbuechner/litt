use std::fmt;
use std::fmt::Formatter;
use std::fs;
use std::path::Path;
use std::time::Instant;

use clap::CommandFactory;
use clap::Parser;

extern crate litt_search;
use litt_index::index::Index;
use litt_search::search::Search;
use litt_shared::search_schema::SearchSchema;
use litt_shared::LITT_DIRECTORY_NAME;

mod cli;
mod tracker;
use cli::Cli;
use tracker::IndexTracker;

use colored::*;

#[derive(Debug)]
struct LittError(String);

impl fmt::Display for LittError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self {
            LittError(s) => write!(f, "{}", s.red()),
        }
    }
}

fn main() -> Result<(), LittError> {
    let cli = Cli::parse();

    let mut index_tracker = match IndexTracker::create(".litt".into()) {
        Ok(index_tracker) => index_tracker,
        Err(e) => return Err(LittError(e.to_string())),
    };

    // everything that does not require litt index

    // Print existing litt indices
    if cli.list {
        println!("Currently available indices:");
        match &index_tracker.all() {
            Ok(indecies) => {
                for index in indecies {
                    println!(" - {:?}", index);
                }
            }
            Err(e) => return Err(LittError(e.to_string())),
        }
        return Ok(());
    }

    // check if name of litt index was given by user
    if cli.litt_index.is_none() {
        if let Err(e) = Cli::command().print_help() {
            return Err(LittError(e.to_string()));
        }
        return Err(LittError("Litt index missing!".into()));
    }

    let index_name = cli
        .litt_index
        .expect("Unkown error when getting name from index");

    // initialize new index
    if !cli.init.is_empty() {
        println!("Creating new index \"{}\" at: {}: ", index_name, cli.init);
        if index_tracker.exists(&index_name) || index_tracker.path_exists(&cli.init).is_some() {
            return Err(LittError(format!(
                "Failed to create index since it already exists: name: {}, path: {}",
                index_tracker.get_name(&cli.init).unwrap_or_default(),
                cli.init
            )));
        }
        // Add new index to index tracker (adding first, so that it can be removed in case of
        // failiure)
        let start = Instant::now();
        if let Err(e) = index_tracker.add(index_name, cli.init.clone()) {
            return Err(LittError(e.to_string()));
        }

        let mut index = match Index::create(&cli.init, SearchSchema::default()) {
            Ok(index) => index,
            Err(e) => return Err(LittError(e.to_string())),
        };

        if let Err(e) = index.add_all_pdf_documents() {
            return Err(LittError(e.to_string()));
        }
        println!(
            "Successfully indexed {} document pages in {:?}",
            index.searcher().num_docs(),
            start.elapsed()
        );
        return Ok(());
    }

    if cli.remove {
        // remove litt directory at index path
        let path = match index_tracker.get_path(&index_name) {
            Ok(path) => path,
            Err(e) => return Err(LittError(e.to_string())),
        };
        let index_path = path.join(LITT_DIRECTORY_NAME);
        fs::remove_dir_all(index_path).expect("Could not remove index-file");
        // remove litt-index from tracker.
        if let Err(e) = index_tracker.remove(index_name.clone()) {
            return Err(LittError(e.to_string()));
        }
        println!("Deleted index \"{}\".", index_name);
        return Ok(());
    }

    // get index:
    let index_path = match index_tracker.get_path(&index_name) {
        Ok(index_path) => index_path,
        Err(e) => return Err(LittError(e.to_string())),
    };
    let mut index = match Index::open_or_create(index_path.clone(), SearchSchema::default()) {
        Ok(index) => index,
        Err(e) => return Err(LittError(e.to_string())),
    };

    // update existing index
    if cli.update {
        println!("Updating index \"{}\".", index_name);
        let start = Instant::now();
        if let Err(e) = index.add_all_pdf_documents() {
            return Err(LittError(e.to_string()));
        }
        println!(
            "Update done. Now: {} document pages in {:?}",
            index.searcher().num_docs(),
            start.elapsed()
        );
        return Ok(());
    }
    // reload existing index
    if cli.reload {
        println!("Reloading index \"{}\".", index_name);
        let start = Instant::now();
        if let Err(e) = index.reload() {
            return Err(LittError(e.to_string()));
        }
        println!(
            "Reload done. Successfully indexed {} document pages in {:?}",
            index.searcher().num_docs(),
            start.elapsed()
        );
        return Ok(());
    }
    // do normal search
    else if !cli.term.is_empty() {
        let num_docs = &index.searcher().num_docs();
        println!(
            "Search index \"{}\" ({}) for {}",
            index_name,
            index_path.to_string_lossy(),
            cli.term
        );
        let start = Instant::now();
        let search = Search::new(index, SearchSchema::default());
        let results = match search.search(&cli.term, cli.offset, cli.limit) {
            Ok(results) => results,
            Err(e) => return Err(LittError(e.to_string())),
        };
        println!("Found results in {} document(s):", results.len());
        let mut counter = 0;
        for (title, pages) in &results {
            counter += 1;
            let title_name = Path::new(title)
                .with_extension("")
                .to_string_lossy()
                .to_string();
            println!("{}. {}", counter, title_name.bold());
            let index_path = index_path.join(title);
            println!("   ({})", index_path.to_string_lossy().italic());
            for page in pages {
                let preview = match search.get_preview(page, &cli.term) {
                    Ok(preview) => preview,
                    Err(e) => return Err(LittError(e.to_string())),
                };
                println!(
                    "  - p.{}: \"{}\", (score: {})",
                    page.page,
                    preview.italic(),
                    page.score
                );
            }
        }
        println!(
            "{} results from {} pages in {:?}.",
            results.values().fold(0, |acc, list| acc + list.len()),
            num_docs,
            start.elapsed()
        );
    }
    // do interactive search
    else {
        println!("Starting interactive search for \"{}\".", index_name);
    }
    Ok(())
}

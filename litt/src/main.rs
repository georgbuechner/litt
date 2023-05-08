use std::fs;
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

fn main() -> Result<(), LittError> {
    let cli = Cli::parse();

    let index_tracker =
        IndexTracker::create(".litt".into()).map_err(|e| LittError(e.to_string()))?;

    // everything that does not require litt index
    //
    // Print existing litt indices
    if cli.list {
        println!("Currently available indices:");
        for index in index_tracker.all().map_err(|e| LittError(e.to_string()))? {
            println!(" - {:?}", index);
        }
        return Ok(());
    }

    // check if name of litt index was given by user
    if cli.litt_index.is_none() {
        Cli::command()
            .print_help()
            .map_err(|e| LittError(e.to_string()))?;
        return Err(LittError("Litt index missing!".into()));
    } else if let Some(index_name) = cli.litt_index {
        // initialize new index
        if !cli.init.is_empty() {
            println!("Creating new index \"{}\" at: {}: ", index_name, cli.init);
            if index_tracker.exists(&index_name) || index_tracker.path_exists(&cli.init).is_some() {
                return Err(LittError(format!("Failed to create new index since a index at this path already exists: name: \"{}\", path: \"{}\"", index_tracker.get_name(&cli.init).unwrap_or_default(), cli.init)));
            }
            // Add new index to index tracker (adding first, so that it can be removed in case of
            // failiure)
            let start = Instant::now();
            _ = index_tracker
                .add(index_name, cli.init.clone())
                .map_err(|e| LittError(e.to_string()));
            // Create new index
            let mut index = Index::create(&cli.init, SearchSchema::default())
                .map_err(|e| LittError(e.to_string()))?;
            index
                .add_all_pdf_documents()
                .map_err(|e| LittError(e.to_string()))?;
            println!(
                "Successfully indexed {} document pages in {:?}",
                index.searcher().num_docs(),
                start.elapsed()
            );

            return Ok(());
        }

        if cli.remove {
            // remove litt directory at index path
            let path = index_tracker
                .get_path(&index_name)
                .map_err(|e| LittError(e.to_string()))?;
            let index_path = path.join(LITT_DIRECTORY_NAME);
            _ = fs::remove_dir_all(index_path);
            // remove litt-index from tracker.
            index_tracker
                .remove(index_name.clone())
                .map_err(|e| LittError(e.to_string()))?;
            println!("Deleted index \"{}\".", index_name);
            return Ok(());
        }

        // get index:
        let index_path = index_tracker
            .get_path(&index_name)
            .map_err(|e| LittError(e.to_string()))?;
        let mut index = Index::open_or_create(index_path.clone(), SearchSchema::default())
            .map_err(|e| LittError(e.to_string()))?;

        // update existing index
        if cli.reload {
            println!("Reloading index \"{}\".", index_name);
            let start = Instant::now();
            _ = index.reload().map_err(|e| LittError(e.to_string()));
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
            println!("Search index \"{}\" for {}", index_name, cli.term);
            let start = Instant::now();
            let search = Search::new(index, SearchSchema::default());
            let results = search
                .search(&cli.term, cli.offset, cli.limit)
                .map_err(|e| LittError(e.to_string()))?;
            println!("Found results in {} document(s):", results.len());
            let mut counter = 0;
            for (title, pages) in &results {
                counter += 1;
                println!("{}. {}", counter, title.bold());
                let index_path = index_path.join(title);
                println!("   ({})", index_path.to_string_lossy().italic());
                for page in pages {
                    let preview = search
                        .get_preview(page, &cli.term)
                        .map_err(|e| LittError(e.to_string()))?;
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
    }

    Ok(())
}

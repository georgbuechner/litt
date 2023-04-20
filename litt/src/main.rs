use clap::Parser;
use clap::CommandFactory;

extern crate litt_search;
use litt_index::index::Index;
use litt_search::search::Search;
use litt_shared::search_schema::SearchSchema;

mod cli;
mod tracker;
use cli::Cli;
use tracker::IndexTracker;

#[derive(Debug)]
struct LittError(String);


fn main() -> Result<(), LittError> {
    let cli = Cli::parse();

    let index_tracker = IndexTracker::create(".litt".into());

    if cli.list {
        println!("Currently availible indices:");
        for index in index_tracker.all() { 
            println!(" - {:?}", index);
        }
        return Ok(())
    }

    if cli.litt_index == "" {
        cli::Cli::command().print_help().unwrap();
        return Err(LittError("Litt index missing!".into()));
    }

    if !cli.init.is_empty() {
        let index = Index::create(
            IndexTracker::get_path(index_tracker, cli.litt_index)
            SearchSchema::default()
        );
        in
        println!(
            "Creating new index \"{}\" at: {}.",
            cli.litt_index, cli.init
        );
    } else if cli.update {
        println!("Updating index \"{}\".", cli.litt_index);
    } else if !cli.term.is_empty() {
        println!("Search index \"{}\" for {}", cli.litt_index, cli.term);
    } else {
        println!("Starting interactive search for \"{}\".", cli.litt_index);
    }

    Ok(())
}

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

    // everything that does not require litt index
    //
    // Print existing litt indices
    if cli.list {
        println!("Currently availible indices:");
        for index in index_tracker.all() { 
            println!(" - {:?}", index);
        }
        return Ok(())
    }

    // check if name of litt index was given by user
    if let None = cli.litt_index {
        cli::Cli::command().print_help().unwrap();
        return Err(LittError("Litt index missing!".into()));
    }
    else if let Some(index_name) = cli.litt_index {
        // initialize new index
        if !cli.init.is_empty() {
            let _index = Index::create(
                index_tracker.get_path(&index_name),
                SearchSchema::default()
            );
            println!(
                "Creating new index \"{}\" at: {}.",
                index_name, cli.init
            );
            return Ok(())
        } 
        
        // index
        let index = Index::open_or_create(
            index_tracker.get_path(&index_name),
            SearchSchema::default()
        ).map_err(|e| LittError(e.to_string()))?;

        // update existing index
        if cli.update {
            // TODO (fux): implement update
            println!("Updating index \"{}\".", index_name);
            return Ok(())
        } 
        // do normal search
        else if !cli.term.is_empty() {
            let _search = Search::new(index, SearchSchema::default());
            println!("Search index \"{}\" for {}", index_name, cli.term);
        } 
        // do interactive search
        else {
            println!("Starting interactive search for \"{}\".", index_name);
        }
    }

    Ok(())
}

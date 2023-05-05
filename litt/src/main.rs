use clap::CommandFactory;
use clap::Parser;

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
            if index_tracker.exists(&index_name) || !index_tracker.path_exists(&cli.init).is_none()
            {
                return Err(LittError(format!("Failed to create new index since a index at this path already exists: name: \"{}\", path: \"{}\"", index_tracker.get_name(&cli.init).unwrap_or_default(), cli.init)));
            }
            // Create new index
            _ = Index::create(&cli.init, SearchSchema::default())
                .map_err(|e| LittError(e.to_string()));
            // Add new index to index tracker
            _ = index_tracker.add(index_name, cli.init.clone())
                .map_err(|e| LittError(e.to_string()));
            return Ok(());
        }

        // get index:
        let mut index = Index::open_or_create(
            index_tracker
                .get_path(&index_name)
                .map_err(|e| LittError(e.to_string()))?,
            SearchSchema::default(),
        )
        .map_err(|e| LittError(e.to_string()))?;

        // update existing index
        if cli.update {
            println!("Updating index \"{}\".", index_name);
            _ = index.update().map_err(|e| LittError(e.to_string()));
            return Ok(());
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

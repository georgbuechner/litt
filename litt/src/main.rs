use std::collections::HashMap;
use std::env;
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
mod message_display;
mod tracker;

use cli::Cli;
use tracker::IndexTracker;

use colored::*;
use litt_shared::message_display::{Message, MessageDisplay};

#[derive(Debug)]
struct LittError(String);

impl fmt::Display for LittError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self {
            LittError(s) => write!(f, "{}", s.red()),
        }
    }
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

fn open_pdf(path: String, page: u32, term: String, cli: &Cli) -> Result<(), LittError> {
    let mut cmd = std::process::Command::new("zathura");
    cmd.arg(&path)
        .arg("-P")
        .arg(&page.to_string())
        .arg("-f")
        .arg(&term);

    let zathura_was_successful = match cmd.status() {
        Ok(status) => match status.code() {
            None => false,
            Some(code) => code == 0,
        },
        Err(_) => false,
    };
    if !zathura_was_successful {
        cli.display(Message::Warning(&format!(
            "Consider installing zathura so we can open the PDF on the correct page for you.\n\
Using standard system PDF viewer... {}",
            path
        )));
        open_std_programm(path)?;
    }
    Ok(())
}

fn open_std_programm(path: String) -> Result<(), LittError> {
    #[cfg(target_os = "macos")]
    std::process::Command::new("open")
        .arg(&path)
        .spawn()
        .map_err(|e| LittError(e.to_string()))?;

    #[cfg(target_os = "linux")]
    std::process::Command::new("xdg-open")
        .arg(&path)
        .spawn()
        .map_err(|e| LittError(e.to_string()))?;

    #[cfg(windows)]
    std::process::Command::new("cmd")
        .arg("/c")
        .arg("start")
        .arg(&path)
        .spawn()
        .map_err(|e| LittError(e.to_string()))?;

    Ok(())
}

fn main() -> Result<(), LittError> {
    let mut index_tracker = match IndexTracker::create(".litt".into()) {
        Ok(index_tracker) => index_tracker,
        Err(e) => return Err(LittError(e.to_string())),
    };

    // Check for fast last-number access
    let cli = Cli::parse();
    let args: Vec<String> = env::args().collect();
    let first_arg_option = args.get(1);
    if let Some(first_arg) = first_arg_option {
        if let Ok(last_result) = &first_arg.trim().parse::<u32>() {
            let fast_results = match index_tracker.load_fast_results() {
                Ok(fast_results) => fast_results,
                Err(e) => return Err(LittError(e.to_string())),
            };
            let path = fast_results
                .get(last_result)
                .expect("Number not in last results");
            if path.0.ends_with("pdf") {
                open_pdf(path.0.clone(), path.1, path.2.clone(), &cli)?;
            } else {
                open_std_programm(path.0.clone())?;
            }
            return Ok(());
        }
    }

    // everything that does not require litt index

    // Print existing litt indices
    if cli.list {
        cli.display(Message::Info("Currently available indices:"));
        match &index_tracker.all() {
            Ok(indecies) => {
                for index in indecies {
                    cli.display(Message::Info(&format!(" - {:?}", index)));
                }
            }
            Err(e) => return Err(LittError(e.to_string())),
        }
        return Ok(());
    }

    // check if name of litt index was given by user
    let index_name = match &cli.litt_index {
        None => {
            Cli::command()
                .print_help()
                .map_err(|e| LittError(e.to_string()))?;
            cli.display(Message::Error("Litt index missing!"));
            return Err(LittError("Litt index missing!".into()));
        }
        Some(index_name) => index_name,
    };

    // initialize new index
    if !cli.init.is_empty() {
        let current_dir = env::current_dir().map_err(|e| LittError(e.to_string()))?;
        let path = current_dir.join(&cli.init);
        cli.display(Message::Info(&format!(
            "Creating new index \"{}\" at: {}: ",
            index_name,
            path.to_string_lossy()
        )));
        if index_tracker.exists(index_name) || index_tracker.path_exists(&path).is_some() {
            return Err(LittError(format!(
                "Failed to create index since it already exists: name: {}, path: {}",
                index_tracker.get_name(&path).unwrap_or_default(),
                path.to_string_lossy()
            )));
        }
        // Add new index to index tracker (adding first, so that it can be removed in case of
        // failiure)
        let start = Instant::now();
        if let Err(e) = index_tracker.add(index_name, path.clone()) {
            return Err(LittError(e.to_string()));
        }

        let mut index = match Index::create(&path, SearchSchema::default(), &cli) {
            Ok(index) => index,
            Err(e) => return Err(LittError(e.to_string())),
        };

        index = match index.add_all_documents() {
            Ok(index_with_documents) => index_with_documents,
            Err(e) => return Err(LittError(e.to_string())),
        };

        let searcher = index.searcher().map_err(|e| LittError(e.to_string()))?;
        cli.display(Message::Info(&format!(
            "Successfully indexed {} document pages in {:?}",
            searcher.num_docs(),
            start.elapsed()
        )));
        return Ok(());
    }

    if cli.remove {
        // remove litt directory at index path
        let path = match index_tracker.get_path(index_name) {
            Ok(path) => path,
            Err(e) => return Err(LittError(e.to_string())),
        };
        let index_path = path.join(LITT_DIRECTORY_NAME);
        fs::remove_dir_all(index_path).expect("Could not remove index-file");
        // remove litt-index from tracker.
        if let Err(e) = index_tracker.remove(index_name.clone()) {
            return Err(LittError(e.to_string()));
        }
        cli.display(Message::Info(&format!("Deleted index \"{}\".", index_name)));
        return Ok(());
    }

    // get index:
    let index_path = index_tracker
        .get_path(index_name)
        .map_err(|e| LittError(e.to_string()))?;
    let index = match Index::open(index_path.clone(), SearchSchema::default(), &cli) {
        Ok(index) => index,
        Err(e) => return Err(LittError(e.to_string())),
    };
    let searcher = index.searcher().map_err(|e| LittError(e.to_string()))?;

    // update existing index
    if cli.update {
        cli.display(Message::Info(&format!(
            "Updating index \"{}\".",
            index_name
        )));
        let old_num_docs = searcher.num_docs();
        let start = Instant::now();
        return match index.update() {
            Ok(_) => {
                cli.display(Message::Info(&format!(
                    "Update done. Successfully indexed {} new document pages in {:?}. Now {} document pages.",
                    searcher
                        .num_docs()-old_num_docs,
                    start.elapsed(),
                    searcher
                        .num_docs(),
                )));
                Ok(())
            }
            Err(e) => Err(LittError(e.to_string())),
        };
    }
    // reload existing index
    if cli.reload {
        cli.display(Message::Info(&format!(
            "Reloading index \"{}\".",
            index_name
        )));
        let old_num_docs = searcher.num_docs();
        let start = Instant::now();
        if let Err(e) = index.reload() {
            return Err(LittError(e.to_string()));
        }
        cli.display(Message::Info(&format!(
            "Reload done. Successfully indexed {} new document pages in {:?}. Now {} document pages.",
            searcher.num_docs()-old_num_docs,
            start.elapsed(),
            searcher.num_docs(),
        )));
        return Ok(());
    }
    // do normal search
    else if !cli.term.is_empty() {
        let num_docs = searcher.num_docs();
        cli.display(Message::Info(&format!(
            "Search index \"{}\" ({}) for {}",
            index_name,
            index_path.to_string_lossy(),
            cli.term
        )));
        let start = Instant::now();
        let search = Search::new(index, SearchSchema::default());
        let search_term = if !cli.fuzzy {
            litt_search::search::SearchTerm::Exact(cli.term.clone())
        } else {
            litt_search::search::SearchTerm::Fuzzy(cli.term.clone(), cli.distance)
        };
        let results = match search.search(&search_term, cli.offset, cli.limit) {
            Ok(results) => results,
            Err(e) => return Err(LittError(e.to_string())),
        };
        cli.display(Message::Info(&format!(
            "Found results in {} document(s):",
            results.len()
        )));
        let mut fast_store_results: HashMap<u32, (String, u32, String)> = HashMap::new();
        let first_query_term = get_first_term(&cli.term);
        let mut counter = 0;
        let mut res_counter = 1;
        for (title, pages) in &results {
            counter += 1;
            let title_name = Path::new(title)
                .with_extension("")
                .to_string_lossy()
                .to_string();
            cli.display(Message::Info(&format!(
                "{}. {}",
                counter,
                title_name.bold()
            )));
            let index_path = index_path.join(title);
            cli.display(Message::Info(&format!(
                "   ({})",
                index_path.to_string_lossy().italic()
            )));
            for page in pages {
                fast_store_results.insert(
                    res_counter,
                    (
                        index_path.to_string_lossy().to_string(),
                        page.page,
                        first_query_term.clone(),
                    ),
                );
                let preview = match search.get_preview(page, &search_term) {
                    Ok(preview) => preview,
                    Err(e) => return Err(LittError(e.to_string())),
                };
                cli.display(Message::Info(&format!(
                    "  - [{}] p.{}: \"{}\", (score: {})",
                    res_counter,
                    page.page,
                    preview.italic(),
                    page.score
                )));
                res_counter += 1;
            }
        }
        if let Err(e) = index_tracker.store_fast_results(fast_store_results) {
            return Err(LittError(e.to_string()));
        }
        cli.display(Message::Info(&format!(
            "{} results from {} pages in {:?}.",
            results.values().fold(0, |acc, list| acc + list.len()),
            num_docs,
            start.elapsed()
        )));
    }
    // do interactive search
    else {
        cli.display(Message::Info(&format!(
            "Starting interactive search for \"{}\".",
            index_name
        )));
    }
    Ok(())
}

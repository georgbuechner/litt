use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use std::{env, io};
use std::io::Write;

use clap::CommandFactory;
use clap::Parser;

extern crate litt_search;
use crossterm::cursor::MoveToColumn;
use litt_index::index::Index;
use litt_search::search::Search;
use litt_shared::search_schema::SearchSchema;
use litt_shared::LITT_DIRECTORY_NAME;

mod cli;
mod tracker;
use cli::Cli;
use tantivy::Searcher;
use tracker::IndexTracker;

use colored::*;
use thiserror::Error;

use crossterm::{event::{self, Event, KeyCode}, terminal, execute};

#[derive(Debug, Error)]
enum LittError {
    #[error("Error:`{0}`")]
    General(String),
    #[error(transparent)]
    IoError(#[from] io::Error),
    #[error(transparent)]
    LittIndexError(#[from] litt_index::LittIndexError),
    #[error(transparent)]
    LittIndexTrackerError(#[from] tracker::LittIndexTrackerError),
}

// helper functions

fn open_pdf(path: String, page: u32, term: String) -> Result<(), LittError> {
    let mut cmd = std::process::Command::new("zathura");
    cmd.arg(&path)
        .arg("-P")
        .arg(page.to_string())
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
        println!(
            "Consider installing zathura so we can open the PDF on the correct page for you.\n\
Using standard system PDF viewer... {}",
            path
        );
        open_std_programm(path)?;
    }
    Ok(())
}

fn open_std_programm(path: String) -> Result<(), LittError> {
    #[cfg(target_os = "macos")]
    std::process::Command::new("open").arg(&path).spawn()?;

    #[cfg(target_os = "linux")]
    std::process::Command::new("xdg-open").arg(&path).spawn()?;

    #[cfg(windows)]
    std::process::Command::new("cmd")
        .arg("/c")
        .arg("start")
        .arg(&path)
        .spawn()?;

    Ok(())
}

fn show_failed_documents_error(index: &Index) {
    let failed_documents: Vec<String> = index.failed_documents().unwrap_or_default();
    if !failed_documents.is_empty() {
        let error_message = format!(
            "The following documents failed to process:\n{}",
            failed_documents.join("\n")
        );
        println!("{}", error_message);
    }
}

fn read(history: &mut Vec<String>) -> Result<String, LittError> {
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    let mut input = String::new();
    let mut index = history.len();
    print!("> ");
    stdout.flush()?;

    loop {
        if event::poll(std::time::Duration::from_millis(500))? {
            if let Event::Key(key_event) = event::read()? {
                match key_event.code {
                    KeyCode::Left => {
                        execute!(stdout, terminal::Clear(terminal::ClearType::CurrentLine))?;
                        input = "<".to_string();
                        break;
                    }
                    KeyCode::Right => {
                        execute!(stdout, terminal::Clear(terminal::ClearType::CurrentLine))?;
                        input = ">".to_string();
                        break;
                    }
                    KeyCode::Up=> {
                        if index > 0 {
                            index -= 1;
                            input = history.get(index).unwrap().to_string();
                            execute!(stdout, terminal::Clear(terminal::ClearType::CurrentLine))?;
                            execute!(stdout, MoveToColumn(0))?;
                            print!("> {}", input);
                            execute!(stdout, MoveToColumn((input.len()+2) as u16))?;
                            stdout.flush()?;
                        }
                    }
                    KeyCode::Down=> {
                        if history.len() > index+1 {
                            index += 1;
                            input = history.get(index).unwrap().to_string();
                            execute!(stdout, terminal::Clear(terminal::ClearType::CurrentLine))?;
                            execute!(stdout, MoveToColumn(0))?;
                            print!("> {}", input);
                            execute!(stdout, MoveToColumn((input.len()+2) as u16))?;
                            stdout.flush()?;
                        }
                        else if history.len() > index {
                            index += 1;
                            input = "".to_string();
                            execute!(stdout, terminal::Clear(terminal::ClearType::CurrentLine))?;
                            execute!(stdout, MoveToColumn(0))?;
                            print!("> ");
                            stdout.flush()?;
                        }
                    }

                    KeyCode::Char(c) => {
                        input.push(c);
                        print!("{}", c); // Echo the character
                        stdout.flush()?;
                    }
                    KeyCode::Backspace => {
                        if !input.is_empty() {
                            input.pop();
                            execute!(stdout, terminal::Clear(terminal::ClearType::CurrentLine))?;
                            execute!(stdout, MoveToColumn(0))?;
                            print!("> ");
                            stdout.flush()?;
                        }
                    }
                    KeyCode::Enter => {
                        break;
                    }
                    _ => {}
                }
            }
        }
    }
    terminal::disable_raw_mode()?;
    println!();
    if history.is_empty() || (history.len() > 0 && history.last().unwrap().to_string() != input) {
        history.push(input.clone());
    }
    Ok(input)
}

/*
 * Open fast result
 */
fn fast_open_result(index_tracker: &IndexTracker, last_result_num: &u32) -> Result<(), LittError> {
    let fast_results = match index_tracker.load_fast_results() {
        Ok(fast_results) => fast_results,
        Err(e) => return Err(LittError::General(e.to_string())),
    };
    let result = fast_results.get(last_result_num).ok_or_else(|| format!("Number {} not in last results", last_result_num));

    match result {
        Ok(path) => {
            if path.0.ends_with("pdf") {
                open_pdf(path.0.clone(), path.1, path.2.clone())?;
            } else {
                open_std_programm(path.0.clone())?;
            }
        },
        Err(err) => return Err(LittError::General(err.to_string())),

    }
    return Ok(());
}

/**
 * Print all availible litt indicies
 */
fn list_indicies(index_tracker: &IndexTracker) -> Result<(), LittError> {
    println!("Currently available indices:");
    match &index_tracker.all() {
        Ok(indecies) => {
            for index in indecies {
                println!(" - {:?}", index);
            }
        }
        Err(e) => return Err(LittError::General(e.to_string())),
    }
    return Ok(());
}

/**
 * Create new litt index
 */
fn create_litt_index(index_tracker: &mut IndexTracker, index_name: String, rel_path: &String) -> Result<(), LittError> {
    let current_dir = env::current_dir()?;
    let path = current_dir.join(rel_path);
    println!(
        "Creating new index \"{}\" at: {}: ",
        index_name,
        path.to_string_lossy()
    );
    if index_tracker.exists(&index_name) || index_tracker.path_exists(&path).is_some() {
        return Err(LittError::General(format!(
            "Failed to create index since it already exists: name: {}, path: {}",
            index_tracker.get_name(&path).unwrap_or_default(),
            path.to_string_lossy()
        )));
    }
    // Add new index to index tracker (adding first, so that it can be removed in case of
    // failiure)
    let start = Instant::now();
    if let Err(e) = index_tracker.add(index_name, path.clone()) {
        return Err(LittError::General(e.to_string()));
    }

    let mut index = match Index::create(&path, SearchSchema::default()) {
        Ok(index) => index,
        Err(e) => return Err(LittError::General(e.to_string())),
    };

    index = match index.add_all_documents() {
        Ok(index_with_documents) => index_with_documents,
        Err(e) => return Err(LittError::General(e.to_string())),
    };

    let searcher = index.searcher()?;
    println!(
        "Successfully indexed {} document pages in {:?}",
        searcher.num_docs(),
        start.elapsed()
    );
    show_failed_documents_error(&index);
    return Ok(());
}

/**
 * Remove existing litt index
 */
fn remove_litt_index(index_tracker: &mut IndexTracker, index_name: String) -> Result<(), LittError> {
    let path = match index_tracker.get_path(&index_name) {
        Ok(path) => path,
        Err(e) => return Err(LittError::General(e.to_string())),
    };
    let index_path = path.join(LITT_DIRECTORY_NAME);
    let msg = match fs::remove_dir_all(index_path) {
            //. expect("Could not remove index-file")
    Ok(()) => "Ok.",
            Err(e) if e.kind() == io::ErrorKind::NotFound => "Index directory didn't exist.",
            Err(e) => return Err(LittError::General(e.to_string())),
        };// remove litt-index from tracker.
    if let Err(e) = index_tracker.remove(index_name.clone()) {
        return Err(LittError::General(e.to_string()));
    }
    println!("Deleted index \"{}\": {}", index_name, msg);
    return Ok(());
}

/**
 * Update litt index (only indexes new or changed documents)
 */
fn update_litt_index(index: Index, searcher: Searcher, index_name: String) -> Result<(), LittError> {
    println!("Updating index \"{}\".", index_name);
    let old_num_docs = searcher.num_docs();
    let start = Instant::now();
    return match index.update() {
        Ok(ref updated_index) => {
            println!(
                "Update done. Successfully indexed {} new document pages in {:?}. Now {} document pages.",
                searcher
                    .num_docs()-old_num_docs,
                start.elapsed(),
                searcher
                    .num_docs(),
            );
            show_failed_documents_error(updated_index);
            Ok(())
        }
        Err(e) => Err(LittError::General(e.to_string())),
    };
}

/**
 * Reload litt index (reloads *every* document)
 */
fn reload_litt_index(index: Index, searcher: Searcher, index_name: String) -> Result<(), LittError> {
    println!("Reloading index \"{}\".", index_name);
    let old_num_docs = searcher.num_docs();
    let start = Instant::now();
    match index.reload() {
        Ok(index) => {
            println!(
                "Reload done. Successfully indexed {} new document pages in {:?}. Now {} document pages.",
                searcher.num_docs()-old_num_docs,
                start.elapsed(),
                searcher.num_docs(),
            );
            show_failed_documents_error(&index);
            return Ok(());
        }
        Err(e) => {
            return Err(LittError::General(e.to_string()));
        }
    }
}

/**
 * Searches for query in litt index
 */
fn search_litt_index(search: &Search, index_tracker: &mut IndexTracker, index_path: &PathBuf, searcher: &Searcher, index_name: &String, term: String, fuzzy: bool, distance: u8, offset: usize, limit: usize) -> Result<(), LittError> {
    let num_docs = searcher.num_docs();
    println!(
        "Search index \"{}\" ({}) for {}",
        index_name,
        index_path.to_string_lossy(),
        term
    );
    let start = Instant::now();
    let search_term = if fuzzy {
        litt_search::search::SearchTerm::Exact(term)
    } else {
        litt_search::search::SearchTerm::Fuzzy(term, distance)
    };
    let results = match search.search(&search_term, offset, limit) {
        Ok(results) => results,
        Err(e) => return Err(LittError::General(e.to_string())),
    };
    println!("Found results in {} document(s):", results.len());
    let mut fast_store_results: HashMap<u32, (String, u32, String)> = HashMap::new();
    let mut counter = 0;
    let mut res_counter = 1;
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
            let (preview, matched_term) = match search.get_preview(page, &search_term) {
                Ok(preview) => preview,
                Err(e) => return Err(LittError::General(e.to_string())),
            };
            fast_store_results.insert(
                res_counter,
                (
                    index_path.to_string_lossy().to_string(),
                    page.page,
                    matched_term,
                ),
            );
            println!(
                "  - [{}] p.{}: \"{}\", (score: {})",
                res_counter,
                page.page,
                preview.italic(),
                page.score
            );
            res_counter += 1;
        }
    }
    if let Err(e) = index_tracker.store_fast_results(fast_store_results) {
        return Err(LittError::General(e.to_string()));
    }
    println!(
        "{} results (offset={}) from {} pages in {:?}.",
        results.values().fold(0, |acc, list| acc + list.len()),
        offset,
        num_docs,
        start.elapsed()
    );
    Ok(())
}

fn main() -> Result<(), LittError> {
    let mut index_tracker = match IndexTracker::create(".litt".into()) {
        Ok(index_tracker) => index_tracker,
        Err(e) => return Err(LittError::General(e.to_string())),
    };

    // Check for fast last-number access
    let args: Vec<String> = env::args().collect();
    let first_arg_option = args.get(1);
    if let Some(first_arg) = first_arg_option {
        if let Ok(last_result) = &first_arg.trim().parse::<u32>() {
            return fast_open_result(&index_tracker, last_result);
        }
    }

    let cli = Cli::parse();

    // everything that does not require litt index

    // Print existing litt indices
    if cli.list {
        return list_indicies(&index_tracker);
    }

    // check if name of litt index was given by user
    let index_name = match cli.litt_index {
        None => {
            Cli::command().print_help()?;
            return Err(LittError::General("Litt index missing!".into()));
        }
        Some(index_name) => index_name,
    };

    // initialize new index
    if !cli.init.is_empty() {
        return create_litt_index(&mut index_tracker, index_name, &cli.init);
    }

    // remove litt directory at index path
    if cli.remove {
        return remove_litt_index(&mut index_tracker, index_name);
    }

    // get index:
    let index_path = index_tracker.get_path(&index_name)?;
    let index = match Index::open(index_path.clone(), SearchSchema::default()) {
        Ok(index) => index,
        Err(e) => return Err(LittError::General(e.to_string())),
    };
    let searcher = index.searcher()?;

    // update existing index
    if cli.update {
        return update_litt_index(index, searcher, index_name.clone());
    }
    // reload existing index
    if cli.reload {
        return reload_litt_index(index, searcher, index_name.clone());
    }
    let search = Search::new(index, SearchSchema::default());
    // do normal search
    if !cli.term.is_empty() {
        return search_litt_index(&search, &mut index_tracker, &index_path, &searcher, &index_name, cli.term, cli.fuzzy, cli.distance, cli.offset, cli.limit);
    }
    // do interactive search
    let mut distance = 2;
    let mut limit = 10;
    let mut offset = 0;
    let mut search_term = String::new();
    let mut history: Vec<String> = Vec::new();
    loop {
        if search_term == "" {
            println!("Interactive search in \"{}\" (limit={}, distance={}; type \"#set <variable> <value>\" to change, \"q\" to quit, start search-term with \"~\" for fuzzy-search)", index_name.clone(), limit, distance);
        } else {
            println!("Interactive search in \"{}\" (showing results {} to {}; type \"→\" for next, \"←\" for previous {} results, \"↑\"|\"↓\" to cycle history, \"q\" to quit)", index_name.clone(), offset, offset+limit, limit);
        }
        let inp = read(&mut history)?;
        if inp == "q" {
            break;
        }
        if (inp == ">" || inp == "<") && search_term == "" {
            println!("No search term specified! Enter search term first...");
            continue;
        } else if inp == "<" && offset == 0 {
            println!("Offset is already zero...");
            continue;
        } else if inp == ">" {
            offset += limit;
        } else if inp == "<" {
            offset = offset - limit;
        } else if inp.starts_with("#") {
            let parts: Vec<&str> = inp.split(" ").collect();
            match parts.get(1) {
                Some(&"limit") => limit = parts[2].parse().unwrap(),
                Some(&"distance") => distance = parts[2].parse().unwrap(),
                _ => {
                    println!("You can only set \"limit\", \"fuzzy\" or \"distance\"...");
                    continue;
                }
            }
            if search_term == "" {
                continue;
            }
        } else {
            search_term = inp;
        }
        let final_term = if search_term.starts_with("~") {
            search_term[1..].to_string()
        } else {
            search_term.clone()
        };
        match search_litt_index(&search, &mut index_tracker, &index_path, &searcher, &index_name, final_term, !search_term.starts_with("~"), distance, offset, limit) {
            Ok(_) => {
                println!();
                continue;
            },
            Err(e) => return Err(e)
        }
    }
    Ok(())
}

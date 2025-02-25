use litt_shared::search_schema::SearchSchema;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::Instant;
use std::{env, io};
use unicode_segmentation::UnicodeSegmentation;

use clap::CommandFactory;
use clap::Parser;

extern crate litt_search;
use crossterm::cursor::MoveToColumn;
use litt_index::index::Index;
use litt_search::search::Search;
use litt_shared::LITT_DIRECTORY_NAME;

mod cli;
mod tracker;

use cli::Cli;
use tantivy::Searcher;
use tracker::IndexTracker;

use colored::*;
use thiserror::Error;

use crossterm::{
    event::{self, Event, KeyCode},
    execute, terminal,
};

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

enum SearchOptionUpdate {
    Limit(usize),
    Distance(u8),
}

enum InteractiveSearchInput {
    BrowseBackword,
    BrowseForward,
    Quit,
    Search(String),
    SearchOptionsUpdate(SearchOptionUpdate),
    OpenPdf(u32),
}

pub struct SearchOptions {
    limit: usize,
    offset: usize,
    fuzzy: bool,
    distance: u8,
}

// helper functions

fn insert_grapheme(input: &mut String, pos: usize, c: char) {
    let graphemes: Vec<&str> = input.graphemes(true).collect(); // Split at grapheme clusters
    let mut new_string = String::new();

    for (i, g) in graphemes.iter().enumerate() {
        if i == pos {
            new_string.push(c);
        }
        new_string.push_str(g);
    }

    if pos >= graphemes.len() {
        new_string.push(c);
    }

    *input = new_string; // Modify the original String
}

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

fn read(history: &mut Vec<String>) -> Result<InteractiveSearchInput, LittError> {
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    let mut input = String::new();
    let cmd: InteractiveSearchInput;
    let mut index = history.len();
    print!("> ");
    stdout.flush()?;

    fn clear_and_print(
        stdout: &mut io::Stdout,
        line: String,
        adjust_cursor: bool,
    ) -> Result<(), LittError> {
        execute!(stdout, terminal::Clear(terminal::ClearType::CurrentLine))?;
        execute!(stdout, MoveToColumn(0))?;
        print!("{}", line);
        if adjust_cursor {
            execute!(stdout, MoveToColumn(line.graphemes(true).count() as u16))?;
        }
        stdout.flush()?;
        Ok(())
    }

    loop {
        if event::poll(std::time::Duration::from_millis(500))? {
            if let Event::Key(key_event) = event::read()? {
                match key_event.code {
                    KeyCode::Left => {
                        // Only browse if input is empty, otherwise move cursor backwords
                        if input.is_empty() {
                            execute!(stdout, terminal::Clear(terminal::ClearType::CurrentLine))?;
                            cmd = InteractiveSearchInput::BrowseBackword;
                            break;
                        } else if let Ok(cursor_pos) = crossterm::cursor::position() {
                            if cursor_pos.0 > 2 {
                                execute!(stdout, MoveToColumn(cursor_pos.0 - 1))?;
                            }
                        }
                    }
                    KeyCode::Right => {
                        // Only browse if input is empty, otherwise move cursor forwards
                        if input.is_empty() {
                            execute!(stdout, terminal::Clear(terminal::ClearType::CurrentLine))?;
                            cmd = InteractiveSearchInput::BrowseForward;
                            break;
                        } else if let Ok(cursor_pos) = crossterm::cursor::position() {
                            if cursor_pos.0 - 2 < (input.graphemes(true).count() as u16) {
                                execute!(stdout, MoveToColumn(cursor_pos.0 + 1))?;
                            }
                        }
                    }
                    KeyCode::Up => {
                        if index > 0 {
                            index -= 1;
                            input = history.get(index).unwrap().to_string();
                            clear_and_print(&mut stdout, format!("> {}", input), true)?;
                            stdout.flush()?;
                        }
                    }
                    KeyCode::Down => {
                        if history.len() > index + 1 {
                            index += 1;
                            input = history.get(index).unwrap().to_string();
                            clear_and_print(&mut stdout, format!("> {}", input), true)?;
                        } else if history.len() > index {
                            index += 1;
                            input = "".to_string();
                            clear_and_print(&mut stdout, "> ".to_string(), false)?;
                        }
                    }
                    KeyCode::Char(c) => {
                        if let Ok(cursor_pos) = crossterm::cursor::position() {
                            let pos: usize = (cursor_pos.0 - 2) as usize;
                            if input.graphemes(true).count() >= pos {
                                insert_grapheme(&mut input, pos, c);
                                clear_and_print(&mut stdout, format!("> {}", input), false)?;
                                execute!(stdout, MoveToColumn(cursor_pos.0 + 1))?;
                            }
                        }
                    }
                    KeyCode::Backspace => {
                        // Remove char at current cursor position and move position left.
                        if let Ok(cursor_pos) = crossterm::cursor::position() {
                            if !input.is_empty() {
                                input = input
                                    .as_str()
                                    .graphemes(true)
                                    .enumerate()
                                    .filter_map(|(i, g)| {
                                        if i == (cursor_pos.0 as usize) - 3 {
                                            None
                                        } else {
                                            Some(g)
                                        }
                                    })
                                    .collect();
                                clear_and_print(&mut stdout, format!("> {}", input), false)?;
                                execute!(stdout, MoveToColumn(cursor_pos.0 - 1))?;
                            }
                        }
                    }
                    KeyCode::Enter => {
                        if input == "q" {
                            cmd = InteractiveSearchInput::Quit;
                        } else if let Ok(result_num) = &input.trim().parse::<u32>() {
                            cmd = InteractiveSearchInput::OpenPdf(*result_num);
                        } else if input.starts_with("#") {
                            let parts: Vec<&str> = input.split(" ").collect();
                            match parts.get(1) {
                                Some(&"limit") => {
                                    cmd = InteractiveSearchInput::SearchOptionsUpdate(
                                        SearchOptionUpdate::Limit(parts[2].parse().unwrap()),
                                    )
                                }
                                Some(&"distance") => {
                                    cmd = InteractiveSearchInput::SearchOptionsUpdate(
                                        SearchOptionUpdate::Distance(parts[2].parse().unwrap()),
                                    )
                                }
                                _ => {
                                    println!(
                                        "You can only set \"limit\", \"fuzzy\" or \"distance\"..."
                                    );
                                    continue;
                                }
                            }
                        } else {
                            cmd = InteractiveSearchInput::Search(input.to_string());
                        }
                        break;
                    }
                    _ => {}
                }
            }
        }
    }
    terminal::disable_raw_mode()?;
    println!();
    if history.is_empty() || (!history.is_empty() && history.last().unwrap() != &input) {
        history.push(input.clone());
    }
    Ok(cmd)
}

/*
 * Open fast result
 */
fn fast_open_result(index_tracker: &IndexTracker, last_result_num: &u32) -> Result<(), LittError> {
    let fast_results = match index_tracker.load_fast_results() {
        Ok(fast_results) => fast_results,
        Err(e) => return Err(LittError::General(e.to_string())),
    };
    let result = fast_results
        .get(last_result_num)
        .ok_or_else(|| format!("Number {} not in last results", last_result_num));

    match result {
        Ok(path) => {
            if path.0.ends_with("pdf") {
                open_pdf(path.0.clone(), path.1, path.2.clone())?;
            } else {
                open_std_programm(path.0.clone())?;
            }
        }
        Err(err) => return Err(LittError::General(err.to_string())),
    }
    Ok(())
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
    Ok(())
}

/**
 * Create new litt index
 */
fn create_litt_index(
    index_tracker: &mut IndexTracker,
    index_name: String,
    rel_path: &String,
) -> Result<(), LittError> {
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
    Ok(())
}

/**
 * Remove existing litt index
 */
fn remove_litt_index(
    index_tracker: &mut IndexTracker,
    index_name: String,
) -> Result<(), LittError> {
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
    }; // remove litt-index from tracker.
    if let Err(e) = index_tracker.remove(index_name.clone()) {
        return Err(LittError::General(e.to_string()));
    }
    println!("Deleted index \"{}\": {}", index_name, msg);
    Ok(())
}

/**
 * Update litt index (only indexes new or changed documents)
 */
fn update_litt_index(
    index: Index,
    searcher: Searcher,
    index_name: String,
) -> Result<(), LittError> {
    println!("Updating index \"{}\".", index_name);
    let old_num_docs = searcher.num_docs();
    let start = Instant::now();
    match index.update() {
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
    }
}

/**
 * Reload litt index (reloads *every* document)
 */
fn reload_litt_index(
    index: Index,
    searcher: Searcher,
    index_name: String,
) -> Result<(), LittError> {
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
            Ok(())
        }
        Err(e) => Err(LittError::General(e.to_string())),
    }
}

/**
 * Searches for query in litt index
 */
fn search_litt_index(
    search: &Search,
    index_tracker: &mut IndexTracker,
    index_path: &Path,
    searcher: &Searcher,
    index_name: &String,
    term: String,
    opts: &SearchOptions,
) -> Result<(), LittError> {
    let num_docs = searcher.num_docs();
    println!(
        "Search index \"{}\" ({}) for {}",
        index_name,
        index_path.to_string_lossy(),
        term
    );
    let start = Instant::now();
    let search_term = if opts.fuzzy {
        litt_search::search::SearchTerm::Fuzzy(term, opts.distance)
    } else {
        litt_search::search::SearchTerm::Exact(term)
    };
    let results = match search.search(&search_term, opts.offset, opts.limit) {
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
        opts.offset,
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
        let opts = SearchOptions {
            limit: cli.limit,
            offset: cli.offset,
            fuzzy: cli.fuzzy,
            distance: cli.distance,
        };
        return search_litt_index(
            &search,
            &mut index_tracker,
            &index_path,
            &searcher,
            &index_name,
            cli.term,
            &opts,
        );
    }

    // do interactive search
    let mut opts = SearchOptions {
        limit: 10,
        offset: 0,
        fuzzy: false,
        distance: 2,
    };
    let mut search_term = String::new();
    let mut history: Vec<String> = Vec::new();
    loop {
        if search_term.is_empty() {
            println!(
                "Interactive search in \"{}\" (limit={}, distance={}; type \"#set <variable> \
                <value>\" to change, \"q\" to quit, start search-term with \"~\" for \
                fuzzy-search)",
                index_name.clone(),
                opts.limit,
                opts.distance
            );
        } else {
            println!(
                "Interactive search in \"{}\" (showing results {} to {}; type \"→\" for next,\
                \"←\" for previous {} results, \"↑\"|\"↓\" to cycle history, \"q\" to quit)",
                index_name.clone(),
                opts.offset,
                opts.offset + opts.limit,
                opts.limit
            );
        }
        match read(&mut history) {
            Ok(InteractiveSearchInput::Quit) => break,
            Ok(InteractiveSearchInput::BrowseForward) => {
                if search_term.is_empty() {
                    println!("No search term specified! Enter search term first...");
                    continue;
                } else {
                    opts.offset += opts.limit;
                }
            }
            Ok(InteractiveSearchInput::BrowseBackword) => {
                if search_term.is_empty() {
                    println!("No search term specified! Enter search term first...");
                    continue;
                } else if opts.offset == 0 {
                    println!("Offset is already zero...");
                    continue;
                } else {
                    opts.offset -= opts.limit;
                }
            }
            Ok(InteractiveSearchInput::OpenPdf(result_num)) => {
                match fast_open_result(&index_tracker, &result_num) {
                    Ok(_) => continue,
                    Err(e) => {
                        println!("{}", e);
                        continue;
                    }
                }
            }
            Ok(InteractiveSearchInput::SearchOptionsUpdate(update)) => {
                // Do search option update
                match update {
                    SearchOptionUpdate::Limit(limit) => opts.limit = limit,
                    SearchOptionUpdate::Distance(distance) => opts.distance = distance,
                }
                // If a search term was already specified, repeat search with updates search
                // options otherwise continue
                if search_term.is_empty() {
                    continue;
                }
            }
            Ok(InteractiveSearchInput::Search(term)) => {
                opts.offset = 0;
                search_term = term;
            }
            Err(_) => {
                println!("[error] Unkown error during input...");
                continue;
            }
        }
        let final_term = search_term.strip_prefix("~").unwrap_or(&search_term);
        opts.fuzzy = search_term.starts_with("~");
        match search_litt_index(
            &search,
            &mut index_tracker,
            &index_path,
            &searcher,
            &index_name,
            final_term.to_string(),
            &opts,
        ) {
            Ok(_) => {
                println!();
                continue;
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

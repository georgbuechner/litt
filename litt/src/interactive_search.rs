use crate::tracker::IndexTracker;
use crate::{
    fast_open_result, search_litt_index, InteractiveSearchInput, LittError, SearchOptionUpdate,
    SearchOptions,
};
use crossterm::cursor::MoveToColumn;
use crossterm::event::{Event, KeyCode};
use crossterm::{event, execute, terminal};
use litt_search::search::Search;
use std::io;
use std::io::Write;
use std::path::Path;
use tantivy::Searcher;
use unicode_segmentation::UnicodeSegmentation;

pub(super) fn interactive_search(
    search: &Search,
    index_tracker: &mut IndexTracker,
    index_path: &Path,
    searcher: &Searcher,
    index_name: &String,
) -> Result<(), LittError> {
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
                match fast_open_result(index_tracker, &result_num) {
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
            Ok(InteractiveSearchInput::Search(term)) => search_term = term,
            Err(_) => {
                println!("[error] Unkown error during input...");
                continue;
            }
        }
        let final_term = search_term.strip_prefix('~').unwrap_or(&search_term);
        opts.fuzzy = search_term.starts_with('~');
        match search_litt_index(
            search,
            index_tracker,
            index_path,
            searcher,
            index_name,
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
            execute!(stdout, MoveToColumn(line.len() as u16))?;
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
                            if cursor_pos.0 - 2 < (input.len() as u16) {
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
                            if input.len() >= pos {
                                input.insert(pos, c);
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
                        } else if input.starts_with('#') {
                            let parts: Vec<&str> = input.split(' ').collect();
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

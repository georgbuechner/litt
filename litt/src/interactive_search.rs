use crate::interactive_search::InteractiveSearchState::*;
use crate::interactive_search::SearchOptionUpdate::*;
use crate::InteractiveSearchInput::{*};
use crate::SearchOptions;

pub(super) struct InteractiveSearch {
    state: InteractiveSearchState
}

pub(super) enum InteractiveSearchState {
    WaitingForInitialInput {
        options: SearchOptions,
    },
    SearchInProgress {
        search_term: String,
        options: SearchOptions,
    },
    OpenPdf {
        last_result_num: u32,
        options: SearchOptions,
    },
    Finished,
}

pub(super) enum SearchOptionUpdate {
    Limit(usize),
    Fuzzy(String),
    Distance(u8),
}

pub(super) enum InteractiveSearchInput {
    BrowseBackward,
    BrowseForward,
    Quit,
    Empty,
    SearchOptionUpdate(SearchOptionUpdate),
    SearchTerm(String),
    LastSearchResult(u32),
}

impl InteractiveSearch {
    pub(super) fn new(options: SearchOptions) -> Self {
        Self {
            state: WaitingForInitialInput { options },
        }
    }
    
    pub(super) fn state(&self) -> &InteractiveSearchState {
        &self.state
    }

    pub(super) fn display_instructions(&self, index_name: &str) {
        match &self.state {
            WaitingForInitialInput { options: opts, .. } => {
                println!(
                    "Interactive search in \"{}\" (limit={}, distance={}; type \"#set <variable> \
                    <value>\" to change, \"q\" to quit, start search-term with \"~\" for \
                    fuzzy-search)",
                    index_name, opts.limit, opts.distance
                );
            }
            SearchInProgress {options: opts, ..} | OpenPdf { options: opts, .. } => {
                println!(
                    "Interactive search in \"{}\" (showing results {} to {}; type \"→\" for next, \
                    \"←\" for previous {} results, \"↑\"|\"↓\" to cycle history, \"q\" to quit)",
                    index_name,
                    opts.offset,
                    opts.offset + opts.limit,
                    opts.limit
                );
            }
            Finished => {}
        }
    }

    /// Transition the interactive search state machine.
    pub(super) fn state_transition(&mut self, input: &InteractiveSearchInput) {
        match (&mut self.state, input) {
            // No state change when input is empty
            (_, Empty) => {}
            (_, Quit) => {
                self.state = Finished;
            }
            // Open pdf/ result
            (WaitingForInitialInput { options } | SearchInProgress { options, .. } | OpenPdf { options, .. }, LastSearchResult(last_number_num)) => {
                self.state = OpenPdf{ 
                    last_result_num: *last_number_num, options: *options
                }
            }
            // Trying to browse results without having searched; print warning and do nothing.
            (WaitingForInitialInput { .. }, BrowseBackward | BrowseForward) => {
                println!("No search term specified! Enter search term first...");
            }
            // Browsing results
            (
                SearchInProgress { ref mut options, .. } | OpenPdf { ref mut options, .. },
                BrowseForward,
            ) => {
                options.offset += options.limit;
            }
            (
                SearchInProgress { ref mut options, .. } | OpenPdf { ref mut options, .. },
                BrowseBackward,
            ) => {
                if options.offset == 0 {
                    println!("Offset is already zero...");
                } else {
                    options.offset -= options.limit;
                }
            }
            // Change options or fuzzy search
            (
                WaitingForInitialInput {
                    ref mut options, ..
                }
                | SearchInProgress {
                    ref mut options, ..
                }
                | OpenPdf { ref mut options, .. },
                SearchOptionUpdate(update),
            ) => match update {
                Limit(limit) => {
                    options.limit = *limit;
                }
                Distance(distance) => {
                    options.distance = *distance;
                }
                Fuzzy(term) => {
                    options.fuzzy = true;
                    self.state = SearchInProgress {
                        search_term: term.to_string(),
                        options: *options,
                    }
                }
            },
            // Normal search
            (
                SearchInProgress { options, .. } | WaitingForInitialInput { options } | OpenPdf { options, .. },
                SearchTerm(term),
            ) => {
                self.state = SearchInProgress {
                    search_term: term.to_string(),
                    options: *options
                };
            }
            (Finished, _) => unreachable!(),
        }
    }
}
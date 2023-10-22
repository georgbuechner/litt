use clap::Parser;

/// Literature tool for searching pdfs in a directory (litt-index).
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// the litt index to open
    #[arg()]
    pub litt_index: Option<String>,

    /// the search term (optional, if not specified starts interactive search)
    #[arg(default_value_t = String::from(""))]
    pub term: String,

    /// create new litt-index at path
    #[arg(short, long, value_name = "PATH", default_value_t = String::from(""))]
    pub init: String,

    /// updates an existing litt-index
    #[arg(short, long, default_value_t = false)]
    pub update: bool,

    /// recreates an existing litt-index
    #[arg(long, default_value_t = false)]
    pub reload: bool,

    /// removes an existing litt-index
    #[arg(short, long, default_value_t = false)]
    pub remove: bool,

    /// shows all existing indices
    #[arg(short, long, default_value_t = false)]
    pub list: bool,

    /// the offset for search results f.e. 0-10 (offset=0)
    #[arg(long, default_value_t = 0)]
    pub offset: usize,

    /// the max number of search results f.e. 0-10 (limit=10)
    #[arg(long, default_value_t = 10)]
    pub limit: usize,

    /// use fuzzy matching. F.e. "hund"="hunde" and "bare"="bori"
    #[arg(short, long, default_value_t = false)]
    pub fuzzy: bool,

    /// the max distance between two terms when using --fuzzy. F.e. "bare"="bori" (distance=2)
    #[arg(long, long, default_value_t = 2)]
    pub distance: u8,
}

#[test]
fn verify_cli() {
    use clap::CommandFactory;
    Cli::command().debug_assert()
}

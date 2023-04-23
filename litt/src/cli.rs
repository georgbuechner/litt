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
    
    /// shows all existing indices
    #[arg(short, long, default_value_t = false)]
    pub list: bool,
}

#[test]
fn verify_cli() {
    use clap::CommandFactory;
    Cli::command().debug_assert()

}

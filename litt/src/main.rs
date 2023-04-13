use std::env;
use args;

use args::{Args, ArgsError};
use getopts::Occur;

const PROGRAMM_DESC: &'static str = "Literature tool for searching all pdfs in a directory";
const PROGRAMM_NAME: &'static str = "litt";

#[derive(Debug)]
enum LittAction { Initialize, Update, Search }

fn main() {
    let cmd_args: Vec::<String> = env::args().collect();
    let cmd_args: Vec<&str> = cmd_args.iter().map(|x| &**x).collect();
    match parse(&cmd_args) {
        Ok(t) => { 
            println!("got: {t:?}"); 
        }
        Err(error) => {
            println!("{}", error);
        }
    }
}

fn parse(input: &Vec::<&str>) -> Result<LittAction, &'static str> {
    let mut args = Args::new(PROGRAMM_NAME, PROGRAMM_DESC);
    args.flag("h", "help", "Print help message");
    args.flag("i", "initialize", "initialize new litt-index, f.e. litt -i 'uni' '/path/to/uni'");
    args.flag("u", "update", "Update existing litt-index, f.e. litt -i 'uni'");
    args.flag("s", "search", "Search term in existing litt-index, f.e. litt -s 'uni' 'Hegel AND Marx");

    args.parse(input)?;

    let help = args.value_of("help").unwrap();
    if help {
        print!("{}", args.full_usage());
    }

    // Get flags
    let initialize = args.value_of("initialize").unwrap();
    let update = args.value_of("update").unwrap();
    let search = args.value_of("search").unwrap();

    if initialize {
        if input.len() > 4 {
            return Ok(LittAction::Initialize) 
        }
        else {
            return Err("initialize: Missing arguments!")
        }
    }
    else if update {
        return Ok(2)
    }
    else if search {
        return Ok(3)
    }

    Ok(0)
}

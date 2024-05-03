pub enum Message<'a> {
    Info(&'a str),
    Warning(&'a str),
    Error(&'a str),
    Index(IndexMessage<'a>),
}

pub enum IndexMessage<'a> {
    Adding { document_name: &'a str },
    SkippedExisting { document_name: &'a str },
}

pub trait MessageDisplay: Sync {
    fn display(&self, message: Message) {
        match message {
            Message::Info(text) => println!("Info: {}", text),
            Message::Warning(text) => println!("Warning: {}", text),
            Message::Error(text) => println!("Error: {}", text),
            Message::Index(index_message) => match index_message {
                IndexMessage::Adding { document_name } => {
                    println!("Adding document: {}", document_name)
                }
                IndexMessage::SkippedExisting { document_name } => {
                    println!("Skipped (already exists): {}", document_name)
                }
            },
        }
    }
}

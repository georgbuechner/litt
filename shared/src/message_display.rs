pub enum Message<'a> {
    Info(&'a str),
    Warning(&'a str),
    Error(&'a str)
}

impl Message<'_> {
    pub fn info(message: &str) -> Message {
        Message::Info(message)
    }
    pub fn error(message: &str) -> Message {
        Message::Error(message)
    }
    
    pub fn warning(message: &str) -> Message {
        Message::Warning(message)
    }
}

pub trait MessageDisplay: Sync {
    fn display(&self, message: Message) {
            match message {
                Message::Info(text) => println!("Info: {}", text),
                Message::Warning(text) => println!("Warning: {}", text),
                Message::Error(text) => println!("Error: {}", text),
            }
        }
}

pub struct SimpleMessageDisplay;

impl MessageDisplay for SimpleMessageDisplay {}
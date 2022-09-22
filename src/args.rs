use clap::{ArgAction, Parser};

#[derive(Parser)] // requires `derive` feature
#[clap(author, version, about, long_about = None)]
pub struct Args {
    #[clap(short, long, action = ArgAction::Count)]
    pub verbose: u8,

    #[clap(required = true, last = true, value_parser)]
    command: Vec<String>,
}

impl Args {
    #[inline(always)]
    pub fn command(&self) -> String {
        self.command.join(" ").trim().to_string()
    }
}

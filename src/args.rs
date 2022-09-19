use clap::Parser;

#[derive(Parser)] // requires `derive` feature
#[clap(author, version, about, long_about = None)]
pub struct Args {
    #[clap(short, long, action)]
    pub verbose: bool,

    #[clap(last = true, value_parser)]
    command: Vec<String>,
}

impl Args {
    #[inline(always)]
    pub fn command(&self) -> String {
        self.command.join(" ").trim().to_string()
    }
}
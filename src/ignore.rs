use std::fs;
use std::path::Path;

use serde::Deserialize;

#[derive(Clone, Deserialize)]
pub struct Ignore {
    push: Option<Vec<String>>,
    pull: Option<Vec<String>>,
    both: Option<Vec<String>>,
}

impl Ignore {
    #[allow(dead_code)]
    pub fn new(
        push: Option<Vec<String>>,
        pull: Option<Vec<String>>,
        both: Option<Vec<String>>,
    ) -> Self {
        Self { push, pull, both }
    }

    pub fn from_working_dir(working_dir: &Path) -> Option<Self> {
        let file = working_dir
            .to_path_buf()
            .join(".mainframer")
            .join("ignore.yml");
        if let Ok(contents) = fs::read_to_string(file) {
            Self::from_file_contents(contents).ok()
        } else {
            None
        }
    }

    pub fn from_file_contents(contents: String) -> Result<Self, String> {
        serde_yaml::from_str::<Ignore>(&contents).map_err(|x| x.to_string())
    }

    pub fn push(&self) -> Vec<String> {
        [
            self.push.clone().unwrap_or_default(),
            self.both.clone().unwrap_or_default(),
        ]
        .concat()
    }

    pub fn pull(&self) -> Vec<String> {
        [
            self.pull.clone().unwrap_or_default(),
            self.both.clone().unwrap_or_default(),
        ]
        .concat()
    }
}

use std::{fs, path::Path};

use crate::sync::PullMode;
use serde::Deserialize;

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct Config {
    pub remote: Remote,
    #[serde(default)]
    pub push: Push,
    #[serde(default)]
    pub pull: Pull,
}

impl Config {
    pub fn from_path(path: &Path) -> Result<Self, String> {
        if let Ok(contents) = fs::read_to_string(path) {
            Self::from_file_contents(&contents)
        } else {
            Err(format!("Failed to find file at {}", path.to_string_lossy()))
        }
    }

    #[inline(always)]
    pub fn from_file_contents(contents: &str) -> Result<Self, String> {
        serde_yaml::from_str::<Config>(contents)
            .map_err(|err| err.to_string())
            .and_then(|config| {
                match (
                    config.valid_pull_compression_range(),
                    config.valid_push_compression_range(),
                ) {
                    (true, true) => Ok(config),
                    (false, _) => Err(format!(
                        "'pull.compression' must be a positive integer from 1 to 9, but was {}",
                        config.pull.compression
                    )),
                    (_, false) => Err(format!(
                        "'push.compression' must be a positive integer from 1 to 9, but was {}",
                        config.push.compression
                    )),
                }
            })
    }

    pub fn valid_pull_compression_range(&self) -> bool {
        (1..=9).contains(&self.pull.compression)
    }

    pub fn valid_push_compression_range(&self) -> bool {
        (1..=9).contains(&self.push.compression)
    }
}

#[derive(Debug, Default, Eq, PartialEq, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Remote {
    pub host: String,
    pub user: Option<String>,
    pub port: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct Push {
    #[serde(default = "Push::default_compression")]
    pub compression: i8,
    pub user: Option<String>,
}

impl Push {
    pub fn default_compression() -> i8 {
        Self::default().compression
    }
}

impl Default for Push {
    fn default() -> Self {
        Self {
            compression: 3,
            user: None,
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct Pull {
    #[serde(default = "Pull::default_compression")]
    pub compression: i8,
    #[serde(default)]
    pub mode: PullMode,
    pub user: Option<String>,
}

impl Pull {
    pub fn default_compression() -> i8 {
        Self::default().compression
    }
}

impl Default for Pull {
    fn default() -> Self {
        Self {
            compression: 1,
            mode: PullMode::default(),
            user: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_config_from_str_all_fields_2_spaces_indent() {
        let content = "
remote:
  host: computer1
push:
  compression: 5
pull:
  compression: 2
  mode: serial
";

        assert_eq!(
            Config::from_file_contents(content),
            Ok(Config {
                remote: Remote {
                    host: String::from("computer1"),
                    ..Default::default()
                },
                push: Push {
                    compression: 5,
                    ..Default::default()
                },
                pull: Pull {
                    compression: 2,
                    mode: PullMode::Serial,
                    ..Default::default()
                },
            })
        );
    }

    #[test]
    fn parse_config_from_str_all_fields_strings_in_quotes() {
        let content = "
remote:
  host: \"computer1\"
push:
  compression: 5
pull:
  compression: 2
  mode: \"serial\"
";

        assert_eq!(
            Config::from_file_contents(content),
            Ok(Config {
                remote: Remote {
                    host: String::from("computer1"),
                    ..Default::default()
                },
                push: Push {
                    compression: 5,
                    ..Default::default()
                },
                pull: Pull {
                    compression: 2,
                    mode: PullMode::Serial,
                    ..Default::default()
                },
            })
        );
    }

    #[test]
    fn parse_config_from_str_all_fields_4_spaces_indent() {
        let content = "
remote:
    host: computer1
push:
    compression: 5
pull:
    compression: 2
    mode: serial
";

        assert_eq!(
            Config::from_file_contents(content),
            Ok(Config {
                remote: Remote {
                    host: String::from("computer1"),
                    ..Default::default()
                },
                push: Push {
                    compression: 5,
                    ..Default::default()
                },
                pull: Pull {
                    compression: 2,
                    mode: PullMode::Serial,
                    ..Default::default()
                },
            })
        );
    }

    #[test]
    fn parse_config_from_str_only_remote_machine_host() {
        let content = "
remote:
  host: computer1
";
        assert_eq!(
            Config::from_file_contents(content),
            Ok(Config {
                remote: Remote {
                    host: String::from("computer1"),
                    ..Default::default()
                },
                push: Push::default(),
                pull: Pull::default(),
            })
        );
    }

    #[test]
    fn parse_config_from_str_compression_valid_range() {
        let mut destinations: Vec<String> = Vec::new();

        destinations.push(String::from("push"));
        destinations.push(String::from("pull"));

        for destination in destinations {
            for compression_level in 1..9 {
                let content = format!(
                    "
remote:
  host: computer1
{:#?}:
  compression: {:#?}
",
                    destination, compression_level
                );

                assert_eq!(
                    Config::from_file_contents(&content),
                    Ok(Config {
                        remote: Remote {
                            host: "computer1".to_string(),
                            ..Default::default()
                        },
                        push: if destination == "push" {
                            Push {
                                compression: compression_level,
                                ..Default::default()
                            }
                        } else {
                            Push::default()
                        },
                        pull: if destination == "pull" {
                            Pull {
                                compression: compression_level,
                                ..Default::default()
                            }
                        } else {
                            Pull::default()
                        },
                    })
                );
            }
        }
    }

    #[test]
    fn parse_config_from_str_compression_invalid_range() {
        let mut destinations: Vec<String> = Vec::new();

        destinations.push(String::from("push"));
        destinations.push(String::from("pull"));

        let mut invalid_compression_levels: Vec<i64> = Vec::new();

        invalid_compression_levels.push(0);
        invalid_compression_levels.push(10);
        invalid_compression_levels.push(-1);

        for destination in destinations {
            for compression_level in &invalid_compression_levels {
                let content = format!(
                    "
remote:
    host: computer1
{:#?}:
  compression: {:#?}
",
                    destination, compression_level
                );

                assert_eq!(
                    Config::from_file_contents(&content),
                    Err(format!(
                        "'{}.compression' must be a positive integer from 1 to 9, but was {}",
                        destination, compression_level
                    ))
                );
            }
        }
    }

    #[test]
    fn parse_config_from_str_push_compression_not_an_integer() {
        let content = "
remote:
    host: computer1
push:
  compression: yooo
";
        assert!(Config::from_file_contents(content).is_err());
    }

    #[test]
    fn parse_config_from_str_only_pull_mode_parallel() {
        let content = "
remote:
    host: computer1
pull:
  mode: parallel
";
        assert_eq!(
            Config::from_file_contents(content),
            Ok(Config {
                remote: Remote {
                    host: "computer1".to_string(),
                    ..Default::default()
                },
                push: Push::default(),
                pull: Pull {
                    mode: PullMode::Parallel,
                    ..Default::default()
                },
            })
        );
    }

    #[test]
    fn parse_config_from_str_only_pull_mode_unsupported_value() {
        let content = "
remote:
    host: computer1
pull:
  mode: unsupported_value
";
        assert!(Config::from_file_contents(content).is_err());
    }
}

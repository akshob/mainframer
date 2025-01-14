use std::io;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use std::sync::mpsc::TryRecvError::*;
use std::thread;
use std::time::{Duration, Instant};

use bus::BusReader;
use crossbeam_channel::unbounded;
use crossbeam_channel::Receiver;
use crossbeam_channel::Sender;
use serde::Deserialize;

use crate::config::Config;
use crate::ignore::Ignore;
use crate::remote_command::{RemoteCommandErr, RemoteCommandOk};

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct PushOk {
    pub duration: Duration,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct PushErr {
    pub duration: Duration,
    pub message: String,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PullMode {
    /// Serial, after remote command execution.
    Serial,

    /// Parallel to remote command execution.
    /// First parameter is pause between pulls.
    Parallel,
}

impl PullMode {
    pub const PARALLEL_DURATION: Duration = Duration::from_millis(500);
}

impl Default for PullMode {
    fn default() -> Self {
        Self::Serial
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct PullOk {
    pub duration: Duration,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct PullErr {
    pub duration: Duration,
    pub message: String,
}

pub fn push(
    local_dir_absolute_path: &Path,
    config: &Config,
    ignore: &Option<Ignore>,
    verbose: u8,
) -> Result<PushOk, PushErr> {
    let start_time = Instant::now();

    let mut command = Command::new("rsync");

    command.arg("--archive").arg("--delete");

    if let Some(port) = &config.remote.port {
        command.arg(format!("-e ssh -p {port}"));
    }

    command
        .arg(format!(
            "--rsync-path=mkdir -p {} && rsync",
            project_dir_on_remote_machine(config, local_dir_absolute_path)
        ))
        .arg(format!("--compress-level={}", config.push.compression));

    for i in 0..verbose {
        //Don't add more than two --verbose to rsync, unless you want to debug rsync
        if i == 2 {
            break;
        };
        command.arg("--verbose");
    }

    if let Some(ignore) = ignore {
        apply_exclude_from(&mut command, ignore.push());
    }

    command.arg("./");

    if let Some(user) = &config.push.user {
        command.arg(format!(
            "{user}@{remote_machine_name}:{project_dir_on_remote_machine}",
            remote_machine_name = config.remote.host,
            project_dir_on_remote_machine =
                project_dir_on_remote_machine(config, local_dir_absolute_path)
        ));
    } else {
        command.arg(format!(
            "{remote_machine_name}:{project_dir_on_remote_machine}",
            remote_machine_name = config.remote.host,
            project_dir_on_remote_machine =
                project_dir_on_remote_machine(config, local_dir_absolute_path)
        ));
    }

    tracing::debug!("Executing rsync push: {:?}", command);

    match execute_rsync(&mut command) {
        Err(reason) => Err(PushErr {
            duration: start_time.elapsed(),
            message: reason,
        }),
        Ok(_) => Ok(PushOk {
            duration: start_time.elapsed(),
        }),
    }
}

pub fn pull(
    local_dir_absolute_path: &Path,
    config: Config,
    ignore: Option<Ignore>,
    pull_mode: &PullMode,
    remote_command_finished_signal: BusReader<Result<RemoteCommandOk, RemoteCommandErr>>,
    verbose: u8,
) -> Receiver<Result<PullOk, PullErr>> {
    match pull_mode {
        PullMode::Serial => pull_serial(
            local_dir_absolute_path.to_path_buf(),
            config,
            ignore,
            remote_command_finished_signal,
            verbose,
        ),
        PullMode::Parallel => pull_parallel(
            local_dir_absolute_path.to_path_buf(),
            config,
            ignore,
            PullMode::PARALLEL_DURATION,
            remote_command_finished_signal,
            verbose,
        ),
    }
}

fn pull_serial(
    local_dir_absolute_path: PathBuf,
    config: Config,
    ignore: Option<Ignore>,
    mut remote_command_finished_rx: BusReader<Result<RemoteCommandOk, RemoteCommandErr>>,
    verbose: u8,
) -> Receiver<Result<PullOk, PullErr>> {
    let (pull_finished_tx, pull_finished_rx): (
        Sender<Result<PullOk, PullErr>>,
        Receiver<Result<PullOk, PullErr>>,
    ) = unbounded();

    #[allow(unused_must_use)]
    // We don't handle remote_command_result, in any case we need to pull after it.
    thread::spawn(move || {
        remote_command_finished_rx
            .recv()
            .expect("Could not receive remote_command_finished_rx");

        pull_finished_tx
            .send(_pull(
                local_dir_absolute_path.as_path(),
                &config,
                &ignore,
                verbose,
            ))
            .expect("Could not send pull_finished signal");
    });

    pull_finished_rx
}

fn pull_parallel(
    local_dir_absolute_path: PathBuf,
    config: Config,
    ignore: Option<Ignore>,
    pause_between_pulls: Duration,
    mut remote_command_finished_signal: BusReader<Result<RemoteCommandOk, RemoteCommandErr>>,
    verbose: u8,
) -> Receiver<Result<PullOk, PullErr>> {
    let (pull_finished_tx, pull_finished_rx): (
        Sender<Result<PullOk, PullErr>>,
        Receiver<Result<PullOk, PullErr>>,
    ) = unbounded();
    let start_time = Instant::now();

    thread::spawn(move || {
        loop {
            if let Err(pull_err) =
                _pull(local_dir_absolute_path.as_path(), &config, &ignore, verbose)
            {
                pull_finished_tx
                    .send(Err(pull_err)) // TODO handle code 24.
                    .expect("Could not send pull_finished signal");
                break;
            }

            match remote_command_finished_signal.try_recv() {
                Err(reason) => match reason {
                    Disconnected => break,
                    Empty => thread::sleep(pause_between_pulls),
                },
                Ok(remote_command_result) => {
                    let remote_command_duration = match remote_command_result {
                        Err(err) => err.duration,
                        Ok(ok) => ok.duration,
                    };

                    // Final pull after remote command to ensure consistency of the files.
                    match _pull(local_dir_absolute_path.as_path(), &config, &ignore, verbose) {
                        Err(err) => pull_finished_tx
                            .send(Err(PullErr {
                                duration: calculate_perceived_pull_duration(
                                    start_time.elapsed(),
                                    remote_command_duration,
                                ),
                                message: err.message,
                            }))
                            .expect("Could not send pull finished signal (last iteration)"),

                        Ok(_) => pull_finished_tx
                            .send(Ok(PullOk {
                                duration: calculate_perceived_pull_duration(
                                    start_time.elapsed(),
                                    remote_command_duration,
                                ),
                            }))
                            .expect("Could not send pull finished signal (last iteration)"),
                    }

                    break;
                }
            }
        }
    });

    pull_finished_rx
}

fn _pull(
    local_dir_absolute_path: &Path,
    config: &Config,
    ignore: &Option<Ignore>,
    verbose: u8,
) -> Result<PullOk, PullErr> {
    let start_time = Instant::now();

    let mut command = Command::new("rsync");

    command
        .arg("--archive")
        .arg("--delete")
        .arg(format!("--compress-level={}", config.pull.compression));

    if let Some(port) = &config.remote.port {
        command.arg(format!("-e ssh -p {port}"));
    }

    for i in 0..verbose {
        //Don't add more than two --verbose to rsync, unless you want to debug rsync
        if i == 2 {
            break;
        };
        command.arg("--verbose");
    }

    if let Some(ignore) = ignore {
        apply_exclude_from(&mut command, ignore.pull());
    }

    if let Some(user) = &config.pull.user {
        command.arg(format!(
            "{user}@{remote_machine_name}:{project_dir_on_remote_machine}/",
            remote_machine_name = config.remote.host,
            project_dir_on_remote_machine =
                project_dir_on_remote_machine(config, local_dir_absolute_path)
        ));
    } else {
        command.arg(format!(
            "{remote_machine_name}:{project_dir_on_remote_machine}/",
            remote_machine_name = config.remote.host,
            project_dir_on_remote_machine =
                project_dir_on_remote_machine(config, local_dir_absolute_path)
        ));
    }

    command.arg("./");

    tracing::debug!("Executing rsync pull: {:?}", command);

    match execute_rsync(&mut command) {
        Err(reason) => Err(PullErr {
            duration: start_time.elapsed(),
            message: reason,
        }),
        Ok(_) => Ok(PullOk {
            duration: start_time.elapsed(),
        }),
    }
}

pub fn project_dir_on_remote_machine(config: &Config, local_dir_absolute_path: &Path) -> String {
    if let Some(path) = &config.remote.path {
        path.clone()
    } else {
        format!("~/mainframer{}", local_dir_absolute_path.to_string_lossy())
    }
}

fn apply_exclude_from(rsync_command: &mut Command, exclude_file: Vec<String>) {
    exclude_file.into_iter().for_each(|glob| {
        rsync_command.arg(format!("--exclude={}", glob));
    });
}

struct Message;

impl Write for Message {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let buffer_string = String::from_utf8_lossy(buf);
        let split_string = buffer_string.split('\n');
        for s in split_string {
            if s.is_empty() || s == "\n" {
                continue;
            }
            tracing::debug!("{}", s);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn execute_rsync(rsync: &mut Command) -> Result<(), String> {
    let mut result = rsync
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let mut message = Message;
    io::copy(&mut result.stdout.take().unwrap(), &mut message)
        .expect("Couldn't copy rsync result's stdout");
    let mut err_message = Message;
    io::copy(&mut result.stderr.take().unwrap(), &mut err_message)
        .expect("Couldn't copy rsync result's stderr");

    match result.wait_with_output() {
        Err(_) => Err(String::from("Generic rsync error.")), // Rust doc doesn't really say when can an error occur.
        Ok(output) => match output.status.code() {
            None => Err(String::from("rsync was terminated.")),
            Some(status_code) => match status_code {
                0 => Ok(()),
                _ => Err(
                    format!(
                        "rsync exit code '{exit_code}',\nrsync stdout '{stdout}',\nrsync stderr '{stderr}'.",
                        exit_code = status_code,
                        stdout = String::from_utf8_lossy(&output.stdout),
                        stderr = String::from_utf8_lossy(&output.stderr)
                    )
                )
            }
        },
    }
}

fn calculate_perceived_pull_duration(
    total_pull_duration: Duration,
    remote_command_duration: Duration,
) -> Duration {
    match total_pull_duration.checked_sub(remote_command_duration) {
        None => Duration::from_millis(0),
        Some(duration) => duration,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calculate_perceived_pull_duration_equals() {
        assert_eq!(
            calculate_perceived_pull_duration(Duration::from_millis(10), Duration::from_millis(10)),
            Duration::from_millis(0)
        );
    }

    #[test]
    fn calculate_perceived_pull_duration_pull_longer_than_execution() {
        assert_eq!(
            calculate_perceived_pull_duration(Duration::from_secs(10), Duration::from_secs(8)),
            Duration::from_secs(2)
        );
    }

    #[test]
    fn calculate_perceived_pull_duration_pull_less_than_execution() {
        assert_eq!(
            calculate_perceived_pull_duration(Duration::from_secs(7), Duration::from_secs(9)),
            Duration::from_secs(0)
        );
    }
}

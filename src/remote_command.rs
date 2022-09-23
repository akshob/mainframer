use std::io;
use std::io::Write;
use std::process::Command;
use std::process::Stdio;
use std::thread;
use std::time::{Duration, Instant};

use bus::{Bus, BusReader};

use crate::config::Config;

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct RemoteCommandOk {
    pub duration: Duration,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct RemoteCommandErr {
    pub duration: Duration,
}

pub fn execute_remote_command(
    remote_command: String,
    config: Config,
    project_dir_on_remote_machine: String,
    number_of_readers: usize,
) -> Vec<BusReader<Result<RemoteCommandOk, RemoteCommandErr>>> {
    let mut bus: Bus<Result<RemoteCommandOk, RemoteCommandErr>> = Bus::new(1);
    let mut readers: Vec<BusReader<Result<RemoteCommandOk, RemoteCommandErr>>> =
        Vec::with_capacity(number_of_readers);

    for _ in 0..number_of_readers {
        readers.push(bus.add_rx())
    }

    thread::spawn(move || {
        bus.broadcast(_execute_remote_command(
            &remote_command,
            &config,
            &project_dir_on_remote_machine,
        ));
    });

    readers
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
            tracing::info!("{}", s);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn _execute_remote_command(
    remote_command: &str,
    config: &Config,
    project_dir_on_remote_machine: &str,
) -> Result<RemoteCommandOk, RemoteCommandErr> {
    let start_time = Instant::now();

    let mut command = Command::new("ssh");

    if let Some(port) = &config.remote.port {
        command.arg(format!("-p {port}"));
    }

    if let Some(user) = &config.remote.user {
        command.arg(format!("{}@{}", user, config.remote.host.clone()));
    } else {
        command.arg(config.remote.host.clone());
    }

    command
        .arg(format!(
            "echo 'set -e && cd {project_dir_on_remote_machine} && echo \"{remote_command}\" && echo \"\" && {remote_command}' | bash",
            project_dir_on_remote_machine = project_dir_on_remote_machine,
            remote_command = remote_command)
        );

    let mut process = command
        // Interactively pipe ssh output to Mainframer output.
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let mut message = Message;
    io::copy(&mut process.stdout.take().unwrap(), &mut message)
        .expect("Couldn't copy ssh command's stdout");
    let mut err_message = Message;
    io::copy(&mut process.stderr.take().unwrap(), &mut err_message)
        .expect("Couldn't copy ssh command's stderr");

    match process.wait() {
        Err(_) => Err(RemoteCommandErr {
            duration: start_time.elapsed(),
        }), // No need to get error description as we've already piped command output to Mainframer output.
        Ok(exit_status) => {
            if exit_status.success() {
                Ok(RemoteCommandOk {
                    duration: start_time.elapsed(),
                })
            } else {
                Err(RemoteCommandErr {
                    duration: start_time.elapsed(),
                })
            }
        }
    }
}

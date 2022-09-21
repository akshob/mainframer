extern crate bus;
extern crate crossbeam_channel;

use std::env;
use std::fs;
use std::process;
use std::time::Instant;

use args::Args;
use clap::Parser;
use config::*;
use ignore::*;
use sync::PullMode;
use time::*;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

mod args;
mod config;
mod ignore;
mod remote_command;
mod sync;
mod time;

// TODO use Reactive Streams instead of Channels.

fn main() {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .expect("Setting default subscriber failed!");

    let total_start = Instant::now();

    tracing::info!(":: Mainframer v{}", env!("CARGO_PKG_VERSION"));

    let args = Args::parse();

    let local_dir_absolute_path = match env::current_dir() {
        Err(_) => exit_with_error("Could not resolve working directory, make sure it exists and user has enough permissions to work with it.", 1),
        Ok(value) => fs::canonicalize(value).unwrap()
    };

    let mut config_file = local_dir_absolute_path.clone();
    config_file.push(".mainframer/config.yml");

    let config = match Config::from_path(&config_file) {
        Err(error) => exit_with_error(&error, 1),
        Ok(value) => value,
    };

    let ignore = Ignore::from_working_dir(&local_dir_absolute_path);

    tracing::info!("Pushing...");

    match sync::push(&local_dir_absolute_path, &config, &ignore, args.verbose) {
        Err(err) => exit_with_error(
            &format!(
                "Push failed: {}, took {}",
                err.message,
                format_duration(err.duration)
            ),
            1,
        ),
        Ok(ok) => tracing::info!("Push done: took {}.", format_duration(ok.duration)),
    }

    match config.pull.mode {
        PullMode::Serial => tracing::info!("Executing command on remote machine..."),
        PullMode::Parallel => {
            tracing::info!("Executing command on remote machine (pulling in parallel)...")
        }
    }

    let mut remote_command_readers = remote_command::execute_remote_command(
        args.command(),
        config.clone(),
        sync::project_dir_on_remote_machine(&local_dir_absolute_path),
        2,
    );

    let pull_finished_rx = sync::pull(
        &local_dir_absolute_path,
        config.clone(),
        ignore,
        &config.pull.mode,
        remote_command_readers.pop().unwrap(),
        args.verbose,
    );

    let remote_command_result = remote_command_readers.pop().unwrap().recv().unwrap();

    match remote_command_result {
        Err(ref err) => {
            tracing::error!(
                "\nExecution failed: took {}.",
                format_duration(err.duration)
            );
            tracing::info!("Pulling...");
        }
        Ok(ref ok) => {
            tracing::info!("Execution done: took {}.", format_duration(ok.duration));
            tracing::info!("Pulling...");
        }
    }

    let pull_result = pull_finished_rx
        .recv()
        .expect("Could not receive remote_to_local_sync_result");

    let total_duration = total_start.elapsed();

    match pull_result {
        Err(ref err) => tracing::error!(
            "Pull failed: {}, took {}.",
            err.message,
            format_duration(err.duration)
        ),
        Ok(ref ok) => tracing::info!("Pull done: took {}", format_duration(ok.duration)),
    }

    if remote_command_result.is_err() || pull_result.is_err() {
        exit_with_error(
            &format!("\nFailure: took {}.", format_duration(total_duration)),
            1,
        );
    } else {
        tracing::info!("Success: took {}.", format_duration(total_duration));
    }
}

fn exit_with_error(message: &str, code: i32) -> ! {
    if !message.is_empty() {
        tracing::error!("{:?}", message);
    }
    process::exit(code);
}

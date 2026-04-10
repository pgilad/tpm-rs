mod cli;
mod commands;
mod config;
mod error;
mod paths;
mod plugin;
mod tmux;
mod user_path;
mod version;

use clap::Parser;

pub use error::{AppError, Result};

pub fn run() -> Result<()> {
    let cli = cli::Cli::parse();
    commands::run(cli)
}

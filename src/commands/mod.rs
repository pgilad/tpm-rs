mod add;
mod cleanup;
mod doctor;
mod install;
mod list;
mod load;
mod migrate;
mod paths;
mod remove;
mod self_update;
mod sync;
mod update;

use std::env;

use crate::{
    cli::{Cli, Command},
    error::Result,
};

pub fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Paths { json } => {
            paths::run(cli.config.as_deref(), cli.plugins_dir.as_deref(), json)
        }
        Command::Load => load::run(cli.config.as_deref(), cli.plugins_dir.as_deref()),
        Command::Install => install::run(cli.config.as_deref(), cli.plugins_dir.as_deref()),
        Command::Update { plugins } => {
            update::run(cli.config.as_deref(), cli.plugins_dir.as_deref(), &plugins)
        }
        Command::SelfUpdate => self_update::run(),
        Command::Cleanup => cleanup::run(cli.config.as_deref(), cli.plugins_dir.as_deref()),
        Command::List { json } => {
            list::run(cli.config.as_deref(), cli.plugins_dir.as_deref(), json)
        }
        Command::Doctor { json } => {
            doctor::run(cli.config.as_deref(), cli.plugins_dir.as_deref(), json)
        }
        Command::Add {
            source,
            branch,
            reference,
            skip_install,
        } => add::run(
            cli.config.as_deref(),
            cli.plugins_dir.as_deref(),
            &source,
            branch.as_deref(),
            reference.as_deref(),
            !skip_install,
        ),
        Command::Migrate { tmux_conf } => migrate::run(cli.config.as_deref(), tmux_conf.as_deref()),
        Command::Remove { name } => remove::run(cli.config.as_deref(), &name),
    }
}

fn current_dir() -> Result<std::path::PathBuf> {
    env::current_dir().map_err(crate::error::AppError::CurrentDirectory)
}

pub(crate) fn resolved_paths(
    config_override: Option<&std::path::Path>,
    plugins_override: Option<&std::path::Path>,
) -> Result<crate::paths::ResolvedPaths> {
    let cwd = current_dir()?;
    crate::paths::resolve(crate::paths::ResolveOptions {
        cwd: &cwd,
        config_override,
        plugins_override,
    })
}

pub(crate) fn base_paths(
    config_override: Option<&std::path::Path>,
    plugins_override: Option<&std::path::Path>,
) -> Result<crate::paths::ResolvedPaths> {
    let cwd = current_dir()?;
    crate::paths::resolve_base(crate::paths::ResolveOptions {
        cwd: &cwd,
        config_override,
        plugins_override,
    })
}

pub(crate) fn config_file_path(
    config_override: Option<&std::path::Path>,
) -> Result<std::path::PathBuf> {
    let cwd = current_dir()?;
    crate::paths::resolve_config_file(crate::paths::ResolveOptions {
        cwd: &cwd,
        config_override,
        plugins_override: None,
    })
}

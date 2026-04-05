use std::path::Path;

use crate::{
    commands::config_file_path,
    config::Config,
    error::{AppError, Result},
    plugin,
};

pub fn run(config_override: Option<&Path>, name: &str) -> Result<()> {
    let config_path = config_file_path(config_override)?;
    let mut config =
        Config::load_if_exists(&config_path)?.ok_or_else(|| AppError::ConfigNotFound {
            path: config_path.clone(),
        })?;

    let removed = config.remove_plugin(name)?;
    let install_name = plugin::install_name(&removed.source)?;
    config.save(&config_path)?;

    println!("Removed {} from {}", install_name, config_path.display());
    Ok(())
}

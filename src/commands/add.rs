use std::path::Path;

use super::{config_file_path, install, resolved_paths, sync};
use crate::{config::Config, error::Result, user_path::display_user_path};

pub fn run(
    config_override: Option<&Path>,
    plugins_override: Option<&Path>,
    source: &str,
    branch: Option<&str>,
    reference: Option<&str>,
    install_after_add: bool,
) -> Result<()> {
    let config_path = config_file_path(config_override)?;
    let created = !config_path.exists();

    let mut config = Config::load_if_exists(&config_path)?.unwrap_or_default();
    let install_name = config.add_plugin(source, branch, reference)?;
    let added_plugin = config
        .plugins
        .last()
        .cloned()
        .expect("added plugin should be present in config");
    config.save(&config_path)?;

    if created {
        println!(
            "Created {} and added {}",
            display_user_path(&config_path),
            install_name
        );
    } else {
        println!(
            "Added {} to {}",
            install_name,
            display_user_path(&config_path)
        );
    }

    if install_after_add {
        let paths = resolved_paths(config_override, plugins_override)?;
        let plugin = sync::configured_plugin(&paths, &added_plugin)?;
        let plugins = [plugin];
        install::run_plugins(&paths, &plugins)?;
    }

    Ok(())
}

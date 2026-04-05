use serde::Serialize;

use crate::{
    commands::{resolved_paths, sync},
    config::Config,
    error::{AppError, Result},
    plugin,
};

#[derive(Debug, Serialize)]
struct ListItem {
    name: String,
    source: String,
    branch: Option<String>,
    reference: Option<String>,
    enabled: bool,
    installed: bool,
    install_dir: std::path::PathBuf,
}

pub fn run(
    config_override: Option<&std::path::Path>,
    plugins_override: Option<&std::path::Path>,
    json: bool,
) -> Result<()> {
    let paths = resolved_paths(config_override, plugins_override)?;
    let config =
        Config::load_if_exists(&paths.config_file)?.ok_or_else(|| AppError::ConfigNotFound {
            path: paths.config_file.clone(),
        })?;

    let items = config
        .plugins
        .iter()
        .map(|plugin_config| {
            let name = plugin::install_name(&plugin_config.source)?;
            let install_dir = plugin::install_dir(&paths.plugins_dir, &plugin_config.source)?;
            let clone_source =
                sync::resolve_clone_source(&plugin_config.source, &paths.config_dir)?;
            let installed = sync::validate_managed_checkout(&install_dir, &clone_source).is_ok();

            Ok(ListItem {
                name,
                source: plugin_config.source.clone(),
                branch: plugin_config.branch.clone(),
                reference: plugin_config.reference.clone(),
                enabled: plugin_config.enabled,
                installed,
                install_dir,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    if json {
        println!("{}", serde_json::to_string_pretty(&items)?);
        return Ok(());
    }

    if items.is_empty() {
        println!("No plugins configured");
        return Ok(());
    }

    let name_width = items.iter().map(|item| item.name.len()).max().unwrap_or(0);
    for item in items {
        let enabled = if item.enabled { "enabled" } else { "disabled" };
        let installed = if item.installed {
            "installed"
        } else {
            "missing"
        };
        let branch = item.branch.as_deref().unwrap_or("-");
        let reference = item.reference.as_deref().unwrap_or("-");
        println!(
            "{name:<name_width$}  {enabled:<8}  {installed:<9}  branch={branch}  ref={reference}  source={source}",
            name = item.name,
            source = item.source,
        );
    }

    Ok(())
}

use std::{fs, path::Path};

use crate::{
    commands::sync::{self, SyncPlugin},
    error::{AppError, Result},
    paths::ResolvedPaths,
};

#[derive(Debug, Default)]
struct InstallReport {
    events: Vec<InstallEvent>,
    failed_operations: usize,
}

enum InstallOutcome {
    Installed(std::path::PathBuf),
    AlreadyInstalled(std::path::PathBuf),
}

#[derive(Debug)]
enum InstallEvent {
    Installed(String, std::path::PathBuf),
    Skipped(String, std::path::PathBuf),
    Failed(String, String),
}

pub fn run(config_override: Option<&Path>, plugins_override: Option<&Path>) -> Result<()> {
    let context = sync::install_context(config_override, plugins_override)?;
    run_plugins(&context.paths, &context.plugins)
}

pub(crate) fn run_plugins(paths: &ResolvedPaths, plugins: &[SyncPlugin]) -> Result<()> {
    if plugins.is_empty() {
        println!("No plugins selected for install");
        return Ok(());
    }

    sync::ensure_directory(&paths.plugins_dir)?;

    let mut report = InstallReport::default();
    for plugin in plugins {
        match install_plugin(plugin) {
            Ok(InstallOutcome::Installed(path)) => {
                report
                    .events
                    .push(InstallEvent::Installed(plugin.install_name.clone(), path));
            }
            Ok(InstallOutcome::AlreadyInstalled(path)) => {
                report
                    .events
                    .push(InstallEvent::Skipped(plugin.install_name.clone(), path));
            }
            Err(error) => {
                report.failed_operations += 1;
                report
                    .events
                    .push(InstallEvent::Failed(plugin.install_name.clone(), error));
            }
        }
    }

    print_report(&report);

    if report.failed_operations == 0 {
        Ok(())
    } else {
        Err(AppError::CommandFailed {
            command: "install",
            failed_operations: report.failed_operations,
        })
    }
}

fn install_plugin(plugin: &SyncPlugin) -> std::result::Result<InstallOutcome, String> {
    if plugin.install_dir.exists() {
        sync::validate_managed_checkout(&plugin.install_dir, &plugin.clone_source)
            .map(|()| InstallOutcome::AlreadyInstalled(plugin.install_dir.clone()))
            .map_err(|error| error.to_string())
    } else {
        let result = install_plugin_inner(plugin);
        if result.is_err()
            && plugin.install_dir.exists()
            && let Err(source) = fs::remove_dir_all(&plugin.install_dir)
        {
            return Err(format!(
                "{}; also failed to remove partial checkout {}: {}",
                result.expect_err("install should have failed"),
                plugin.install_dir.display(),
                source
            ));
        }

        result.map(|()| InstallOutcome::Installed(plugin.install_dir.clone()))
    }
}

fn install_plugin_inner(plugin: &SyncPlugin) -> std::result::Result<(), String> {
    if let Some(parent) = plugin.install_dir.parent() {
        sync::ensure_directory(parent).map_err(|error| error.to_string())?;
    }

    let install_dir = plugin.install_dir.to_string_lossy().into_owned();
    let clone_args = vec![
        "clone".to_string(),
        plugin.clone_source.clone(),
        install_dir,
    ];
    sync::git(None, &clone_args).map_err(|error| error.to_string())?;

    if plugin.branch.is_some() || plugin.reference.is_some() {
        sync::fetch_origin(&plugin.install_dir).map_err(|error| error.to_string())?;
    }

    match (plugin.branch.as_deref(), plugin.reference.as_deref()) {
        (Some(branch), None) => {
            if !sync::remote_branch_exists(&plugin.install_dir, branch) {
                return Err(format!(
                    "configured branch `{branch}` is not available as a remote branch"
                ));
            }

            sync::checkout_branch(&plugin.install_dir, branch)
                .map_err(|error| error.to_string())?;
            sync::fast_forward_branch(&plugin.install_dir, branch)
                .map_err(|error| error.to_string())?;
        }
        (None, Some(reference)) => {
            sync::checkout_pinned_reference(&plugin.install_dir, reference)?;
        }
        (None, None) => {}
        (Some(_), Some(_)) => {
            return Err("plugin configuration cannot set both `branch` and `ref`".to_string());
        }
    }

    sync::update_submodules(&plugin.install_dir).map_err(|error| error.to_string())
}

fn print_report(report: &InstallReport) {
    for event in &report.events {
        match event {
            InstallEvent::Installed(name, path) => {
                println!("Installed {name} into {}", path.display());
            }
            InstallEvent::Skipped(name, path) => {
                println!("Skipped already installed {name} at {}", path.display());
            }
            InstallEvent::Failed(name, error) => {
                eprintln!("Failed to install {name}: {error}");
            }
        }
    }
}

use std::path::Path;

use crate::{
    commands::sync::{self, SyncPlugin},
    error::{AppError, Result},
};

#[derive(Debug, Default)]
struct UpdateReport {
    events: Vec<UpdateEvent>,
    failed_operations: usize,
}

enum UpdateOutcome {
    Updated(std::path::PathBuf),
    AlreadyCurrent(std::path::PathBuf),
    Pinned(std::path::PathBuf, String),
    RealignedPinned(std::path::PathBuf, String),
}

#[derive(Debug)]
enum UpdateEvent {
    Updated(String, std::path::PathBuf),
    AlreadyCurrent(String, std::path::PathBuf),
    Pinned(String, std::path::PathBuf, String),
    RealignedPinned(String, std::path::PathBuf, String),
    Failed(String, String),
}

pub fn run(
    config_override: Option<&Path>,
    plugins_override: Option<&Path>,
    selectors: &[String],
) -> Result<()> {
    let context = sync::update_context(config_override, plugins_override, selectors)?;
    if context.plugins.is_empty() {
        println!("No plugins selected for update");
        return Ok(());
    }

    let mut report = UpdateReport::default();
    for plugin in &context.plugins {
        match update_plugin(plugin) {
            Ok(UpdateOutcome::Updated(path)) => {
                report
                    .events
                    .push(UpdateEvent::Updated(plugin.install_name.clone(), path));
            }
            Ok(UpdateOutcome::AlreadyCurrent(path)) => {
                report.events.push(UpdateEvent::AlreadyCurrent(
                    plugin.install_name.clone(),
                    path,
                ));
            }
            Ok(UpdateOutcome::Pinned(path, reference)) => {
                report.events.push(UpdateEvent::Pinned(
                    plugin.install_name.clone(),
                    path,
                    reference,
                ));
            }
            Ok(UpdateOutcome::RealignedPinned(path, reference)) => {
                report.events.push(UpdateEvent::RealignedPinned(
                    plugin.install_name.clone(),
                    path,
                    reference,
                ));
            }
            Err(error) => {
                report.failed_operations += 1;
                report
                    .events
                    .push(UpdateEvent::Failed(plugin.install_name.clone(), error));
            }
        }
    }

    print_report(&report);

    if report.failed_operations == 0 {
        Ok(())
    } else {
        Err(AppError::CommandFailed {
            command: "update",
            failed_operations: report.failed_operations,
        })
    }
}

fn update_plugin(plugin: &SyncPlugin) -> std::result::Result<UpdateOutcome, String> {
    sync::validate_managed_checkout(&plugin.install_dir, &plugin.clone_source)
        .map_err(|error| error.to_string())?;

    if sync::git_is_dirty(&plugin.install_dir).map_err(|error| error.to_string())? {
        return Err(format!(
            "plugin checkout has uncommitted tracked changes: {}",
            plugin.install_dir.display()
        ));
    }

    let before = sync::git_head_commit(&plugin.install_dir).map_err(|error| error.to_string())?;

    sync::fetch_origin(&plugin.install_dir).map_err(|error| error.to_string())?;

    match (plugin.branch.as_deref(), plugin.reference.as_deref()) {
        (Some(branch), None) => update_branch(plugin, branch, &before),
        (None, Some(reference)) => update_pinned(plugin, reference, &before),
        (None, None) => {
            let branch = sync::default_branch(&plugin.install_dir)?;
            update_branch(plugin, &branch, &before)
        }
        (Some(_), Some(_)) => {
            Err("plugin configuration cannot set both `branch` and `ref`".to_string())
        }
    }
}

fn update_branch(
    plugin: &SyncPlugin,
    branch: &str,
    before: &str,
) -> std::result::Result<UpdateOutcome, String> {
    if !sync::remote_branch_exists(&plugin.install_dir, branch) {
        return Err(format!(
            "configured branch `{branch}` is not available as a remote branch"
        ));
    }

    sync::checkout_branch(&plugin.install_dir, branch).map_err(|error| error.to_string())?;
    sync::fast_forward_branch(&plugin.install_dir, branch).map_err(|error| error.to_string())?;
    sync::update_submodules(&plugin.install_dir).map_err(|error| error.to_string())?;

    let after = sync::git_head_commit(&plugin.install_dir).map_err(|error| error.to_string())?;
    if before == after {
        Ok(UpdateOutcome::AlreadyCurrent(plugin.install_dir.clone()))
    } else {
        Ok(UpdateOutcome::Updated(plugin.install_dir.clone()))
    }
}

fn update_pinned(
    plugin: &SyncPlugin,
    reference: &str,
    before: &str,
) -> std::result::Result<UpdateOutcome, String> {
    sync::checkout_pinned_reference(&plugin.install_dir, reference)?;
    sync::update_submodules(&plugin.install_dir).map_err(|error| error.to_string())?;

    let after = sync::git_head_commit(&plugin.install_dir).map_err(|error| error.to_string())?;
    if before == after {
        Ok(UpdateOutcome::Pinned(
            plugin.install_dir.clone(),
            reference.to_string(),
        ))
    } else {
        Ok(UpdateOutcome::RealignedPinned(
            plugin.install_dir.clone(),
            reference.to_string(),
        ))
    }
}

fn print_report(report: &UpdateReport) {
    for event in &report.events {
        match event {
            UpdateEvent::Updated(name, path) => {
                println!("Updated {name} in {}", path.display());
            }
            UpdateEvent::AlreadyCurrent(name, path) => {
                println!("Already up to date {name} at {}", path.display());
            }
            UpdateEvent::Pinned(name, path, reference) => {
                println!(
                    "Kept pinned {name} at ref {reference} in {}",
                    path.display()
                );
            }
            UpdateEvent::RealignedPinned(name, path, reference) => {
                println!(
                    "Realigned pinned {name} to ref {reference} in {}",
                    path.display()
                );
            }
            UpdateEvent::Failed(name, error) => {
                eprintln!("Failed to update {name}: {error}");
            }
        }
    }
}

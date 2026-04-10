use std::{
    io::{self, IsTerminal},
    path::Path,
    time::Instant,
};

use crate::{
    commands::{
        progress::{ProgressStream, display_user_path, format_duration, indent_detail, pluralize},
        sync::{self, SyncPlugin},
    },
    error::{AppError, Result},
    paths::ResolvedPaths,
};

#[derive(Debug, Default)]
struct UpdateReport {
    events: Vec<UpdateEvent>,
    updated_count: usize,
    already_current_count: usize,
    pinned_count: usize,
    realigned_count: usize,
    failed_count: usize,
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

impl UpdateReport {
    fn push(&mut self, event: UpdateEvent) {
        match &event {
            UpdateEvent::Updated(_, _) => self.updated_count += 1,
            UpdateEvent::AlreadyCurrent(_, _) => self.already_current_count += 1,
            UpdateEvent::Pinned(_, _, _) => self.pinned_count += 1,
            UpdateEvent::RealignedPinned(_, _, _) => self.realigned_count += 1,
            UpdateEvent::Failed(_, _) => self.failed_count += 1,
        }

        self.events.push(event);
    }
}

#[derive(Debug)]
struct HumanUpdateUi {
    stream: ProgressStream,
    started_at: Instant,
}

#[derive(Debug)]
struct UpdateUi {
    human: Option<HumanUpdateUi>,
    emit_machine_report: bool,
}

impl UpdateUi {
    fn detect() -> Self {
        let stdout_is_terminal = io::stdout().is_terminal();
        let stderr_is_terminal = io::stderr().is_terminal();
        let human = if stderr_is_terminal {
            Some(HumanUpdateUi {
                stream: ProgressStream::Stderr,
                started_at: Instant::now(),
            })
        } else if stdout_is_terminal {
            Some(HumanUpdateUi {
                stream: ProgressStream::Stdout,
                started_at: Instant::now(),
            })
        } else {
            None
        };

        Self {
            human,
            emit_machine_report: !stdout_is_terminal,
        }
    }

    fn begin(&self, plugins_dir: &Path, total: usize) {
        if let Some(human) = &self.human {
            human.begin(plugins_dir, total);
        }
    }

    fn start_plugin(&self, index: usize, total: usize, name: &str) {
        if let Some(human) = &self.human {
            human.start_plugin(index, total, name);
        }
    }

    fn finish_plugin(&self, event: &UpdateEvent) {
        if let Some(human) = &self.human {
            human.finish_plugin(event);
        }
    }

    fn finish(&self, report: &UpdateReport) {
        if let Some(human) = &self.human {
            human.finish(report);
        }

        if self.emit_machine_report {
            print_machine_report(report);
        }
    }
}

impl HumanUpdateUi {
    fn begin(&self, plugins_dir: &Path, total: usize) {
        self.write_line(&format!(
            "Updating {total} {} in {}",
            pluralize(total, "plugin"),
            display_user_path(plugins_dir)
        ));
    }

    fn start_plugin(&self, index: usize, total: usize, name: &str) {
        self.write(&format!("  [{index}/{total}] {name}..."));
    }

    fn finish_plugin(&self, event: &UpdateEvent) {
        match event {
            UpdateEvent::Updated(_, _) => self.write_line(" updated"),
            UpdateEvent::AlreadyCurrent(_, _) => self.write_line(" already up to date"),
            UpdateEvent::Pinned(_, _, reference) => {
                self.write_line(&format!(" pinned to ref {reference}"));
            }
            UpdateEvent::RealignedPinned(_, _, reference) => {
                self.write_line(&format!(" realigned to ref {reference}"));
            }
            UpdateEvent::Failed(_, error) => {
                self.write_line(" failed");
                self.write_line(&format!("         {}", indent_detail(error)));
            }
        }
    }

    fn finish(&self, report: &UpdateReport) {
        self.write_line(&format!(
            "Done in {}. {} updated, {} already up to date, {} pinned, {} realigned, {} failed.",
            format_duration(self.started_at.elapsed()),
            report.updated_count,
            report.already_current_count,
            report.pinned_count,
            report.realigned_count,
            report.failed_count
        ));
    }

    fn write(&self, message: &str) {
        self.stream.write(message);
    }

    fn write_line(&self, message: &str) {
        self.stream.write_line(message);
    }
}

pub fn run(
    config_override: Option<&Path>,
    plugins_override: Option<&Path>,
    selectors: &[String],
) -> Result<()> {
    let context = sync::update_context(config_override, plugins_override, selectors)?;
    run_plugins(&context.paths, &context.plugins)
}

pub(crate) fn run_plugins(paths: &ResolvedPaths, plugins: &[SyncPlugin]) -> Result<()> {
    if plugins.is_empty() {
        println!("No plugins selected for update");
        return Ok(());
    }

    let ui = UpdateUi::detect();
    ui.begin(&paths.plugins_dir, plugins.len());

    let mut report = UpdateReport::default();
    for (index, plugin) in plugins.iter().enumerate() {
        ui.start_plugin(index + 1, plugins.len(), &plugin.install_name);

        let event = match update_plugin(plugin) {
            Ok(UpdateOutcome::Updated(path)) => {
                UpdateEvent::Updated(plugin.install_name.clone(), path)
            }
            Ok(UpdateOutcome::AlreadyCurrent(path)) => {
                UpdateEvent::AlreadyCurrent(plugin.install_name.clone(), path)
            }
            Ok(UpdateOutcome::Pinned(path, reference)) => {
                UpdateEvent::Pinned(plugin.install_name.clone(), path, reference)
            }
            Ok(UpdateOutcome::RealignedPinned(path, reference)) => {
                UpdateEvent::RealignedPinned(plugin.install_name.clone(), path, reference)
            }
            Err(error) => UpdateEvent::Failed(plugin.install_name.clone(), error),
        };

        ui.finish_plugin(&event);
        report.push(event);
    }

    ui.finish(&report);

    if report.failed_count == 0 {
        Ok(())
    } else {
        Err(AppError::CommandFailed {
            command: "update",
            failed_operations: report.failed_count,
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

fn print_machine_report(report: &UpdateReport) {
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

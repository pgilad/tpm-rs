use std::{
    collections::BTreeSet,
    io::{self, IsTerminal},
    path::{Path, PathBuf},
    time::Instant,
};

use crate::{
    config::Config,
    error::{AppError, Result},
    plugin,
};

use super::{
    cleanup,
    install::{self, InstallOutcome},
    progress::{
        ProgressStream, TerminalTheme, display_user_path, format_duration, indent_detail, pluralize,
    },
    resolved_paths, sync,
    update::{self, UpdateOutcome},
};

#[derive(Debug, Default)]
struct SyncReport {
    events: Vec<SyncEvent>,
    removed_count: usize,
    installed_count: usize,
    updated_count: usize,
    already_current_count: usize,
    pinned_count: usize,
    realigned_count: usize,
    failed_count: usize,
}

#[derive(Debug)]
enum SyncEvent {
    Removed(PathBuf),
    PreservedLegacyCheckout(PathBuf),
    CleanupFailed(PathBuf, String),
    Installed(String, PathBuf),
    Updated(String, PathBuf),
    AlreadyCurrent(String, PathBuf),
    Pinned(String, PathBuf, String),
    RealignedPinned(String, PathBuf, String),
    InstallFailed(String, String),
    UpdateFailed(String, String),
}

impl SyncReport {
    fn push(&mut self, event: SyncEvent) {
        match &event {
            SyncEvent::Removed(_) => self.removed_count += 1,
            SyncEvent::Installed(_, _) => self.installed_count += 1,
            SyncEvent::Updated(_, _) => self.updated_count += 1,
            SyncEvent::AlreadyCurrent(_, _) => self.already_current_count += 1,
            SyncEvent::Pinned(_, _, _) => self.pinned_count += 1,
            SyncEvent::RealignedPinned(_, _, _) => self.realigned_count += 1,
            SyncEvent::CleanupFailed(_, _)
            | SyncEvent::InstallFailed(_, _)
            | SyncEvent::UpdateFailed(_, _) => self.failed_count += 1,
            SyncEvent::PreservedLegacyCheckout(_) => {}
        }

        self.events.push(event);
    }
}

#[derive(Debug)]
struct HumanSyncUi {
    stream: ProgressStream,
    theme: TerminalTheme,
    started_at: Instant,
}

#[derive(Debug)]
struct SyncUi {
    human: Option<HumanSyncUi>,
    emit_machine_report: bool,
}

impl SyncUi {
    fn detect() -> Self {
        let stdout_is_terminal = io::stdout().is_terminal();
        let stderr_is_terminal = io::stderr().is_terminal();
        let human_stream = if stderr_is_terminal {
            Some(ProgressStream::Stderr)
        } else if stdout_is_terminal {
            Some(ProgressStream::Stdout)
        } else {
            None
        };
        let human = human_stream.map(|stream| HumanSyncUi {
            stream,
            theme: TerminalTheme::detect(stream),
            started_at: Instant::now(),
        });

        Self {
            human,
            emit_machine_report: !stdout_is_terminal,
        }
    }

    fn emit_cleanup_event(&self, event: &SyncEvent) {
        if let Some(human) = &self.human {
            human.emit_cleanup_event(event);
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

    fn finish_plugin(&self, event: &SyncEvent) {
        if let Some(human) = &self.human {
            human.finish_plugin(event);
        }
    }

    fn nothing_to_do(&self) {
        if let Some(human) = &self.human {
            human.write_line("Nothing to sync");
        }

        if self.emit_machine_report {
            println!("Nothing to sync");
        }
    }

    fn finish(&self, report: &SyncReport) {
        if let Some(human) = &self.human {
            human.finish(report);
        }

        if self.emit_machine_report {
            print_machine_report(report);
        }
    }
}

impl HumanSyncUi {
    fn emit_cleanup_event(&self, event: &SyncEvent) {
        match event {
            SyncEvent::Removed(path) => self.write_line(&format!(
                "{} {}",
                self.theme.success("Removed stale plugin directory"),
                display_user_path(path)
            )),
            SyncEvent::PreservedLegacyCheckout(path) => self.write_line(&format!(
                "{} {}",
                self.theme.warning("Preserved legacy TPM checkout"),
                display_user_path(path)
            )),
            SyncEvent::CleanupFailed(path, error) => {
                self.write_line(&self.theme.failure(&format!(
                    "Failed to remove stale plugin directory {}: {error}",
                    display_user_path(path)
                )))
            }
            SyncEvent::Installed(_, _)
            | SyncEvent::Updated(_, _)
            | SyncEvent::AlreadyCurrent(_, _)
            | SyncEvent::Pinned(_, _, _)
            | SyncEvent::RealignedPinned(_, _, _)
            | SyncEvent::InstallFailed(_, _)
            | SyncEvent::UpdateFailed(_, _) => {}
        }
    }

    fn begin(&self, plugins_dir: &Path, total: usize) {
        self.write_line(&format!(
            "Syncing {total} {} in {}",
            pluralize(total, "plugin"),
            display_user_path(plugins_dir)
        ));
    }

    fn start_plugin(&self, index: usize, total: usize, name: &str) {
        self.write(&format!("  [{index}/{total}] {name}..."));
    }

    fn finish_plugin(&self, event: &SyncEvent) {
        match event {
            SyncEvent::Installed(_, _) => {
                self.write_line(&format!(" {}", self.theme.success("installed")))
            }
            SyncEvent::Updated(_, _) => {
                self.write_line(&format!(" {}", self.theme.success("updated")))
            }
            SyncEvent::AlreadyCurrent(_, _) => {
                self.write_line(&format!(" {}", self.theme.warning("already up to date")))
            }
            SyncEvent::Pinned(_, _, reference) => self.write_line(&format!(
                " {}",
                self.theme.warning(&format!("pinned to ref {reference}"))
            )),
            SyncEvent::RealignedPinned(_, _, reference) => self.write_line(&format!(
                " {}",
                self.theme.info(&format!("realigned to ref {reference}"))
            )),
            SyncEvent::InstallFailed(_, error) | SyncEvent::UpdateFailed(_, error) => {
                self.write_line(&format!(" {}", self.theme.failure("failed")));
                self.write_line(
                    &self
                        .theme
                        .failure(&format!("         {}", indent_detail(error))),
                );
            }
            SyncEvent::Removed(_)
            | SyncEvent::PreservedLegacyCheckout(_)
            | SyncEvent::CleanupFailed(_, _) => {}
        }
    }

    fn finish(&self, report: &SyncReport) {
        let failed = if report.failed_count == 0 {
            format!("{} failed", report.failed_count)
        } else {
            self.theme
                .failure(&format!("{} failed", report.failed_count))
        };

        self.write_line(&format!(
            "Done in {}. {}, {}, {}, {}, {}, {}, {}.",
            format_duration(self.started_at.elapsed()),
            self.theme
                .success(&format!("{} removed", report.removed_count)),
            self.theme
                .success(&format!("{} installed", report.installed_count)),
            self.theme
                .success(&format!("{} updated", report.updated_count)),
            self.theme.warning(&format!(
                "{} already up to date",
                report.already_current_count
            )),
            self.theme
                .warning(&format!("{} pinned", report.pinned_count)),
            self.theme
                .info(&format!("{} realigned", report.realigned_count)),
            failed,
        ));
    }

    fn write(&self, message: &str) {
        self.stream.write(message);
    }

    fn write_line(&self, message: &str) {
        self.stream.write_line(message);
    }
}

pub fn run(config_override: Option<&Path>, plugins_override: Option<&Path>) -> Result<()> {
    let paths = resolved_paths(config_override, plugins_override)?;
    let config =
        Config::load_if_exists(&paths.config_file)?.ok_or_else(|| AppError::ConfigNotFound {
            path: paths.config_file.clone(),
        })?;

    let declared = config
        .plugins
        .iter()
        .map(|plugin_config| plugin::install_name(&plugin_config.source).map(PathBuf::from))
        .collect::<Result<BTreeSet<_>>>()?;
    let plugins = config
        .plugins
        .iter()
        .filter(|plugin_config| plugin_config.enabled)
        .map(|plugin_config| sync::configured_plugin(&paths, plugin_config))
        .collect::<Result<Vec<_>>>()?;

    let ui = SyncUi::detect();
    let mut report = SyncReport::default();

    if !plugins.is_empty() {
        ui.begin(&paths.plugins_dir, plugins.len());
    }

    let cleanup_report = cleanup::cleanup_plugins_dir(&paths.plugins_dir, &declared)?;
    push_cleanup_events(&ui, &mut report, cleanup_report);

    if plugins.is_empty() && report.events.is_empty() {
        ui.nothing_to_do();
        return Ok(());
    }

    for (index, plugin) in plugins.iter().enumerate() {
        ui.start_plugin(index + 1, plugins.len(), &plugin.install_name);

        let event = if plugin.install_dir.exists() {
            match update::update_plugin(plugin) {
                Ok(UpdateOutcome::Updated(path)) => {
                    SyncEvent::Updated(plugin.install_name.clone(), path)
                }
                Ok(UpdateOutcome::AlreadyCurrent(path)) => {
                    SyncEvent::AlreadyCurrent(plugin.install_name.clone(), path)
                }
                Ok(UpdateOutcome::Pinned(path, reference)) => {
                    SyncEvent::Pinned(plugin.install_name.clone(), path, reference)
                }
                Ok(UpdateOutcome::RealignedPinned(path, reference)) => {
                    SyncEvent::RealignedPinned(plugin.install_name.clone(), path, reference)
                }
                Err(error) => SyncEvent::UpdateFailed(plugin.install_name.clone(), error),
            }
        } else {
            match install::install_plugin(plugin) {
                Ok(InstallOutcome::Installed(path)) => {
                    SyncEvent::Installed(plugin.install_name.clone(), path)
                }
                Ok(InstallOutcome::AlreadyInstalled(path)) => {
                    SyncEvent::AlreadyCurrent(plugin.install_name.clone(), path)
                }
                Err(error) => SyncEvent::InstallFailed(plugin.install_name.clone(), error),
            }
        };

        ui.finish_plugin(&event);
        report.push(event);
    }

    ui.finish(&report);

    if report.failed_count == 0 {
        Ok(())
    } else {
        Err(AppError::CommandFailed {
            command: "sync",
            failed_operations: report.failed_count,
        })
    }
}

fn push_cleanup_events(
    ui: &SyncUi,
    report: &mut SyncReport,
    cleanup_report: cleanup::CleanupReport,
) {
    for path in cleanup_report.removed {
        let event = SyncEvent::Removed(path);
        ui.emit_cleanup_event(&event);
        report.push(event);
    }

    for path in cleanup_report.preserved {
        let event = SyncEvent::PreservedLegacyCheckout(path);
        ui.emit_cleanup_event(&event);
        report.push(event);
    }

    for (path, error) in cleanup_report.failed {
        let event = SyncEvent::CleanupFailed(path, error.to_string());
        ui.emit_cleanup_event(&event);
        report.push(event);
    }
}

fn print_machine_report(report: &SyncReport) {
    for event in &report.events {
        match event {
            SyncEvent::Removed(path) => {
                println!("Removed stale plugin directory {}", display_user_path(path));
            }
            SyncEvent::PreservedLegacyCheckout(path) => {
                println!("Preserved legacy TPM checkout {}", display_user_path(path));
            }
            SyncEvent::CleanupFailed(path, error) => {
                eprintln!(
                    "Failed to remove stale plugin directory {}: {error}",
                    display_user_path(path)
                );
            }
            SyncEvent::Installed(name, path) => {
                println!("Installed {name} into {}", display_user_path(path));
            }
            SyncEvent::Updated(name, path) => {
                println!("Updated {name} in {}", display_user_path(path));
            }
            SyncEvent::AlreadyCurrent(name, path) => {
                println!("Already up to date {name} at {}", display_user_path(path));
            }
            SyncEvent::Pinned(name, path, reference) => {
                println!(
                    "Kept pinned {name} at ref {reference} in {}",
                    display_user_path(path)
                );
            }
            SyncEvent::RealignedPinned(name, path, reference) => {
                println!(
                    "Realigned pinned {name} to ref {reference} in {}",
                    display_user_path(path)
                );
            }
            SyncEvent::InstallFailed(name, error) => {
                eprintln!("Failed to install {name}: {error}");
            }
            SyncEvent::UpdateFailed(name, error) => {
                eprintln!("Failed to update {name}: {error}");
            }
        }
    }
}

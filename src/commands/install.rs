use std::{
    fs,
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
struct InstallReport {
    events: Vec<InstallEvent>,
    installed_count: usize,
    skipped_count: usize,
    failed_count: usize,
}

pub(crate) enum InstallOutcome {
    Installed(std::path::PathBuf),
    AlreadyInstalled(std::path::PathBuf),
}

#[derive(Debug)]
enum InstallEvent {
    Installed(String, std::path::PathBuf),
    Skipped(String, std::path::PathBuf),
    Failed(String, String),
}

impl InstallReport {
    fn push(&mut self, event: InstallEvent) {
        match &event {
            InstallEvent::Installed(_, _) => self.installed_count += 1,
            InstallEvent::Skipped(_, _) => self.skipped_count += 1,
            InstallEvent::Failed(_, _) => self.failed_count += 1,
        }

        self.events.push(event);
    }
}

#[derive(Debug)]
struct HumanInstallUi {
    stream: ProgressStream,
    started_at: Instant,
}

#[derive(Debug)]
struct InstallUi {
    human: Option<HumanInstallUi>,
    emit_machine_report: bool,
}

impl InstallUi {
    fn detect() -> Self {
        let stdout_is_terminal = io::stdout().is_terminal();
        let stderr_is_terminal = io::stderr().is_terminal();
        let human = if stderr_is_terminal {
            Some(HumanInstallUi {
                stream: ProgressStream::Stderr,
                started_at: Instant::now(),
            })
        } else if stdout_is_terminal {
            Some(HumanInstallUi {
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

    fn finish_plugin(&self, event: &InstallEvent) {
        if let Some(human) = &self.human {
            human.finish_plugin(event);
        }
    }

    fn finish(&self, report: &InstallReport) {
        if let Some(human) = &self.human {
            human.finish(report);
        }

        if self.emit_machine_report {
            print_machine_report(report);
        }
    }
}

impl HumanInstallUi {
    fn begin(&self, plugins_dir: &Path, total: usize) {
        self.write_line(&format!(
            "Installing {total} {} into {}",
            pluralize(total, "plugin"),
            display_user_path(plugins_dir)
        ));
    }

    fn start_plugin(&self, index: usize, total: usize, name: &str) {
        self.write(&format!("  [{index}/{total}] {name}..."));
    }

    fn finish_plugin(&self, event: &InstallEvent) {
        match event {
            InstallEvent::Installed(_, _) => self.write_line(" installed"),
            InstallEvent::Skipped(_, _) => self.write_line(" already installed"),
            InstallEvent::Failed(_, error) => {
                self.write_line(" failed");
                self.write_line(&format!("         {}", indent_detail(error)));
            }
        }
    }

    fn finish(&self, report: &InstallReport) {
        self.write_line(&format!(
            "Done in {}. {} installed, {} already installed, {} failed.",
            format_duration(self.started_at.elapsed()),
            report.installed_count,
            report.skipped_count,
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

    let ui = InstallUi::detect();
    ui.begin(&paths.plugins_dir, plugins.len());

    let mut report = InstallReport::default();
    for (index, plugin) in plugins.iter().enumerate() {
        ui.start_plugin(index + 1, plugins.len(), &plugin.install_name);

        let event = match install_plugin(plugin) {
            Ok(InstallOutcome::Installed(path)) => {
                InstallEvent::Installed(plugin.install_name.clone(), path)
            }
            Ok(InstallOutcome::AlreadyInstalled(path)) => {
                InstallEvent::Skipped(plugin.install_name.clone(), path)
            }
            Err(error) => InstallEvent::Failed(plugin.install_name.clone(), error),
        };

        ui.finish_plugin(&event);
        report.push(event);
    }

    ui.finish(&report);

    if report.failed_count == 0 {
        Ok(())
    } else {
        Err(AppError::CommandFailed {
            command: "install",
            failed_operations: report.failed_count,
        })
    }
}

pub(crate) fn install_plugin(plugin: &SyncPlugin) -> std::result::Result<InstallOutcome, String> {
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
                display_user_path(&plugin.install_dir),
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

fn print_machine_report(report: &InstallReport) {
    for event in &report.events {
        match event {
            InstallEvent::Installed(name, path) => {
                println!("Installed {name} into {}", display_user_path(path));
            }
            InstallEvent::Skipped(name, path) => {
                println!(
                    "Skipped already installed {name} at {}",
                    display_user_path(path)
                );
            }
            InstallEvent::Failed(name, error) => {
                eprintln!("Failed to install {name}: {error}");
            }
        }
    }
}

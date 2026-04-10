use std::{
    env, fs,
    io::{self, IsTerminal, Write},
    path::Path,
    time::{Duration, Instant},
};

use crate::{
    commands::sync::{self, SyncPlugin},
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

#[derive(Debug, Clone, Copy)]
enum ProgressStream {
    Stdout,
    Stderr,
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
        match self.stream {
            ProgressStream::Stdout => {
                print!("{message}");
                let _ = io::stdout().flush();
            }
            ProgressStream::Stderr => {
                eprint!("{message}");
                let _ = io::stderr().flush();
            }
        }
    }

    fn write_line(&self, message: &str) {
        match self.stream {
            ProgressStream::Stdout => println!("{message}"),
            ProgressStream::Stderr => eprintln!("{message}"),
        }
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

fn print_machine_report(report: &InstallReport) {
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

fn pluralize(count: usize, singular: &str) -> String {
    if count == 1 {
        singular.to_string()
    } else {
        format!("{singular}s")
    }
}

fn display_user_path(path: &Path) -> String {
    let home = env::var_os("HOME").map(std::path::PathBuf::from);
    display_user_path_with_home(path, home.as_deref())
}

fn display_user_path_with_home(path: &Path, home: Option<&Path>) -> String {
    if let Some(home) = home
        && let Ok(relative) = path.strip_prefix(home)
    {
        return if relative.as_os_str().is_empty() {
            "~".to_string()
        } else {
            format!("~/{}", relative.display())
        };
    }

    path.display().to_string()
}

fn indent_detail(detail: &str) -> String {
    detail.replace('\n', "\n         ")
}

fn format_duration(duration: Duration) -> String {
    if duration.as_secs() >= 1 {
        format!("{:.1}s", duration.as_secs_f64())
    } else {
        format!("{}ms", duration.as_millis())
    }
}

#[cfg(test)]
mod tests {
    use std::{path::Path, time::Duration};

    use super::{display_user_path_with_home, format_duration, indent_detail};

    #[test]
    fn shortens_home_prefixed_paths_for_human_output() {
        assert_eq!(
            display_user_path_with_home(
                Path::new("/Users/pgilad/.local/share/tpm/plugins"),
                Some(Path::new("/Users/pgilad"))
            ),
            "~/.local/share/tpm/plugins"
        );
    }

    #[test]
    fn leaves_non_home_paths_unchanged_for_human_output() {
        assert_eq!(
            display_user_path_with_home(
                Path::new("/tmp/tpm/plugins"),
                Some(Path::new("/Users/pgilad"))
            ),
            "/tmp/tpm/plugins"
        );
    }

    #[test]
    fn indents_multiline_failure_details() {
        assert_eq!(indent_detail("first\nsecond"), "first\n         second");
    }

    #[test]
    fn formats_short_and_long_durations() {
        assert_eq!(format_duration(Duration::from_millis(240)), "240ms");
        assert_eq!(format_duration(Duration::from_millis(1250)), "1.2s");
    }
}

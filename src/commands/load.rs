use std::{
    env,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use sha2::{Digest, Sha256};

use crate::{
    commands::resolved_paths,
    config::Config,
    error::{AppError, Result},
    plugin, tmux,
    user_path::display_user_path,
};

const TMUX_PLUGIN_MANAGER_PATH: &str = "TMUX_PLUGIN_MANAGER_PATH";

#[derive(Debug, Default)]
struct LoadReport {
    failures: Vec<LoadFailure>,
}

#[derive(Debug)]
struct LoadFailure {
    context: String,
    detail: String,
}

#[derive(Debug, Default)]
struct LoadLogger {
    file: Option<File>,
}

impl LoadFailure {
    fn plugin(name: String, detail: String) -> Self {
        Self {
            context: name,
            detail,
        }
    }

    fn render(&self) -> String {
        format!("Failed to load {}: {}", self.context, self.detail)
    }
}

pub fn run(config_override: Option<&Path>, plugins_override: Option<&Path>) -> Result<()> {
    match run_inner(config_override, plugins_override) {
        Ok(()) => Ok(()),
        Err(error) => {
            if !matches!(
                error,
                AppError::CommandFailed {
                    command: "load",
                    ..
                }
            ) {
                let _ = tmux::display_message(&format!("[tpm] load failed: {error}"));
            }

            Err(error)
        }
    }
}

fn run_inner(config_override: Option<&Path>, plugins_override: Option<&Path>) -> Result<()> {
    let load_started = Instant::now();
    let paths = resolved_paths(config_override, plugins_override)?;
    let config =
        Config::load_if_exists(&paths.config_file)?.ok_or_else(|| AppError::ConfigNotFound {
            path: paths.config_file.clone(),
        })?;
    let manager_path = tmux::plugin_manager_path(&paths.plugins_dir);
    let mut logger = LoadLogger::new(&paths.state_dir);
    logger.log_run_start(&paths.config_file, &paths.plugins_dir);

    let mut report = LoadReport::default();
    // A stale TMUX environment should not block offline plugin loading.
    let _ = tmux::set_global_environment(TMUX_PLUGIN_MANAGER_PATH, &manager_path);

    let enabled_plugins = config
        .plugins
        .iter()
        .filter(|plugin| plugin.enabled)
        .collect::<Vec<_>>();
    logger.log_line(format!("enabled plugins: {}", enabled_plugins.len()));

    for plugin_config in enabled_plugins {
        let name = plugin::install_name(&plugin_config.source)?;
        let install_dir = plugin::install_dir(&paths.plugins_dir, &plugin_config.source)?;

        if let Err(detail) = load_plugin(&name, &install_dir, &manager_path, &mut logger) {
            report.failures.push(LoadFailure::plugin(name, detail));
        }
    }

    print_report(&report);

    if report.failures.is_empty() {
        logger.log_line(format!(
            "load completed successfully in {}",
            format_duration(load_started.elapsed())
        ));
        Ok(())
    } else {
        logger.log_line(format!(
            "load completed with {} failed operations in {}",
            report.failures.len(),
            format_duration(load_started.elapsed())
        ));
        let _ = tmux::display_message(&tmux_failure_message(&report));
        Err(AppError::CommandFailed {
            command: "load",
            failed_operations: report.failures.len(),
        })
    }
}

fn load_plugin(
    name: &str,
    install_dir: &Path,
    manager_path: &str,
    logger: &mut LoadLogger,
) -> std::result::Result<(), String> {
    let plugin_started = Instant::now();
    logger.log_line(format!(
        "plugin {name}: loading from {}",
        display_user_path(install_dir)
    ));

    if !install_dir.exists() {
        let detail = format!(
            "plugin checkout is missing at {}",
            display_user_path(install_dir)
        );
        logger.log_plugin_failure(name, &detail, plugin_started.elapsed());
        return Err(detail);
    }

    if !install_dir.is_dir() {
        let detail = format!(
            "expected plugin checkout directory at {}",
            display_user_path(install_dir)
        );
        logger.log_plugin_failure(name, &detail, plugin_started.elapsed());
        return Err(detail);
    }

    let entrypoints = match plugin::executable_entrypoints(install_dir) {
        Ok(entrypoints) => entrypoints,
        Err(error) => {
            let detail = error.to_string();
            logger.log_plugin_failure(name, &detail, plugin_started.elapsed());
            return Err(detail);
        }
    };
    if entrypoints.is_empty() {
        logger.log_line(format!(
            "plugin {name}: discovered no executable root *.tmux entrypoints"
        ));
        logger.log_line(format!(
            "plugin {name}: loaded successfully in {}",
            format_duration(plugin_started.elapsed())
        ));
        return Ok(());
    }

    for entrypoint in &entrypoints {
        logger.log_line(format!(
            "plugin {name}: discovered entrypoint {}",
            display_user_path(entrypoint)
        ));
    }

    for entrypoint in entrypoints {
        if let Err(detail) = run_entrypoint(name, &entrypoint, install_dir, manager_path, logger) {
            logger.log_plugin_failure(name, &detail, plugin_started.elapsed());
            return Err(detail);
        }
    }

    logger.log_line(format!(
        "plugin {name}: loaded successfully in {}",
        format_duration(plugin_started.elapsed())
    ));
    Ok(())
}

fn run_entrypoint(
    name: &str,
    entrypoint: &Path,
    plugin_dir: &Path,
    manager_path: &str,
    logger: &mut LoadLogger,
) -> std::result::Result<(), String> {
    let entrypoint_started = Instant::now();
    logger.log_line(format!(
        "plugin {name}: running entrypoint {}",
        display_user_path(entrypoint)
    ));

    let output = match Command::new(entrypoint)
        .current_dir(plugin_dir)
        .env(TMUX_PLUGIN_MANAGER_PATH, manager_path)
        .output()
    {
        Ok(output) => output,
        Err(source) => {
            let detail = format!(
                "failed to execute entrypoint {}: {}",
                display_user_path(entrypoint),
                source
            );
            logger.log_line(format!(
                "plugin {name}: entrypoint failed {} in {}: {}",
                display_user_path(entrypoint),
                format_duration(entrypoint_started.elapsed()),
                detail
            ));
            return Err(detail);
        }
    };

    if output.status.success() {
        logger.log_line(format!(
            "plugin {name}: entrypoint succeeded {} in {}",
            display_user_path(entrypoint),
            format_duration(entrypoint_started.elapsed())
        ));
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        String::new()
    };

    if detail.is_empty() {
        let detail = format!(
            "entrypoint {} exited with status {}",
            display_user_path(entrypoint),
            output.status
        );
        logger.log_line(format!(
            "plugin {name}: entrypoint failed {} in {}: {}",
            display_user_path(entrypoint),
            format_duration(entrypoint_started.elapsed()),
            detail
        ));
        Err(detail)
    } else {
        let detail = format!(
            "entrypoint {} exited with status {}: {}",
            display_user_path(entrypoint),
            output.status,
            detail
        );
        logger.log_line(format!(
            "plugin {name}: entrypoint failed {} in {}: {}",
            display_user_path(entrypoint),
            format_duration(entrypoint_started.elapsed()),
            detail
        ));
        Err(detail)
    }
}

impl LoadLogger {
    fn new(state_dir: &Path) -> Self {
        let Some(log_path) = load_log_path(state_dir) else {
            return Self::default();
        };

        if fs::create_dir_all(state_dir).is_err() {
            return Self::default();
        }

        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(log_path)
            .ok();

        Self { file }
    }

    fn log_run_start(&mut self, config_file: &Path, plugins_dir: &Path) {
        let Some(server_socket) = tmux_server_socket() else {
            return;
        };

        self.log_line(format!("load started at unix-seconds {}", unix_timestamp()));
        self.log_line(format!("tmux server socket: {server_socket}"));
        self.log_line(format!("config file: {}", display_user_path(config_file)));
        self.log_line(format!("plugins dir: {}", display_user_path(plugins_dir)));
    }

    fn log_line(&mut self, message: impl AsRef<str>) {
        let failed = match self.file.as_mut() {
            Some(file) => writeln!(file, "{}", message.as_ref())
                .and_then(|_| file.flush())
                .is_err(),
            None => false,
        };

        if failed {
            self.file = None;
        }
    }

    fn log_plugin_failure(&mut self, name: &str, detail: &str, elapsed: Duration) {
        self.log_line(format!(
            "plugin {name}: failed in {}: {detail}",
            format_duration(elapsed)
        ));
    }
}

fn load_log_path(state_dir: &Path) -> Option<PathBuf> {
    let server_socket = tmux_server_socket()?;
    let mut hasher = Sha256::new();
    hasher.update(server_socket.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    Some(state_dir.join(format!("load-{hash}.log")))
}

fn tmux_server_socket() -> Option<String> {
    let raw = env::var_os("TMUX")?;
    let raw = raw.to_string_lossy();
    let server_socket = raw.split(',').next().unwrap_or_default().trim();
    if server_socket.is_empty() {
        None
    } else {
        Some(server_socket.to_string())
    }
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn format_duration(duration: Duration) -> String {
    format!("{}ms", duration.as_millis())
}

fn print_report(report: &LoadReport) {
    for failure in &report.failures {
        eprintln!("{}", failure.render());
    }
}

fn tmux_failure_message(report: &LoadReport) -> String {
    match report.failures.as_slice() {
        [failure] => truncate_tmux_message(&format!("[tpm] {}", failure.render())),
        failures => format!(
            "[tpm] load failed for {} items; see stderr for details",
            failures.len()
        ),
    }
}

fn truncate_tmux_message(message: &str) -> String {
    const MAX_CHARS: usize = 180;

    let mut truncated = String::new();
    let mut characters = message.chars();
    for _ in 0..MAX_CHARS {
        match characters.next() {
            Some(character) => truncated.push(character),
            None => return truncated,
        }
    }

    if characters.next().is_some() {
        truncated.push_str("...");
    }

    truncated
}

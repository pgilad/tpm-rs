use std::{
    cmp::Ordering,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use serde::Serialize;

use crate::{
    commands::{base_paths, resolved_paths, sync},
    config::Config,
    error::{AppError, Result},
    paths::ResolvedPaths,
    plugin,
};

const MINIMUM_TMUX_VERSION: &str = "3.2";
const MINIMUM_GIT_VERSION: &str = "2.25.0";

#[derive(Debug, Serialize)]
pub struct DoctorReport {
    ok: bool,
    failing_checks: usize,
    paths: ResolvedPaths,
    checks: Vec<DoctorCheck>,
}

#[derive(Debug, Serialize)]
struct DoctorCheck {
    name: String,
    status: DoctorStatus,
    summary: String,
    detail: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum DoctorStatus {
    Pass,
    Fail,
    Skip,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NumericVersion(Vec<u64>);

pub fn run(
    config_override: Option<&Path>,
    plugins_override: Option<&Path>,
    json: bool,
) -> Result<()> {
    let report = build_report(config_override, plugins_override)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_human(&report);
    }

    if report.ok {
        Ok(())
    } else {
        Err(AppError::ChecksFailed {
            command: "doctor",
            failing_checks: report.failing_checks,
        })
    }
}

fn build_report(
    config_override: Option<&Path>,
    plugins_override: Option<&Path>,
) -> Result<DoctorReport> {
    let base_paths = base_paths(config_override, plugins_override)?;
    let mut checks = Vec::new();

    let config_result = inspect_config(&base_paths.config_file, &mut checks)?;
    let git_check = inspect_tool("git", &["--version"], MINIMUM_GIT_VERSION);
    let git_available = git_check.status == DoctorStatus::Pass;
    checks.push(git_check);
    checks.push(inspect_tool("tmux", &["-V"], MINIMUM_TMUX_VERSION));

    let effective_paths = if config_result.is_some() {
        resolved_paths(config_override, plugins_override)?
    } else {
        base_paths.clone()
    };

    checks.extend(path_checks(&effective_paths));

    if let Some(config) = config_result {
        checks.extend(plugin_checks(&config, &effective_paths, git_available)?);
    } else {
        checks.push(DoctorCheck::skip(
            "plugins",
            "skipped plugin checkout checks because config is missing or invalid",
        ));
    }

    let failing_checks = checks
        .iter()
        .filter(|check| check.status == DoctorStatus::Fail)
        .count();

    Ok(DoctorReport {
        ok: failing_checks == 0,
        failing_checks,
        paths: effective_paths,
        checks,
    })
}

fn inspect_config(path: &Path, checks: &mut Vec<DoctorCheck>) -> Result<Option<Config>> {
    if !path.exists() {
        checks.push(DoctorCheck::fail(
            "config_file",
            format!("missing config file at {}", path.display()),
        ));
        checks.push(DoctorCheck::skip(
            "config_schema",
            "skipped config schema validation because the config file is missing",
        ));
        return Ok(None);
    }

    checks.push(DoctorCheck::pass(
        "config_file",
        format!("found config file at {}", path.display()),
    ));

    match Config::load_if_exists(path) {
        Ok(Some(config)) => {
            checks.push(DoctorCheck::pass(
                "config_schema",
                format!(
                    "parsed config version {} with {} plugin(s)",
                    config.version,
                    config.plugins.len()
                ),
            ));
            Ok(Some(config))
        }
        Ok(None) => {
            checks.push(DoctorCheck::skip(
                "config_schema",
                "skipped config schema validation because the config file is missing",
            ));
            Ok(None)
        }
        Err(AppError::InvalidConfig { message, .. }) => {
            checks.push(DoctorCheck::fail(
                "config_schema",
                format!("invalid config schema: {message}"),
            ));
            Ok(None)
        }
        Err(error) => {
            checks.push(DoctorCheck::fail(
                "config_schema",
                format!("failed to load config: {error}"),
            ));
            Ok(None)
        }
    }
}

fn inspect_tool(name: &str, args: &[&str], minimum_version: &str) -> DoctorCheck {
    let output = match Command::new(name).args(args).output() {
        Ok(output) => output,
        Err(error) => {
            return DoctorCheck::fail(name, format!("failed to execute `{name}`: {error}"));
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let detail = if stderr.is_empty() {
            format!("`{name}` exited with status {}", output.status)
        } else {
            format!("`{name}` exited with status {}: {stderr}", output.status)
        };
        return DoctorCheck::fail(name, detail);
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let actual_version = match NumericVersion::parse(&stdout) {
        Some(version) => version,
        None => {
            return DoctorCheck::fail(name, format!("could not parse a version from `{stdout}`"));
        }
    };
    let minimum = NumericVersion::parse(minimum_version).expect("minimum version should parse");

    if actual_version < minimum {
        return DoctorCheck::fail(
            name,
            format!(
                "found {stdout}, which is older than the minimum supported version {minimum_version}"
            ),
        );
    }

    DoctorCheck::pass(
        name,
        format!("found {stdout} (minimum supported {minimum_version})"),
    )
}

fn path_checks(paths: &ResolvedPaths) -> Vec<DoctorCheck> {
    vec![
        inspect_directory_path("config_dir", &paths.config_dir),
        inspect_directory_path("data_dir", &paths.data_dir),
        inspect_directory_path("state_dir", &paths.state_dir),
        inspect_directory_path("cache_dir", &paths.cache_dir),
        inspect_directory_path("plugins_dir", &paths.plugins_dir),
    ]
}

fn inspect_directory_path(name: &str, path: &Path) -> DoctorCheck {
    if path.exists() {
        if !path.is_dir() {
            return DoctorCheck::fail(name, format!("expected a directory at {}", path.display()));
        }

        match fs::metadata(path) {
            Ok(metadata) if metadata.permissions().readonly() => DoctorCheck::fail(
                name,
                format!("directory exists but appears read-only: {}", path.display()),
            ),
            Ok(_) => DoctorCheck::pass(
                name,
                format!("directory exists and appears writable: {}", path.display()),
            ),
            Err(error) => DoctorCheck::fail(
                name,
                format!("failed to inspect directory {}: {error}", path.display()),
            ),
        }
    } else {
        let Some(parent) = nearest_existing_ancestor(path) else {
            return DoctorCheck::fail(
                name,
                format!(
                    "path does not exist and no existing parent could be found for {}",
                    path.display()
                ),
            );
        };

        match fs::metadata(&parent) {
            Ok(metadata) if metadata.permissions().readonly() => DoctorCheck::fail(
                name,
                format!(
                    "path does not exist and nearest existing parent appears read-only: {}",
                    parent.display()
                ),
            ),
            Ok(_) => DoctorCheck::pass(
                name,
                format!(
                    "path does not exist but nearest existing parent appears writable: {}",
                    parent.display()
                ),
            ),
            Err(error) => DoctorCheck::fail(
                name,
                format!("failed to inspect parent {}: {error}", parent.display()),
            ),
        }
    }
}

fn nearest_existing_ancestor(path: &Path) -> Option<PathBuf> {
    let mut current = path.to_path_buf();
    loop {
        if current.exists() {
            return Some(current);
        }

        if !current.pop() {
            return None;
        }
    }
}

fn plugin_checks(
    config: &Config,
    paths: &ResolvedPaths,
    git_available: bool,
) -> Result<Vec<DoctorCheck>> {
    config
        .plugins
        .iter()
        .map(|plugin_config| {
            let name = plugin::install_name(&plugin_config.source)?;
            let install_dir = plugin::install_dir(&paths.plugins_dir, &plugin_config.source)?;
            let check_name = format!("plugin/{name}");

            if !plugin_config.enabled {
                return Ok(DoctorCheck::skip(
                    check_name,
                    format!(
                        "plugin is disabled in config; expected install dir {}",
                        install_dir.display()
                    ),
                ));
            }

            if !install_dir.exists() {
                return Ok(DoctorCheck::fail(
                    check_name,
                    format!("missing plugin checkout at {}", install_dir.display()),
                ));
            }

            if !install_dir.is_dir() {
                return Ok(DoctorCheck::fail(
                    check_name,
                    format!(
                        "expected plugin checkout directory at {}",
                        install_dir.display()
                    ),
                ));
            }

            if !has_git_metadata(&install_dir) {
                return Ok(DoctorCheck::fail(
                    check_name,
                    format!(
                        "plugin checkout is not a git repository: {}",
                        install_dir.display()
                    ),
                ));
            }

            let clone_source =
                sync::resolve_clone_source(&plugin_config.source, &paths.config_dir)?;
            if !git_available {
                return Ok(DoctorCheck::skip(
                    check_name,
                    format!(
                        "skipped repository integrity checks because git is unavailable: {}",
                        install_dir.display()
                    ),
                ));
            }

            match sync::validate_managed_checkout(&install_dir, &clone_source) {
                Ok(()) => {}
                Err(error) => return Ok(DoctorCheck::fail(check_name, error.to_string())),
            }

            match sync::git_is_dirty(&install_dir) {
                Ok(true) => Ok(DoctorCheck::fail(
                    check_name,
                    format!(
                        "plugin checkout has uncommitted tracked changes: {}",
                        install_dir.display()
                    ),
                )),
                Ok(false) => Ok(DoctorCheck::pass(
                    check_name,
                    format!(
                        "plugin checkout is present and clean: {}",
                        install_dir.display()
                    ),
                )),
                Err(error) => Ok(DoctorCheck::fail_with_detail(
                    check_name,
                    format!(
                        "failed to inspect plugin checkout at {}",
                        install_dir.display()
                    ),
                    error.to_string(),
                )),
            }
        })
        .collect()
}

fn has_git_metadata(path: &Path) -> bool {
    let git_dir = path.join(".git");
    git_dir.is_dir() || git_dir.is_file()
}

fn print_human(report: &DoctorReport) {
    let width = report
        .checks
        .iter()
        .map(|check| check.name.len())
        .max()
        .unwrap_or(0);

    for check in &report.checks {
        println!(
            "{status:<4} {name:<width$}  {summary}",
            status = check.status.label(),
            name = check.name,
            summary = check.summary,
        );
        if let Some(detail) = &check.detail {
            println!("     {detail}");
        }
    }

    if report.ok {
        println!("Doctor completed without failing checks");
    } else {
        println!("Doctor found {} failing check(s)", report.failing_checks);
    }

    if !report.paths.config_exists {
        print_missing_config_guide(&report.paths.config_file);
    }
}

fn print_missing_config_guide(config_file: &Path) {
    println!();
    println!("Getting started:");
    println!("  Expected config path: {}", config_file.display());
    println!("  Existing shell TPM setup: run `tpm migrate`");
    println!("  Different tmux.conf path: run `tpm migrate --tmux-conf PATH`");
    println!("  New setup: run `tpm add tmux-plugins/tmux-sensible`");
    println!("  Then add `run-shell \"tpm load\"` to the end of `tmux.conf`");
    println!("  Finally run `tpm install`");
}

impl DoctorCheck {
    fn pass(name: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: DoctorStatus::Pass,
            summary: summary.into(),
            detail: None,
        }
    }

    fn fail(name: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: DoctorStatus::Fail,
            summary: summary.into(),
            detail: None,
        }
    }

    fn fail_with_detail(
        name: impl Into<String>,
        summary: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            status: DoctorStatus::Fail,
            summary: summary.into(),
            detail: Some(detail.into()),
        }
    }

    fn skip(name: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: DoctorStatus::Skip,
            summary: summary.into(),
            detail: None,
        }
    }
}

impl DoctorStatus {
    const fn label(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::Skip => "SKIP",
        }
    }
}

impl NumericVersion {
    fn parse(input: &str) -> Option<Self> {
        let start = input.find(|character: char| character.is_ascii_digit())?;
        let parts = input[start..]
            .split(|character: char| !character.is_ascii_digit())
            .filter(|segment| !segment.is_empty())
            .map(str::parse)
            .collect::<std::result::Result<Vec<u64>, _>>()
            .ok()?;

        if parts.is_empty() {
            None
        } else {
            Some(Self(parts))
        }
    }
}

impl PartialOrd for NumericVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for NumericVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        let max_len = self.0.len().max(other.0.len());

        for index in 0..max_len {
            let left = self.0.get(index).copied().unwrap_or(0);
            let right = other.0.get(index).copied().unwrap_or(0);

            match left.cmp(&right) {
                Ordering::Equal => continue,
                ordering => return ordering,
            }
        }

        Ordering::Equal
    }
}

#[cfg(test)]
mod tests {
    use super::NumericVersion;

    #[test]
    fn parses_tmux_style_versions() {
        let version = NumericVersion::parse("tmux 3.6a").expect("version should parse");
        assert_eq!(version, NumericVersion(vec![3, 6]));
    }

    #[test]
    fn parses_git_style_versions() {
        let version = NumericVersion::parse("git version 2.53.0").expect("version should parse");
        assert_eq!(version, NumericVersion(vec![2, 53, 0]));
    }

    #[test]
    fn compares_versions_component_wise() {
        let left = NumericVersion::parse("3.10").expect("left should parse");
        let right = NumericVersion::parse("3.2").expect("right should parse");
        assert!(left > right);
    }
}

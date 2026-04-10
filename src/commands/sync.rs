use std::{
    collections::BTreeSet,
    ffi::{OsStr, OsString},
    fmt, fs,
    path::{Path, PathBuf},
    process::Command,
};

use crate::{
    commands::resolved_paths,
    config::{Config, PluginConfig},
    error::{AppError, Result},
    paths::{ResolvedPaths, normalize_lexically},
    plugin,
    user_path::display_user_path,
};

const GIT_REMOTE: &str = "origin";

#[derive(Debug, Clone)]
pub(crate) struct SyncContext {
    pub paths: ResolvedPaths,
    pub plugins: Vec<SyncPlugin>,
}

#[derive(Debug, Clone)]
pub(crate) struct SyncPlugin {
    pub source: String,
    pub branch: Option<String>,
    pub reference: Option<String>,
    pub enabled: bool,
    pub install_name: String,
    pub install_dir: PathBuf,
    pub clone_source: String,
}

#[derive(Debug, Clone)]
pub(crate) struct GitOutput {
    pub stdout: String,
}

#[derive(Debug, Clone)]
pub(crate) struct GitCommandError {
    command: String,
    detail: String,
}

impl fmt::Display for GitCommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.command, self.detail)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CheckoutValidationError {
    detail: String,
}

impl fmt::Display for CheckoutValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.detail)
    }
}

pub(crate) fn install_context(
    config_override: Option<&Path>,
    plugins_override: Option<&Path>,
) -> Result<SyncContext> {
    let paths = resolved_paths(config_override, plugins_override)?;
    let plugins = load_config(&paths)?
        .plugins
        .iter()
        .filter(|plugin| plugin.enabled)
        .map(|plugin| configured_plugin(&paths, plugin))
        .collect::<Result<Vec<_>>>()?;

    Ok(SyncContext { paths, plugins })
}

pub(crate) fn update_context(
    config_override: Option<&Path>,
    plugins_override: Option<&Path>,
    selectors: &[String],
) -> Result<SyncContext> {
    let paths = resolved_paths(config_override, plugins_override)?;
    let configured = load_config(&paths)?
        .plugins
        .iter()
        .map(|plugin| configured_plugin(&paths, plugin))
        .collect::<Result<Vec<_>>>()?;

    let plugins = if selectors.is_empty() {
        configured
            .into_iter()
            .filter(|plugin| plugin.enabled)
            .collect::<Vec<_>>()
    } else {
        let mut selected = Vec::new();
        let mut seen = BTreeSet::new();

        for selector in selectors {
            let selector = selector.trim();
            let plugin = configured
                .iter()
                .find(|plugin| plugin.install_name == selector || plugin.source == selector)
                .ok_or_else(|| AppError::PluginNotConfigured {
                    name: selector.to_string(),
                })?;

            if seen.insert(plugin.install_name.clone()) {
                selected.push(plugin.clone());
            }
        }

        selected
    };

    Ok(SyncContext { paths, plugins })
}

pub(crate) fn ensure_directory(path: &Path) -> Result<()> {
    fs::create_dir_all(path).map_err(|source| AppError::CreateDirectory {
        path: path.to_path_buf(),
        source,
    })
}

pub(crate) fn git<I, S>(
    cwd: Option<&Path>,
    args: I,
) -> std::result::Result<GitOutput, GitCommandError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args = args
        .into_iter()
        .map(|arg| arg.as_ref().to_os_string())
        .collect::<Vec<OsString>>();
    let command = format_command(cwd, &args);

    let mut process = Command::new("git");
    if let Some(path) = cwd {
        process.current_dir(path);
    }
    process.args(&args);

    let output = process.output().map_err(|source| GitCommandError {
        command: command.clone(),
        detail: source.to_string(),
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if output.status.success() {
        Ok(GitOutput { stdout })
    } else {
        let detail = if stderr.is_empty() {
            if stdout.is_empty() {
                format!("exited with status {}", output.status)
            } else {
                format!("exited with status {}: {}", output.status, stdout)
            }
        } else {
            format!("exited with status {}: {}", output.status, stderr)
        };

        Err(GitCommandError { command, detail })
    }
}

pub(crate) fn git_ref_exists(path: &Path, reference: &str) -> bool {
    let args = [
        "show-ref".to_string(),
        "--verify".to_string(),
        "--quiet".to_string(),
        reference.to_string(),
    ];
    git(Some(path), &args).is_ok()
}

pub(crate) fn fetch_origin(path: &Path) -> std::result::Result<(), GitCommandError> {
    git(Some(path), ["fetch", "--prune", "--tags", GIT_REMOTE]).map(|_| ())
}

pub(crate) fn remote_branch_exists(path: &Path, branch: &str) -> bool {
    git_ref_exists(path, &remote_branch_ref(branch))
}

pub(crate) fn checkout_branch(
    path: &Path,
    branch: &str,
) -> std::result::Result<(), GitCommandError> {
    if git_ref_exists(path, &local_branch_ref(branch)) {
        let args = vec![
            "checkout".to_string(),
            "--quiet".to_string(),
            branch.to_string(),
        ];
        git(Some(path), &args).map(|_| ())
    } else {
        let args = vec![
            "checkout".to_string(),
            "--quiet".to_string(),
            "--track".to_string(),
            "-b".to_string(),
            branch.to_string(),
            remote_branch_ref(branch),
        ];
        git(Some(path), &args).map(|_| ())
    }
}

pub(crate) fn fast_forward_branch(
    path: &Path,
    branch: &str,
) -> std::result::Result<(), GitCommandError> {
    let args = vec![
        "merge".to_string(),
        "--ff-only".to_string(),
        remote_branch_ref(branch),
    ];
    git(Some(path), &args).map(|_| ())
}

pub(crate) fn update_submodules(path: &Path) -> std::result::Result<(), GitCommandError> {
    git(Some(path), ["submodule", "update", "--init", "--recursive"]).map(|_| ())
}

pub(crate) fn default_branch(path: &Path) -> std::result::Result<String, String> {
    let _ = git(Some(path), ["remote", "set-head", GIT_REMOTE, "--auto"]);
    let remote_head = format!("refs/remotes/{GIT_REMOTE}/HEAD");

    if let Ok(output) = git(
        Some(path),
        ["symbolic-ref", "--quiet", "--short", remote_head.as_str()],
    ) && let Some(branch) = output.stdout.strip_prefix("origin/")
    {
        return Ok(branch.to_string());
    }

    if let Ok(output) = git(Some(path), ["symbolic-ref", "--quiet", "--short", "HEAD"])
        && remote_branch_exists(path, &output.stdout)
    {
        return Ok(output.stdout);
    }

    Err("could not determine the remote default branch for origin".to_string())
}

pub(crate) fn checkout_pinned_reference(
    path: &Path,
    reference: &str,
) -> std::result::Result<(), String> {
    if remote_branch_exists(path, reference) {
        return Err(format!(
            "configured ref `{reference}` names a remote branch; use `branch: {reference}` instead"
        ));
    }

    if git_ref_exists(path, &format!("refs/tags/{reference}")) {
        let checkout_args = vec![
            "checkout".to_string(),
            "--quiet".to_string(),
            reference.to_string(),
        ];
        git(Some(path), &checkout_args)
            .map(|_| ())
            .map_err(|error| error.to_string())
    } else if looks_like_commit(reference) {
        let rev_parse_args = vec![
            "rev-parse".to_string(),
            "--verify".to_string(),
            format!("{reference}^{{commit}}"),
        ];
        git(Some(path), &rev_parse_args).map_err(|error| error.to_string())?;

        let checkout_args = vec![
            "checkout".to_string(),
            "--quiet".to_string(),
            reference.to_string(),
        ];
        git(Some(path), &checkout_args)
            .map(|_| ())
            .map_err(|error| error.to_string())
    } else {
        Err(format!(
            "configured ref `{reference}` is not available as a tag or commit"
        ))
    }
}

pub(crate) fn git_head_commit(path: &Path) -> std::result::Result<String, GitCommandError> {
    git(Some(path), ["rev-parse", "HEAD"]).map(|output| output.stdout)
}

pub(crate) fn git_is_work_tree(path: &Path) -> std::result::Result<bool, GitCommandError> {
    git(Some(path), ["rev-parse", "--is-inside-work-tree"]).map(|output| output.stdout == "true")
}

pub(crate) fn git_is_dirty(path: &Path) -> std::result::Result<bool, GitCommandError> {
    git(
        Some(path),
        ["status", "--porcelain", "--untracked-files=no"],
    )
    .map(|output| !output.stdout.is_empty())
}

pub(crate) fn validate_managed_checkout(
    path: &Path,
    expected_clone_source: &str,
) -> std::result::Result<(), CheckoutValidationError> {
    if !path.exists() {
        return Err(CheckoutValidationError {
            detail: format!("plugin checkout is missing at {}", display_user_path(path)),
        });
    }

    if !path.is_dir() {
        return Err(CheckoutValidationError {
            detail: format!(
                "expected plugin checkout directory at {}",
                display_user_path(path)
            ),
        });
    }

    match git_is_work_tree(path) {
        Ok(true) => {}
        Ok(false) | Err(_) => {
            return Err(CheckoutValidationError {
                detail: format!(
                    "plugin checkout is not a valid git work tree: {}",
                    display_user_path(path)
                ),
            });
        }
    }

    let actual_clone_source = git(Some(path), ["remote", "get-url", GIT_REMOTE])
        .map(|output| output.stdout)
        .map_err(|_| CheckoutValidationError {
            detail: format!(
                "plugin checkout is missing the origin remote expected for managed plugins: {}",
                display_user_path(path)
            ),
        })?;

    if !same_clone_source(expected_clone_source, &actual_clone_source, path) {
        return Err(CheckoutValidationError {
            detail: format!(
                "plugin checkout source does not match configured source at {} (expected {}, found {})",
                display_user_path(path),
                display_local_source(expected_clone_source),
                display_local_source(&actual_clone_source)
            ),
        });
    }

    Ok(())
}

pub(crate) fn looks_like_commit(reference: &str) -> bool {
    (7..=40).contains(&reference.len())
        && reference
            .chars()
            .all(|character| character.is_ascii_hexdigit())
}

pub(crate) fn remote_branch_ref(branch: &str) -> String {
    format!("refs/remotes/{GIT_REMOTE}/{branch}")
}

pub(crate) fn local_branch_ref(branch: &str) -> String {
    format!("refs/heads/{branch}")
}

fn load_config(paths: &ResolvedPaths) -> Result<Config> {
    Config::load_if_exists(&paths.config_file)?.ok_or_else(|| AppError::ConfigNotFound {
        path: paths.config_file.clone(),
    })
}

pub(crate) fn configured_plugin(
    paths: &ResolvedPaths,
    plugin_config: &PluginConfig,
) -> Result<SyncPlugin> {
    sync_plugin(
        plugin_config.source.clone(),
        plugin_config.branch.clone(),
        plugin_config.reference.clone(),
        plugin_config.enabled,
        &paths.plugins_dir,
        &paths.config_dir,
    )
}

fn sync_plugin(
    source: String,
    branch: Option<String>,
    reference: Option<String>,
    enabled: bool,
    plugins_dir: &Path,
    source_base_dir: &Path,
) -> Result<SyncPlugin> {
    let install_name = plugin::install_name(&source)?;
    let install_dir = plugin::install_dir(plugins_dir, &source)?;
    let clone_source = resolve_clone_source(&source, source_base_dir)?;

    Ok(SyncPlugin {
        source,
        branch,
        reference,
        enabled,
        install_name,
        install_dir,
        clone_source,
    })
}

pub(crate) fn resolve_clone_source(source: &str, base_dir: &Path) -> Result<String> {
    let source = source.trim();
    let github_source = source.trim_end_matches('/');

    match plugin::classify_source(source)? {
        plugin::SourceKind::Remote => Ok(source.to_string()),
        plugin::SourceKind::GitHubShorthand => Ok(format!("https://github.com/{github_source}")),
        plugin::SourceKind::LocalPath => Ok(resolve_local_path(source, base_dir)?
            .to_string_lossy()
            .into_owned()),
    }
}

fn resolve_local_path(source: &str, base_dir: &Path) -> Result<PathBuf> {
    let path = if source == "~" || source.starts_with("~/") {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or(AppError::HomeDirectoryMissing)?;
        if source == "~" {
            home
        } else {
            home.join(source.trim_start_matches("~/"))
        }
    } else {
        let path = PathBuf::from(source);
        if path.is_absolute() {
            path
        } else {
            base_dir.join(path)
        }
    };

    Ok(normalize_lexically(&path))
}

fn format_command(cwd: Option<&Path>, args: &[OsString]) -> String {
    let rendered = args.iter().map(shell_escape).collect::<Vec<_>>().join(" ");

    match cwd {
        Some(path) if rendered.is_empty() => format!("git -C {}", display_user_path(path)),
        Some(path) => format!("git -C {} {}", display_user_path(path), rendered),
        None if rendered.is_empty() => "git".to_string(),
        None => format!("git {rendered}"),
    }
}

fn shell_escape(value: &OsString) -> String {
    let value_path = Path::new(value);
    let value = if value_path.is_absolute() {
        display_user_path(value_path).into()
    } else {
        value.to_string_lossy()
    };
    if value.chars().all(|character| {
        character.is_ascii_alphanumeric()
            || matches!(character, '/' | '-' | '_' | '.' | ':' | '=' | '~')
    }) {
        value.into_owned()
    } else {
        format!("{value:?}")
    }
}

fn display_local_source(source: &str) -> String {
    let path = Path::new(source);
    if path.is_absolute() {
        display_user_path(path)
    } else {
        source.to_string()
    }
}

fn same_clone_source(expected: &str, actual: &str, checkout_dir: &Path) -> bool {
    normalize_source(expected, None) == normalize_source(actual, Some(checkout_dir))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum NormalizedSource {
    Local(PathBuf),
    Remote(String),
}

fn normalize_source(source: &str, local_base_dir: Option<&Path>) -> NormalizedSource {
    let source = source.trim();

    if let Some(path) = source.strip_prefix("file://") {
        return NormalizedSource::Local(normalize_lexically(&PathBuf::from(path)));
    }

    if let Some(remote) = normalize_remote_source(source) {
        return NormalizedSource::Remote(remote);
    }

    let candidate = PathBuf::from(source);
    let resolved = if candidate.is_absolute() {
        candidate
    } else if let Some(base_dir) = local_base_dir {
        base_dir.join(candidate)
    } else {
        candidate
    };

    NormalizedSource::Local(normalize_lexically(&resolved))
}

fn normalize_remote_source(source: &str) -> Option<String> {
    if let Some((authority, path)) = source
        .strip_prefix("git@")
        .and_then(|value| value.split_once(':'))
    {
        return Some(format!(
            "{}/{}",
            authority.to_ascii_lowercase(),
            normalize_remote_path(path),
        ));
    }

    let (_, remainder) = source.split_once("://")?;
    let remainder = remainder.trim_start_matches('/');
    let (authority, path) = remainder.split_once('/').unwrap_or((remainder, ""));
    let authority = authority
        .rsplit('@')
        .next()
        .unwrap_or(authority)
        .to_ascii_lowercase();

    Some(format!("{}/{}", authority, normalize_remote_path(path),))
}

fn normalize_remote_path(path: &str) -> String {
    path.trim_matches('/').trim_end_matches(".git").to_string()
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{NormalizedSource, normalize_source, resolve_clone_source, same_clone_source};

    #[test]
    fn treats_github_https_and_ssh_sources_as_equivalent() {
        assert!(same_clone_source(
            "https://github.com/tmux-plugins/tmux-sensible",
            "git@github.com:tmux-plugins/tmux-sensible.git",
            Path::new("/tmp/plugin"),
        ));
    }

    #[test]
    fn normalizes_local_paths_lexically() {
        assert_eq!(
            normalize_source("/tmp/source/./repo.git", None),
            NormalizedSource::Local(PathBuf::from("/tmp/source/repo.git")),
        );
    }

    #[test]
    fn resolves_relative_actual_local_paths_against_the_checkout_directory() {
        assert_eq!(
            normalize_source(
                "../remotes/tmux-open.git",
                Some(Path::new("/tmp/plugins/tmux-open"))
            ),
            NormalizedSource::Local(PathBuf::from("/tmp/plugins/remotes/tmux-open.git")),
        );
    }

    #[test]
    fn prefers_github_shorthand_even_when_a_matching_local_path_exists() {
        let root = unique_temp_dir("github-shorthand");
        let base_dir = root.join("config");
        let local_path = base_dir.join("tmux-plugins").join("tmux-sensible");
        fs::create_dir_all(&local_path).expect("local path should exist");

        let clone_source = resolve_clone_source("tmux-plugins/tmux-sensible", &base_dir)
            .expect("clone source should resolve");

        assert_eq!(
            clone_source,
            "https://github.com/tmux-plugins/tmux-sensible"
        );
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be monotonic")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "tpm-rs-sync-test-{name}-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).expect("temp directory should exist");
        directory
    }
}

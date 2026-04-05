use std::{
    env, fs,
    path::{Path, PathBuf},
};

use crate::{
    config::Config,
    error::{AppError, Result},
    paths::normalize_lexically,
    plugin,
};

use super::{config_file_path, current_dir};

#[derive(Debug, Default)]
struct ParsedTmuxConfig {
    plugins: Vec<MigratedPlugin>,
    skipped_source_files: Vec<SkippedSourceFile>,
}

#[derive(Debug)]
struct MigratedPlugin {
    source: String,
    branch: Option<String>,
    reference: Option<String>,
}

#[derive(Debug)]
struct SkippedSourceFile {
    line_number: usize,
    resolved_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenizeState {
    Unquoted,
    SingleQuoted,
    DoubleQuoted,
}

pub fn run(config_override: Option<&Path>, tmux_conf_override: Option<&Path>) -> Result<()> {
    let config_path = config_file_path(config_override)?;
    if config_path.exists() {
        return Err(AppError::Migration {
            message: format!(
                "config already exists at `{}`; refusing to overwrite",
                config_path.display()
            ),
        });
    }

    let tmux_conf_path = resolve_tmux_conf_path(tmux_conf_override)?;
    let parsed = parse_tmux_config(&tmux_conf_path)?;
    if parsed.plugins.is_empty() {
        return Err(no_plugins_detected_error(&tmux_conf_path, &parsed));
    }

    let mut config = Config::new();
    for plugin in parsed.plugins {
        config.add_plugin(
            &plugin.source,
            plugin.branch.as_deref(),
            plugin.reference.as_deref(),
        )?;
    }
    config.save(&config_path)?;

    println!(
        "Detected and parsed {} plugin(s) correctly",
        config.plugins.len()
    );
    println!(
        "Skipped {} source-file directive(s); multi-file tmux configs are not supported",
        parsed.skipped_source_files.len()
    );
    for skipped in &parsed.skipped_source_files {
        println!("{}", skipped.render());
    }
    println!("Wrote tpm.yaml to {}", config_path.display());
    println!("Did not modify {}", tmux_conf_path.display());
    println!(
        "You may still need to replace the legacy TPM bootstrap with `run-shell \"tpm load\"` at the end of the file"
    );

    Ok(())
}

fn no_plugins_detected_error(tmux_conf_path: &Path, parsed: &ParsedTmuxConfig) -> AppError {
    if parsed.skipped_source_files.is_empty() {
        return AppError::Migration {
            message: format!(
                "no migratable tmux plugins were detected in `{}`",
                tmux_conf_path.display()
            ),
        };
    }

    let mut message = format!(
        "no migratable tmux plugins were detected in `{}`\n",
        tmux_conf_path.display()
    );
    message.push_str(&format!(
        "Skipped {} source-file directive(s); multi-file tmux configs are not supported\n",
        parsed.skipped_source_files.len()
    ));
    for skipped in &parsed.skipped_source_files {
        message.push_str(&skipped.render());
        message.push('\n');
    }
    message.push_str(
        "Run `tpm migrate --tmux-conf PATH` against a tmux config file that directly contains `@plugin` lines, or merge sourced plugin declarations into one file first",
    );

    AppError::Migration { message }
}

fn resolve_tmux_conf_path(tmux_conf_override: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = tmux_conf_override {
        let cwd = current_dir()?;
        return Ok(resolve_path(path, &cwd));
    }

    let home_dir = env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or(AppError::HomeDirectoryMissing)?;
    let cwd = current_dir()?;
    let xdg_config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .map(|path| resolve_path(&path, &cwd))
        .unwrap_or_else(|| normalize_lexically(&home_dir.join(".config")));

    let candidates = [
        normalize_lexically(&home_dir.join(".tmux.conf")),
        normalize_lexically(&home_dir.join(".tmux")),
        normalize_lexically(&home_dir.join(".tmux").join("tmux.conf")),
        normalize_lexically(&xdg_config_home.join("tmux").join("tmux.conf")),
    ];

    candidates
        .iter()
        .find(|path| path.is_file())
        .cloned()
        .ok_or_else(|| AppError::Migration {
            message: format!(
                "no tmux config file found; checked: {}",
                candidates
                    .iter()
                    .map(|path| format!("`{}`", path.display()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        })
}

fn resolve_path(path: &Path, cwd: &Path) -> PathBuf {
    let path = path.to_string_lossy();
    if path == "~" {
        return env::var_os("HOME")
            .map(PathBuf::from)
            .map(|path| normalize_lexically(&path))
            .unwrap_or_else(|| normalize_lexically(Path::new("~")));
    }

    if let Some(stripped) = path.strip_prefix("~/")
        && let Some(home_dir) = env::var_os("HOME").map(PathBuf::from)
    {
        return normalize_lexically(&home_dir.join(stripped));
    }

    let path = PathBuf::from(path.as_ref());
    if path.is_absolute() {
        normalize_lexically(&path)
    } else {
        normalize_lexically(&cwd.join(path))
    }
}

fn parse_tmux_config(path: &Path) -> Result<ParsedTmuxConfig> {
    let raw = fs::read_to_string(path).map_err(|source| AppError::ReadTmuxConfig {
        path: normalize_lexically(path),
        source,
    })?;
    let base_dir = path
        .parent()
        .map(normalize_lexically)
        .unwrap_or_else(|| normalize_lexically(Path::new(".")));

    let mut parsed = ParsedTmuxConfig::default();
    for (index, line) in raw.lines().enumerate() {
        let tokens = tokenize_tmux_line(line).map_err(|message| AppError::Migration {
            message: format!(
                "tmux config `{}` line {}: {message}",
                path.display(),
                index + 1
            ),
        })?;
        if tokens.is_empty() {
            continue;
        }

        if is_source_file_command(&tokens) {
            parsed
                .skipped_source_files
                .push(skipped_source_file(&tokens, &base_dir, index + 1));
            continue;
        }

        let Some(source) = plugin_source_from_tokens(&tokens) else {
            continue;
        };

        let Some(plugin) = migrate_plugin_source(source, &base_dir)? else {
            continue;
        };
        parsed.plugins.push(plugin);
    }

    Ok(parsed)
}

fn tokenize_tmux_line(line: &str) -> std::result::Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars().peekable();
    let mut state = TokenizeState::Unquoted;

    while let Some(character) = chars.next() {
        match state {
            TokenizeState::Unquoted => match character {
                '#' if current.is_empty() => break,
                '\'' => state = TokenizeState::SingleQuoted,
                '"' => state = TokenizeState::DoubleQuoted,
                '\\' => {
                    if let Some(next) = chars.next() {
                        current.push(next);
                    }
                }
                character if character.is_whitespace() => {
                    if !current.is_empty() {
                        tokens.push(std::mem::take(&mut current));
                    }
                }
                character => current.push(character),
            },
            TokenizeState::SingleQuoted => {
                if character == '\'' {
                    state = TokenizeState::Unquoted;
                } else {
                    current.push(character);
                }
            }
            TokenizeState::DoubleQuoted => match character {
                '"' => state = TokenizeState::Unquoted,
                '\\' => {
                    if let Some(next) = chars.next() {
                        current.push(next);
                    }
                }
                character => current.push(character),
            },
        }
    }

    if state != TokenizeState::Unquoted {
        return Err("unterminated quoted string".to_string());
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    Ok(tokens)
}

fn is_source_file_command(tokens: &[String]) -> bool {
    matches!(
        tokens.first().map(String::as_str),
        Some("source-file") | Some("source")
    )
}

fn skipped_source_file(
    tokens: &[String],
    base_dir: &Path,
    line_number: usize,
) -> SkippedSourceFile {
    let resolved_path =
        source_file_target(tokens).map(|target| resolve_path(Path::new(target), base_dir));

    SkippedSourceFile {
        line_number,
        resolved_path,
    }
}

fn source_file_target(tokens: &[String]) -> Option<&str> {
    if !is_source_file_command(tokens) {
        return None;
    }

    let mut position = 1;
    while position < tokens.len() && tokens[position].starts_with('-') {
        position += 1;
    }

    tokens.get(position).map(String::as_str)
}

fn plugin_source_from_tokens(tokens: &[String]) -> Option<&str> {
    match tokens.first().map(String::as_str) {
        Some("set") | Some("set-option") => {}
        _ => return None,
    }

    let mut position = 1;
    while position < tokens.len() && tokens[position].starts_with('-') {
        position += 1;
    }

    if position >= tokens.len() || tokens[position] != "@plugin" {
        return None;
    }

    tokens.get(position + 1).map(String::as_str)
}

impl SkippedSourceFile {
    fn render(&self) -> String {
        match &self.resolved_path {
            Some(path) => format!(
                "Skipped source-file on line {}: {}",
                self.line_number,
                path.display()
            ),
            None => format!(
                "Skipped source-file on line {}: could not determine the sourced file path",
                self.line_number
            ),
        }
    }
}

fn migrate_plugin_source(source: &str, base_dir: &Path) -> Result<Option<MigratedPlugin>> {
    let source = source.trim();
    if source.is_empty() {
        return Err(AppError::Migration {
            message: "encountered an empty `@plugin` value during migration".to_string(),
        });
    }

    let (source, branch, reference) = split_legacy_source_fragment(source);
    let normalized_source = normalize_plugin_source(&source, base_dir)?;
    let install_name = plugin::install_name(&normalized_source)?;
    if install_name == "tmux-plugins/tpm" {
        return Ok(None);
    }

    Ok(Some(MigratedPlugin {
        source: normalized_source,
        branch,
        reference,
    }))
}

fn split_legacy_source_fragment(source: &str) -> (String, Option<String>, Option<String>) {
    let Some((source, fragment)) = source.rsplit_once('#') else {
        return (source.to_string(), None, None);
    };
    let fragment = fragment.trim();
    if fragment.is_empty() {
        return (source.to_string(), None, None);
    }

    if looks_like_pinned_reference(fragment) {
        (source.to_string(), None, Some(fragment.to_string()))
    } else {
        (source.to_string(), Some(fragment.to_string()), None)
    }
}

fn looks_like_pinned_reference(fragment: &str) -> bool {
    (fragment.starts_with('v') && fragment.len() > 1)
        || (7..=40).contains(&fragment.len())
            && fragment
                .chars()
                .all(|character| character.is_ascii_hexdigit())
}

fn normalize_plugin_source(source: &str, base_dir: &Path) -> Result<String> {
    match plugin::classify_source(source)? {
        plugin::SourceKind::LocalPath => absolutize_local_source(source, base_dir),
        plugin::SourceKind::Remote | plugin::SourceKind::GitHubShorthand => Ok(source.to_string()),
    }
}

fn absolutize_local_source(source: &str, base_dir: &Path) -> Result<String> {
    let absolute = if source == "~" || source.starts_with("~/") {
        let home_dir = env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or(AppError::HomeDirectoryMissing)?;
        if source == "~" {
            home_dir
        } else {
            home_dir.join(source.trim_start_matches("~/"))
        }
    } else {
        let path = PathBuf::from(source);
        if path.is_absolute() {
            path
        } else {
            base_dir.join(path)
        }
    };

    Ok(normalize_lexically(&absolute)
        .to_string_lossy()
        .into_owned())
}

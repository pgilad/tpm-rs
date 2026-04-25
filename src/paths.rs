use std::{
    ffi::OsString,
    path::{Component, Path, PathBuf},
};

use serde::Serialize;

use crate::{
    config::Config,
    error::{AppError, Result},
};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ResolvedPaths {
    pub config_file: PathBuf,
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub state_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub plugins_dir: PathBuf,
    pub config_exists: bool,
}

#[derive(Debug, Clone)]
pub struct ResolveOptions<'a> {
    pub cwd: &'a Path,
    pub config_override: Option<&'a Path>,
    pub plugins_override: Option<&'a Path>,
}

#[derive(Debug, Clone)]
struct BasePaths {
    home_dir: Option<PathBuf>,
    config_file: PathBuf,
    config_dir: PathBuf,
    data_dir: PathBuf,
    state_dir: PathBuf,
    cache_dir: PathBuf,
}

pub fn resolve(options: ResolveOptions<'_>) -> Result<ResolvedPaths> {
    let env = ProcessEnv;
    resolve_with_env(&env, options)
}

pub fn resolve_config_file(options: ResolveOptions<'_>) -> Result<PathBuf> {
    let env = ProcessEnv;
    resolve_config_file_with_env(&env, options)
}

pub fn resolve_base(options: ResolveOptions<'_>) -> Result<ResolvedPaths> {
    let env = ProcessEnv;
    resolve_base_with_env(&env, options)
}

fn resolve_with_env(env: &impl EnvProvider, options: ResolveOptions<'_>) -> Result<ResolvedPaths> {
    let base = BasePaths::resolve(env, options.cwd, options.config_override)?;
    let config = Config::load_if_exists(&base.config_file)?;
    let plugins_dir = resolve_plugins_dir(
        env,
        options.cwd,
        base.home_dir.as_deref(),
        options.plugins_override,
        config.as_ref(),
        &base.config_dir,
        &base.data_dir,
    )?;

    Ok(ResolvedPaths {
        config_exists: config.is_some(),
        config_file: base.config_file,
        config_dir: base.config_dir,
        data_dir: base.data_dir,
        state_dir: base.state_dir,
        cache_dir: base.cache_dir,
        plugins_dir,
    })
}

fn resolve_base_with_env(
    env: &impl EnvProvider,
    options: ResolveOptions<'_>,
) -> Result<ResolvedPaths> {
    let base = BasePaths::resolve(env, options.cwd, options.config_override)?;
    let plugins_dir = resolve_plugins_dir(
        env,
        options.cwd,
        base.home_dir.as_deref(),
        options.plugins_override,
        None,
        &base.config_dir,
        &base.data_dir,
    )?;

    Ok(ResolvedPaths {
        config_exists: base.config_file.exists(),
        config_file: base.config_file,
        config_dir: base.config_dir,
        data_dir: base.data_dir,
        state_dir: base.state_dir,
        cache_dir: base.cache_dir,
        plugins_dir,
    })
}

fn resolve_config_file_with_env(
    env: &impl EnvProvider,
    options: ResolveOptions<'_>,
) -> Result<PathBuf> {
    let home_dir = maybe_home_dir(env);
    resolve_config_file_path(
        env,
        options.cwd,
        options.config_override,
        home_dir.as_deref(),
    )
}

impl BasePaths {
    fn resolve(env: &impl EnvProvider, cwd: &Path, config_override: Option<&Path>) -> Result<Self> {
        let home_dir = maybe_home_dir(env);
        let config_file = resolve_config_file_path(env, cwd, config_override, home_dir.as_deref())?;

        let config_file = normalize_lexically(&config_file);
        let config_dir = config_file
            .parent()
            .map(normalize_lexically)
            .unwrap_or_else(|| normalize_lexically(Path::new(".")));

        let data_dir = match resolve_app_dir(env, cwd, home_dir.as_deref(), "TPM_DATA_DIR")? {
            Some(path) => path,
            None => resolve_xdg_home(
                env,
                cwd,
                home_dir.as_deref(),
                "XDG_DATA_HOME",
                ".local/share",
            )?
            .join("tpm"),
        };
        let state_dir = match resolve_app_dir(env, cwd, home_dir.as_deref(), "TPM_STATE_DIR")? {
            Some(path) => path,
            None => resolve_xdg_home(
                env,
                cwd,
                home_dir.as_deref(),
                "XDG_STATE_HOME",
                ".local/state",
            )?
            .join("tpm"),
        };
        let cache_dir = match resolve_app_dir(env, cwd, home_dir.as_deref(), "TPM_CACHE_DIR")? {
            Some(path) => path,
            None => resolve_xdg_home(env, cwd, home_dir.as_deref(), "XDG_CACHE_HOME", ".cache")?
                .join("tpm"),
        };

        Ok(Self {
            home_dir,
            config_file,
            config_dir,
            data_dir: normalize_lexically(&data_dir),
            state_dir: normalize_lexically(&state_dir),
            cache_dir: normalize_lexically(&cache_dir),
        })
    }
}

fn resolve_plugins_dir(
    env: &impl EnvProvider,
    cwd: &Path,
    home_dir: Option<&Path>,
    plugins_override: Option<&Path>,
    config: Option<&Config>,
    config_dir: &Path,
    data_dir: &Path,
) -> Result<PathBuf> {
    let path = if let Some(path) = plugins_override {
        expand_path(path.to_string_lossy().as_ref(), env, cwd, home_dir, None)?
    } else if let Some(value) = env.var_os("TPM_PLUGINS_DIR") {
        expand_path(value.to_string_lossy().as_ref(), env, cwd, home_dir, None)?
    } else if let Some(value) = config.and_then(|config| config.paths.plugins.as_deref()) {
        expand_path(value, env, cwd, home_dir, Some(config_dir))?
    } else {
        data_dir.join("plugins")
    };

    Ok(normalize_lexically(&path))
}

fn resolve_directory_env(
    env: &impl EnvProvider,
    cwd: &Path,
    home_dir: Option<&Path>,
    key: &str,
) -> Result<Option<PathBuf>> {
    env.var_os(key)
        .map(|value| expand_path(value.to_string_lossy().as_ref(), env, cwd, home_dir, None))
        .transpose()
}

fn resolve_app_dir(
    env: &impl EnvProvider,
    cwd: &Path,
    home_dir: Option<&Path>,
    key: &str,
) -> Result<Option<PathBuf>> {
    resolve_directory_env(env, cwd, home_dir, key)
}

fn maybe_home_dir(env: &impl EnvProvider) -> Option<PathBuf> {
    env.var_os("HOME")
        .map(PathBuf::from)
        .map(|path| normalize_lexically(&path))
}

fn resolve_config_file_path(
    env: &impl EnvProvider,
    cwd: &Path,
    config_override: Option<&Path>,
    home_dir: Option<&Path>,
) -> Result<PathBuf> {
    let config_file = if let Some(path) = config_override {
        expand_path(path.to_string_lossy().as_ref(), env, cwd, home_dir, None)?
    } else if let Some(value) = env.var_os("TPM_CONFIG_FILE") {
        expand_path(value.to_string_lossy().as_ref(), env, cwd, home_dir, None)?
    } else if let Some(value) = env.var_os("TPM_CONFIG_DIR") {
        expand_path(value.to_string_lossy().as_ref(), env, cwd, home_dir, None)?.join("tpm.yaml")
    } else {
        resolve_xdg_home(env, cwd, home_dir, "XDG_CONFIG_HOME", ".config")?
            .join("tpm")
            .join("tpm.yaml")
    };

    Ok(normalize_lexically(&config_file))
}

fn resolve_xdg_home(
    env: &impl EnvProvider,
    cwd: &Path,
    home_dir: Option<&Path>,
    env_key: &str,
    home_fallback: &str,
) -> Result<PathBuf> {
    if let Some(path) = resolve_directory_env(env, cwd, home_dir, env_key)? {
        Ok(path)
    } else {
        default_home_subdir(home_dir, home_fallback)
    }
}

fn default_home_subdir(home_dir: Option<&Path>, suffix: &str) -> Result<PathBuf> {
    let home_dir = home_dir.ok_or(AppError::HomeDirectoryMissing)?;
    Ok(normalize_lexically(&home_dir.join(suffix)))
}

fn expand_path(
    value: &str,
    env: &impl EnvProvider,
    cwd: &Path,
    home_dir: Option<&Path>,
    relative_to: Option<&Path>,
) -> Result<PathBuf> {
    let expanded = expand_variables(value, env)?;
    let expanded = if expanded == "~" {
        home_dir
            .ok_or(AppError::HomeDirectoryMissing)?
            .to_string_lossy()
            .into_owned()
    } else if let Some(stripped) = expanded.strip_prefix("~/") {
        let home_dir = home_dir.ok_or(AppError::HomeDirectoryMissing)?;
        format!("{}/{}", home_dir.to_string_lossy(), stripped)
    } else {
        expanded
    };

    let candidate = PathBuf::from(expanded);
    let absolute = if candidate.is_absolute() {
        candidate
    } else if let Some(base) = relative_to {
        base.join(candidate)
    } else {
        cwd.join(candidate)
    };

    Ok(normalize_lexically(&absolute))
}

fn expand_variables(value: &str, env: &impl EnvProvider) -> Result<String> {
    let chars: Vec<char> = value.chars().collect();
    let mut index = 0;
    let mut output = String::with_capacity(value.len());

    while index < chars.len() {
        if chars[index] != '$' {
            output.push(chars[index]);
            index += 1;
            continue;
        }

        index += 1;
        if index >= chars.len() {
            return Err(AppError::InvalidEnvironmentSyntax {
                value: value.to_string(),
            });
        }

        if chars[index] == '{' {
            index += 1;
            let start = index;
            while index < chars.len() && chars[index] != '}' {
                index += 1;
            }

            if index >= chars.len() {
                return Err(AppError::InvalidEnvironmentSyntax {
                    value: value.to_string(),
                });
            }

            let key: String = chars[start..index].iter().collect();
            if key.is_empty() {
                return Err(AppError::InvalidEnvironmentSyntax {
                    value: value.to_string(),
                });
            }

            let replacement =
                env.var_os(&key)
                    .ok_or_else(|| AppError::MissingEnvironmentVariable {
                        key: key.clone(),
                        value: value.to_string(),
                    })?;
            output.push_str(&replacement.to_string_lossy());
            index += 1;
            continue;
        }

        let start = index;
        while index < chars.len() && is_env_name_character(chars[index]) {
            index += 1;
        }

        if start == index {
            return Err(AppError::InvalidEnvironmentSyntax {
                value: value.to_string(),
            });
        }

        let key: String = chars[start..index].iter().collect();
        let replacement = env
            .var_os(&key)
            .ok_or_else(|| AppError::MissingEnvironmentVariable {
                key: key.clone(),
                value: value.to_string(),
            })?;
        output.push_str(&replacement.to_string_lossy());
    }

    Ok(output)
}

const fn is_env_name_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

pub fn normalize_lexically(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    let absolute = path.is_absolute();

    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(Path::new("/")),
            Component::CurDir => {}
            Component::ParentDir => match normalized.components().next_back() {
                Some(Component::Normal(_)) => {
                    normalized.pop();
                }
                Some(Component::ParentDir) | None if !absolute => normalized.push(".."),
                _ => {}
            },
            Component::Normal(part) => normalized.push(part),
        }
    }

    if normalized.as_os_str().is_empty() {
        if absolute {
            PathBuf::from("/")
        } else {
            PathBuf::from(".")
        }
    } else {
        normalized
    }
}

trait EnvProvider {
    fn var_os(&self, key: &str) -> Option<OsString>;
}

struct ProcessEnv;

impl EnvProvider for ProcessEnv {
    fn var_os(&self, key: &str) -> Option<OsString> {
        std::env::var_os(key)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        ffi::OsString,
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{
        EnvProvider, ResolveOptions, normalize_lexically, resolve_base_with_env, resolve_with_env,
    };

    #[test]
    fn resolves_default_xdg_paths_without_config() {
        let env = FakeEnv::new([("HOME", "/tmp/test-home")]);
        let resolved = resolve_base_with_env(
            &env,
            ResolveOptions {
                cwd: Path::new("/workspace"),
                config_override: None,
                plugins_override: None,
            },
        )
        .expect("paths should resolve");

        assert_eq!(
            resolved.config_file,
            PathBuf::from("/tmp/test-home/.config/tpm/tpm.yaml")
        );
        assert_eq!(
            resolved.config_dir,
            PathBuf::from("/tmp/test-home/.config/tpm")
        );
        assert_eq!(
            resolved.data_dir,
            PathBuf::from("/tmp/test-home/.local/share/tpm")
        );
        assert_eq!(
            resolved.state_dir,
            PathBuf::from("/tmp/test-home/.local/state/tpm")
        );
        assert_eq!(
            resolved.cache_dir,
            PathBuf::from("/tmp/test-home/.cache/tpm")
        );
        assert_eq!(
            resolved.plugins_dir,
            PathBuf::from("/tmp/test-home/.local/share/tpm/plugins")
        );
        assert!(!resolved.config_exists);
    }

    #[test]
    fn resolve_base_ignores_invalid_config_file_contents() {
        let root = unique_temp_dir("base-invalid-config");
        let config_dir = root.join(".config").join("tpm");
        fs::create_dir_all(&config_dir).expect("failed to create config dir");
        fs::write(config_dir.join("tpm.yaml"), "not: [valid").expect("failed to write config");

        let env = FakeEnv::new([("HOME", root.to_string_lossy().as_ref())]);
        let resolved = resolve_base_with_env(
            &env,
            ResolveOptions {
                cwd: Path::new("/workspace"),
                config_override: None,
                plugins_override: None,
            },
        )
        .expect("base paths should resolve");

        assert!(resolved.config_exists);
        assert_eq!(resolved.plugins_dir, root.join(".local/share/tpm/plugins"));
    }

    #[test]
    fn resolves_plugins_relative_to_the_config_file() {
        let root = unique_temp_dir("config-relative");
        let config_dir = root.join("config");
        fs::create_dir_all(&config_dir).expect("failed to create config dir");
        fs::write(
            config_dir.join("tpm.yaml"),
            r#"
version: 1
paths:
  plugins: ../plugins
plugins:
  - source: tmux-plugins/tmux-sensible
"#,
        )
        .expect("failed to write config");

        let env = FakeEnv::new([("HOME", root.to_string_lossy().as_ref())]);
        let config_file = config_dir.join("tpm.yaml");
        let resolved = resolve_with_env(
            &env,
            ResolveOptions {
                cwd: Path::new("/workspace"),
                config_override: Some(config_file.as_path()),
                plugins_override: None,
            },
        )
        .expect("paths should resolve");

        assert_eq!(resolved.plugins_dir, root.join("plugins"));
        assert!(resolved.config_exists);
    }

    #[test]
    fn cli_plugins_override_beats_config_plugins_dir() {
        let root = unique_temp_dir("cli-override");
        let config_dir = root.join("config");
        fs::create_dir_all(&config_dir).expect("failed to create config dir");
        fs::write(
            config_dir.join("tpm.yaml"),
            r#"
version: 1
paths:
  plugins: ../plugins-from-config
plugins: []
"#,
        )
        .expect("failed to write config");

        let env = FakeEnv::new([("HOME", root.to_string_lossy().as_ref())]);
        let cli_override = root.join("plugins-from-cli");
        let config_file = config_dir.join("tpm.yaml");
        let resolved = resolve_with_env(
            &env,
            ResolveOptions {
                cwd: Path::new("/workspace"),
                config_override: Some(config_file.as_path()),
                plugins_override: Some(cli_override.as_path()),
            },
        )
        .expect("paths should resolve");

        assert_eq!(resolved.plugins_dir, cli_override);
    }

    #[test]
    fn normalize_preserves_leading_relative_parent_components() {
        assert_eq!(
            normalize_lexically(Path::new("../../plugins")),
            PathBuf::from("../../plugins")
        );
    }

    #[test]
    fn normalize_collapses_parent_components_after_normal_segments() {
        assert_eq!(
            normalize_lexically(Path::new("vendor/../plugins/./tmux-open")),
            PathBuf::from("plugins/tmux-open")
        );
        assert_eq!(
            normalize_lexically(Path::new("vendor/plugins/../../..")),
            PathBuf::from("..")
        );
    }

    #[derive(Default)]
    struct FakeEnv {
        vars: BTreeMap<String, OsString>,
    }

    impl FakeEnv {
        fn new<const N: usize>(entries: [(&str, &str); N]) -> Self {
            Self {
                vars: entries
                    .into_iter()
                    .map(|(key, value)| (key.to_string(), OsString::from(value)))
                    .collect(),
            }
        }
    }

    impl EnvProvider for FakeEnv {
        fn var_os(&self, key: &str) -> Option<OsString> {
            self.vars.get(key).cloned()
        }
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be monotonic")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "tpm-rs-paths-test-{name}-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).expect("failed to create temp dir");
        directory
    }
}

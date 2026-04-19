use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    process,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{
    error::{AppError, Result},
    paths::normalize_lexically,
    plugin,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default, skip_serializing_if = "ConfigPaths::is_empty")]
    pub paths: ConfigPaths,
    #[serde(default)]
    pub plugins: Vec<PluginConfig>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigPaths {
    pub plugins: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginConfig {
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, rename = "ref", skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    #[serde(default = "default_enabled", skip_serializing_if = "is_true")]
    pub enabled: bool,
}

impl Config {
    pub fn new() -> Self {
        Self {
            version: default_version(),
            paths: ConfigPaths::default(),
            plugins: Vec::new(),
        }
    }

    pub fn load_if_exists(path: &Path) -> Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }

        let raw = fs::read_to_string(path).map_err(|source| AppError::ReadConfig {
            path: normalize_lexically(path),
            source,
        })?;

        let config: Self =
            serde_yaml::from_str(&raw).map_err(|source| AppError::InvalidConfig {
                path: normalize_lexically(path),
                message: source.to_string(),
            })?;

        config.validate(path)?;
        Ok(Some(config))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate(path)?;

        let path = normalize_lexically(path);
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(|source| AppError::CreateDirectory {
                path: normalize_lexically(parent),
                source,
            })?;
        }

        let mut serialized =
            serde_yaml::to_string(self).map_err(|source| AppError::SerializeConfig {
                path: path.clone(),
                source,
            })?;

        if let Some(stripped) = serialized.strip_prefix("---\n") {
            serialized = stripped.to_string();
        }

        if !serialized.ends_with('\n') {
            serialized.push('\n');
        }

        atomic_write_config(&path, serialized.as_bytes())
    }

    pub fn add_plugin(
        &mut self,
        source: &str,
        branch: Option<&str>,
        reference: Option<&str>,
    ) -> Result<String> {
        let source = source.trim();
        let install_name = plugin::install_name(source)?;
        reject_legacy_manager_plugin(source, &install_name)?;
        let branch = branch.map(str::trim).filter(|branch| !branch.is_empty());
        let reference = reference
            .map(str::trim)
            .filter(|reference| !reference.is_empty());
        if branch.is_some() && reference.is_some() {
            return Err(AppError::InvalidPluginSource {
                plugin_source: source.to_string(),
                message: "use either `branch` or `ref`, not both".to_string(),
            });
        }

        for existing in &self.plugins {
            let existing_install_name = plugin::install_name(&existing.source)?;
            if existing_install_name == install_name {
                return Err(AppError::PluginAlreadyConfigured {
                    plugin_source: source.to_string(),
                    install_name,
                    existing_source: existing.source.clone(),
                });
            }
        }

        self.plugins.push(PluginConfig {
            source: source.to_string(),
            branch: branch.map(str::to_owned),
            reference: reference.map(str::to_owned),
            enabled: default_enabled(),
        });

        Ok(install_name)
    }

    pub fn remove_plugin(&mut self, name: &str) -> Result<PluginConfig> {
        let name = name.trim();
        let Some(position) = self
            .plugins
            .iter()
            .position(|plugin| plugin::install_name(&plugin.source).ok().as_deref() == Some(name))
        else {
            return Err(AppError::PluginNotConfigured {
                name: name.to_string(),
            });
        };

        Ok(self.plugins.remove(position))
    }

    fn validate(&self, path: &Path) -> Result<()> {
        if self.version != 1 {
            return Err(AppError::InvalidConfig {
                path: normalize_lexically(path),
                message: format!("unsupported schema version {}; expected 1", self.version),
            });
        }

        for (index, plugin) in self.plugins.iter().enumerate() {
            if plugin.source.trim().is_empty() {
                return Err(AppError::InvalidConfig {
                    path: normalize_lexically(path),
                    message: format!("plugins[{index}].source must not be empty"),
                });
            }

            if plugin
                .branch
                .as_deref()
                .is_some_and(|branch| branch.trim().is_empty())
            {
                return Err(AppError::InvalidConfig {
                    path: normalize_lexically(path),
                    message: format!("plugins[{index}].branch must not be empty"),
                });
            }

            if plugin
                .reference
                .as_deref()
                .is_some_and(|reference| reference.trim().is_empty())
            {
                return Err(AppError::InvalidConfig {
                    path: normalize_lexically(path),
                    message: format!("plugins[{index}].ref must not be empty"),
                });
            }

            if plugin.branch.is_some() && plugin.reference.is_some() {
                return Err(AppError::InvalidConfig {
                    path: normalize_lexically(path),
                    message: format!("plugins[{index}] cannot set both `branch` and `ref`"),
                });
            }
        }

        let mut install_names = BTreeMap::<String, String>::new();
        for (index, plugin) in self.plugins.iter().enumerate() {
            let install_name =
                plugin::install_name(&plugin.source).map_err(|error| AppError::InvalidConfig {
                    path: normalize_lexically(path),
                    message: format!("plugins[{index}].source: {error}"),
                })?;
            reject_legacy_manager_plugin(&plugin.source, &install_name).map_err(|error| {
                AppError::InvalidConfig {
                    path: normalize_lexically(path),
                    message: format!("plugins[{index}].source: {error}"),
                }
            })?;

            if let Some(existing_source) =
                install_names.insert(install_name.clone(), plugin.source.clone())
            {
                return Err(AppError::InvalidConfig {
                    path: normalize_lexically(path),
                    message: format!(
                        "plugins[{index}].source resolves to duplicate install directory `{install_name}` already used by `{existing_source}`"
                    ),
                });
            }
        }

        Ok(())
    }
}

fn atomic_write_config(path: &Path, contents: &[u8]) -> Result<()> {
    let write_path = resolve_config_write_path(path)?;
    let (temp_path, mut file) = create_temp_config_file(&write_path)?;
    if let Err(source) = preserve_existing_file_permissions(&write_path, &file) {
        let _ = fs::remove_file(&temp_path);
        return Err(AppError::WriteConfig {
            path: path.to_path_buf(),
            source,
        });
    }

    if let Err(source) = file.write_all(contents).and_then(|()| file.sync_all()) {
        let _ = fs::remove_file(&temp_path);
        return Err(AppError::WriteConfig {
            path: path.to_path_buf(),
            source,
        });
    }
    drop(file);

    if let Err(source) = fs::rename(&temp_path, &write_path) {
        let _ = fs::remove_file(&temp_path);
        return Err(AppError::WriteConfig {
            path: path.to_path_buf(),
            source,
        });
    }

    Ok(())
}

fn resolve_config_write_path(path: &Path) -> Result<PathBuf> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            fs::canonicalize(path).map_err(|source| AppError::WriteConfig {
                path: path.to_path_buf(),
                source,
            })
        }
        Ok(_) => Ok(path.to_path_buf()),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(path.to_path_buf()),
        Err(source) => Err(AppError::WriteConfig {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn create_temp_config_file(path: &Path) -> Result<(PathBuf, fs::File)> {
    let file_name = path.file_name().ok_or_else(|| AppError::WriteConfig {
        path: path.to_path_buf(),
        source: io::Error::new(
            io::ErrorKind::InvalidInput,
            "config path must include a file name",
        ),
    })?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();

    let mut last_collision = None;
    for attempt in 0..16_u8 {
        let mut temp_name = OsString::from(".");
        temp_name.push(file_name);
        temp_name.push(format!(".tmp.{}.{nonce}.{attempt}", process::id()));
        let temp_path = parent
            .map(|parent| parent.join(&temp_name))
            .unwrap_or_else(|| PathBuf::from(&temp_name));

        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
        {
            Ok(file) => return Ok((temp_path, file)),
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {
                last_collision = Some(source);
            }
            Err(source) => {
                return Err(AppError::WriteConfig {
                    path: path.to_path_buf(),
                    source,
                });
            }
        }
    }

    Err(AppError::WriteConfig {
        path: path.to_path_buf(),
        source: last_collision.unwrap_or_else(|| {
            io::Error::new(
                io::ErrorKind::AlreadyExists,
                "could not allocate a temporary config file",
            )
        }),
    })
}

fn preserve_existing_file_permissions(path: &Path, file: &fs::File) -> io::Result<()> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => file.set_permissions(metadata.permissions()),
        Ok(_) => Ok(()),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(source),
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigPaths {
    fn is_empty(&self) -> bool {
        self.plugins.is_none()
    }
}

const fn default_version() -> u32 {
    1
}

const fn default_enabled() -> bool {
    true
}

const fn is_true(value: &bool) -> bool {
    *value
}

const LEGACY_MANAGER_INSTALL_NAME: &str = "tmux-plugins/tpm";

fn reject_legacy_manager_plugin(source: &str, install_name: &str) -> Result<()> {
    if install_name != LEGACY_MANAGER_INSTALL_NAME {
        return Ok(());
    }

    Err(AppError::InvalidPluginSource {
        plugin_source: source.to_string(),
        message: concat!(
            "the legacy TPM plugin manager is not supported in `tpm.yaml`; ",
            "use the `tpm` CLI and `run-shell \"tpm load\"` instead"
        )
        .to_string(),
    })
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{Config, PluginConfig};

    #[test]
    fn loads_minimal_config_and_applies_defaults() {
        let path = write_temp_file(
            "minimal",
            r#"
version: 1
plugins:
  - source: tmux-plugins/tmux-sensible
"#,
        );

        let config = Config::load_if_exists(&path)
            .expect("config should parse")
            .expect("config should exist");

        assert_eq!(config.version, 1);
        assert_eq!(config.plugins.len(), 1);
        assert_eq!(config.plugins[0].source, "tmux-plugins/tmux-sensible");
        assert_eq!(config.plugins[0].branch, None);
        assert_eq!(config.plugins[0].reference, None);
        assert!(config.plugins[0].enabled);
    }

    #[test]
    fn rejects_unsupported_version() {
        let path = write_temp_file(
            "bad-version",
            r#"
version: 2
plugins: []
"#,
        );

        let error = Config::load_if_exists(&path).expect_err("config should fail");
        assert!(
            error
                .to_string()
                .contains("unsupported schema version 2; expected 1")
        );
    }

    #[test]
    fn rejects_duplicate_install_names() {
        let path = write_temp_file(
            "duplicate-name",
            r#"
version: 1
plugins:
  - source: tmux-plugins/tmux-sensible
  - source: https://github.com/tmux-plugins/tmux-sensible.git
"#,
        );

        let error = Config::load_if_exists(&path).expect_err("config should fail");
        assert!(
            error
                .to_string()
                .contains("duplicate install directory `tmux-plugins/tmux-sensible`")
        );
    }

    #[test]
    fn rejects_unsupported_source_format() {
        let path = write_temp_file(
            "bad-source",
            r#"
version: 1
plugins:
  - source: foo
"#,
        );

        let error = Config::load_if_exists(&path).expect_err("config should fail");
        assert!(
            error
                .to_string()
                .contains("expected GitHub shorthand `owner/repo`")
        );
    }

    #[test]
    fn rejects_plugins_with_both_branch_and_ref() {
        let path = write_temp_file(
            "branch-and-ref",
            r#"
version: 1
plugins:
  - source: tmux-plugins/tmux-sensible
    branch: main
    ref: v1.0.0
"#,
        );

        let error = Config::load_if_exists(&path).expect_err("config should fail");
        assert!(
            error
                .to_string()
                .contains("cannot set both `branch` and `ref`")
        );
    }

    #[test]
    fn rejects_legacy_tpm_plugin_manager() {
        let path = write_temp_file(
            "legacy-tpm",
            r#"
version: 1
plugins:
  - source: tmux-plugins/tpm
"#,
        );

        let error = Config::load_if_exists(&path).expect_err("config should fail");
        assert!(
            error
                .to_string()
                .contains("the legacy TPM plugin manager is not supported")
        );
    }

    #[test]
    fn writes_deterministic_yaml() {
        let path = unique_temp_dir("save").join("nested").join("tpm.yaml");
        let mut config = Config::new();
        config
            .add_plugin("tmux-plugins/tmux-sensible", Some("main"), None)
            .expect("plugin should add");
        config
            .add_plugin("catppuccin/tmux", None, Some("v2.1.3"))
            .expect("plugin should add");

        config.save(&path).expect("config should save");

        let actual = fs::read_to_string(&path).expect("config should be readable");
        assert_eq!(
            actual,
            concat!(
                "version: 1\n",
                "plugins:\n",
                "- source: tmux-plugins/tmux-sensible\n",
                "  branch: main\n",
                "- source: catppuccin/tmux\n",
                "  ref: v2.1.3\n",
            )
        );
    }

    #[test]
    fn save_overwrites_existing_config_without_leaving_temp_files() {
        let directory = unique_temp_dir("save-overwrite");
        let path = directory.join("tpm.yaml");
        fs::write(&path, "version: 1\nplugins: []\n").expect("old config should be writable");
        let mut config = Config::new();
        config
            .add_plugin("tmux-plugins/tmux-sensible", None, None)
            .expect("plugin should add");

        config.save(&path).expect("config should save");

        let actual = fs::read_to_string(&path).expect("config should be readable");
        assert_eq!(
            actual,
            concat!(
                "version: 1\n",
                "plugins:\n",
                "- source: tmux-plugins/tmux-sensible\n",
            )
        );
        assert!(
            temp_config_files(&directory).is_empty(),
            "temporary config files should be cleaned up"
        );
    }

    #[test]
    fn save_cleans_up_temp_file_when_rename_fails() {
        let directory = unique_temp_dir("save-rename-fails");
        let path = directory.join("tpm.yaml");
        fs::create_dir_all(&path).expect("target directory should exist");
        let mut config = Config::new();
        config
            .add_plugin("tmux-plugins/tmux-sensible", None, None)
            .expect("plugin should add");

        let error = config
            .save(&path)
            .expect_err("save should fail when target is a directory");

        assert!(error.to_string().contains("failed to write config"));
        assert!(path.is_dir(), "failed save should leave the target intact");
        assert!(
            temp_config_files(&directory).is_empty(),
            "temporary config file should be removed after rename failure"
        );
    }

    #[cfg(unix)]
    #[test]
    fn save_preserves_existing_file_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let directory = unique_temp_dir("save-permissions");
        let path = directory.join("tpm.yaml");
        fs::write(&path, "version: 1\nplugins: []\n").expect("old config should be writable");
        let mut permissions = fs::metadata(&path)
            .expect("old config metadata should be readable")
            .permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(&path, permissions).expect("old config permissions should update");
        let mut config = Config::new();
        config
            .add_plugin("tmux-plugins/tmux-sensible", None, None)
            .expect("plugin should add");

        config.save(&path).expect("config should save");

        let mode = fs::metadata(&path)
            .expect("new config metadata should be readable")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn save_updates_symlink_target_without_replacing_the_symlink() {
        use std::os::unix::fs::symlink;

        let directory = unique_temp_dir("save-symlink");
        let target_dir = directory.join("target");
        fs::create_dir_all(&target_dir).expect("target directory should exist");
        let target_path = target_dir.join("tpm.yaml");
        let link_path = directory.join("tpm.yaml");
        fs::write(&target_path, "version: 1\nplugins: []\n")
            .expect("target config should be writable");
        symlink(&target_path, &link_path).expect("config symlink should be created");
        let mut config = Config::new();
        config
            .add_plugin("tmux-plugins/tmux-sensible", None, None)
            .expect("plugin should add");

        config.save(&link_path).expect("config should save");

        assert!(
            fs::symlink_metadata(&link_path)
                .expect("link metadata should be readable")
                .file_type()
                .is_symlink(),
            "save should keep the config symlink"
        );
        let actual = fs::read_to_string(&target_path).expect("target config should be readable");
        assert_eq!(
            actual,
            concat!(
                "version: 1\n",
                "plugins:\n",
                "- source: tmux-plugins/tmux-sensible\n",
            )
        );
        assert!(
            temp_config_files(&target_dir).is_empty(),
            "temporary config files should be cleaned up next to the target"
        );
    }

    #[test]
    fn remove_plugin_matches_derived_install_name() {
        let mut config = Config {
            version: 1,
            paths: Default::default(),
            plugins: vec![
                PluginConfig {
                    source: "tmux-plugins/tmux-sensible".to_string(),
                    branch: None,
                    reference: None,
                    enabled: true,
                },
                PluginConfig {
                    source: "catppuccin/tmux".to_string(),
                    branch: None,
                    reference: Some("v2.1.3".to_string()),
                    enabled: true,
                },
            ],
        };

        let removed = config
            .remove_plugin("catppuccin/tmux")
            .expect("plugin should be removed");

        assert_eq!(removed.source, "catppuccin/tmux");
        assert_eq!(config.plugins.len(), 1);
        assert_eq!(config.plugins[0].source, "tmux-plugins/tmux-sensible");
    }

    #[test]
    fn add_plugin_rejects_duplicate_install_name() {
        let mut config = Config::new();
        config
            .add_plugin("tmux-plugins/tmux-sensible", None, None)
            .expect("plugin should add");

        let error = config
            .add_plugin(
                "https://github.com/tmux-plugins/tmux-sensible.git",
                None,
                None,
            )
            .expect_err("duplicate plugin should fail");

        assert!(
            error
                .to_string()
                .contains("already configured by `tmux-plugins/tmux-sensible`")
        );
    }

    #[test]
    fn add_plugin_rejects_branch_and_ref_together() {
        let error = Config::new()
            .add_plugin("tmux-plugins/tmux-sensible", Some("main"), Some("v1.0.0"))
            .expect_err("conflicting plugin should fail");
        assert!(error.to_string().contains("use either `branch` or `ref`"));
    }

    #[test]
    fn add_plugin_rejects_legacy_tpm_plugin_manager() {
        let error = Config::new()
            .add_plugin("tmux-plugins/tpm", None, None)
            .expect_err("legacy manager should fail");
        assert!(
            error
                .to_string()
                .contains("the legacy TPM plugin manager is not supported")
        );
    }

    #[test]
    fn remove_plugin_rejects_missing_name() {
        let error = Config::new()
            .remove_plugin("missing")
            .expect_err("missing plugin should fail");
        assert!(
            error
                .to_string()
                .contains("plugin `missing` is not configured")
        );
    }

    fn write_temp_file(name: &str, contents: &str) -> PathBuf {
        let directory = unique_temp_dir(name);
        let path = directory.join("tpm.yaml");
        fs::write(&path, contents).expect("failed to write temp config");
        path
    }

    fn temp_config_files(directory: &Path) -> Vec<PathBuf> {
        fs::read_dir(directory)
            .expect("directory should be readable")
            .map(|entry| entry.expect("directory entry should be readable").path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(".tpm.yaml.tmp."))
            })
            .collect()
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be monotonic")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "tpm-rs-config-test-{name}-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).expect("failed to create temp dir");
        directory
    }

    #[allow(dead_code)]
    fn _assert_path(_: &Path) {}
}

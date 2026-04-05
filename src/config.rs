use std::{collections::BTreeMap, fs, path::Path};

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

        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(|source| AppError::CreateDirectory {
                path: normalize_lexically(parent),
                source,
            })?;
        }

        let path = normalize_lexically(path);
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

        fs::write(&path, serialized).map_err(|source| AppError::WriteConfig { path, source })
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

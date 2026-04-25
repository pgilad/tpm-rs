use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Component, Path, PathBuf},
    process,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{
    error::{AppError, Result},
    paths::normalize_lexically,
};

const MANIFEST_DIR: &str = ".tpm-rs";
const MANIFEST_FILE: &str = "managed.yaml";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ManagedManifest {
    #[serde(default = "default_version")]
    version: u32,
    #[serde(default)]
    plugins: BTreeMap<String, ManagedPlugin>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ManagedPlugin {
    pub(crate) source: String,
    pub(crate) clone_source: String,
    pub(crate) path: String,
}

impl ManagedManifest {
    pub(crate) fn load_or_default(plugins_dir: &Path) -> Result<Self> {
        let path = manifest_path(plugins_dir);
        if !path.exists() {
            return Ok(Self::default());
        }

        let raw = fs::read_to_string(&path).map_err(|source| AppError::ReadManifest {
            path: path.clone(),
            source,
        })?;
        let manifest: Self =
            serde_norway::from_str(&raw).map_err(|source| AppError::InvalidManifest {
                path: path.clone(),
                message: source.to_string(),
            })?;
        manifest.validate(&path)?;
        Ok(manifest)
    }

    pub(crate) fn save(&self, plugins_dir: &Path) -> Result<()> {
        let path = manifest_path(plugins_dir);
        self.validate(&path)?;

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
            serde_norway::to_string(self).map_err(|source| AppError::InvalidManifest {
                path: path.clone(),
                message: source.to_string(),
            })?;
        if let Some(stripped) = serialized.strip_prefix("---\n") {
            serialized = stripped.to_string();
        }
        if !serialized.ends_with('\n') {
            serialized.push('\n');
        }

        atomic_write_manifest(&path, serialized.as_bytes())
    }

    pub(crate) fn record_plugin(
        &mut self,
        plugins_dir: &Path,
        install_name: &str,
        source: &str,
        clone_source: &str,
        install_dir: &Path,
    ) -> Result<bool> {
        let path = relative_plugin_path(plugins_dir, install_dir)?;
        let entry = ManagedPlugin {
            source: source.to_string(),
            clone_source: clone_source.to_string(),
            path,
        };

        if self.plugins.get(install_name) == Some(&entry) {
            return Ok(false);
        }

        self.plugins.insert(install_name.to_string(), entry);
        Ok(true)
    }

    pub(crate) fn remove(&mut self, install_name: &str) -> Option<ManagedPlugin> {
        self.plugins.remove(install_name)
    }

    pub(crate) fn entries(&self) -> impl Iterator<Item = (&String, &ManagedPlugin)> {
        self.plugins.iter()
    }

    fn validate(&self, path: &Path) -> Result<()> {
        if self.version != default_version() {
            return Err(AppError::InvalidManifest {
                path: path.to_path_buf(),
                message: format!("unsupported schema version {}; expected 1", self.version),
            });
        }

        for (name, plugin) in &self.plugins {
            if name.trim().is_empty() {
                return Err(AppError::InvalidManifest {
                    path: path.to_path_buf(),
                    message: "managed plugin names must not be empty".to_string(),
                });
            }

            validate_manifest_relative_path(&plugin.path).map_err(|message| {
                AppError::InvalidManifest {
                    path: path.to_path_buf(),
                    message: format!(
                        "plugin `{name}` has invalid path `{}`: {message}",
                        plugin.path
                    ),
                }
            })?;
        }

        Ok(())
    }
}

impl Default for ManagedManifest {
    fn default() -> Self {
        Self {
            version: default_version(),
            plugins: BTreeMap::new(),
        }
    }
}

pub(crate) fn manifest_path(plugins_dir: &Path) -> PathBuf {
    normalize_lexically(&plugins_dir.join(MANIFEST_DIR).join(MANIFEST_FILE))
}

pub(crate) fn entry_install_dir(plugins_dir: &Path, plugin: &ManagedPlugin) -> Result<PathBuf> {
    validate_manifest_relative_path(&plugin.path).map_err(|message| AppError::InvalidManifest {
        path: manifest_path(plugins_dir),
        message: format!("plugin path `{}` is invalid: {message}", plugin.path),
    })?;

    Ok(normalize_lexically(&plugins_dir.join(&plugin.path)))
}

fn relative_plugin_path(plugins_dir: &Path, install_dir: &Path) -> Result<String> {
    let plugins_dir = normalize_lexically(plugins_dir);
    let install_dir = normalize_lexically(install_dir);
    let relative =
        install_dir
            .strip_prefix(&plugins_dir)
            .map_err(|_| AppError::InvalidManifest {
                path: manifest_path(&plugins_dir),
                message: format!(
                    "plugin checkout {} is outside managed plugin root {}",
                    install_dir.display(),
                    plugins_dir.display()
                ),
            })?;

    let relative = relative.to_string_lossy().into_owned();
    validate_manifest_relative_path(&relative).map_err(|message| AppError::InvalidManifest {
        path: manifest_path(&plugins_dir),
        message: format!("plugin path `{relative}` is invalid: {message}"),
    })?;
    Ok(relative)
}

fn validate_manifest_relative_path(path: &str) -> std::result::Result<(), String> {
    let path = Path::new(path);
    if path.as_os_str().is_empty() {
        return Err("path must not be empty".to_string());
    }

    if path.is_absolute() {
        return Err("path must be relative".to_string());
    }

    let mut components = path.components();
    let first = components
        .next()
        .ok_or_else(|| "path must not be empty".to_string())?;
    match first {
        Component::Normal(part) if part != MANIFEST_DIR => {}
        Component::Normal(_) => {
            return Err(format!("path must not start with `{MANIFEST_DIR}`"));
        }
        _ => return Err("path must contain only normal relative components".to_string()),
    }

    for component in components {
        if !matches!(component, Component::Normal(_)) {
            return Err("path must contain only normal relative components".to_string());
        }
    }

    Ok(())
}

fn atomic_write_manifest(path: &Path, contents: &[u8]) -> Result<()> {
    let (temp_path, mut file) = create_temp_manifest_file(path)?;
    if let Err(source) = file.write_all(contents).and_then(|()| file.sync_all()) {
        let _ = fs::remove_file(&temp_path);
        return Err(AppError::WriteManifest {
            path: path.to_path_buf(),
            source,
        });
    }
    drop(file);

    if let Err(source) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(AppError::WriteManifest {
            path: path.to_path_buf(),
            source,
        });
    }

    Ok(())
}

fn create_temp_manifest_file(path: &Path) -> Result<(PathBuf, fs::File)> {
    let file_name = path.file_name().ok_or_else(|| AppError::WriteManifest {
        path: path.to_path_buf(),
        source: io::Error::new(
            io::ErrorKind::InvalidInput,
            "manifest path must include a file name",
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
                return Err(AppError::WriteManifest {
                    path: path.to_path_buf(),
                    source,
                });
            }
        }
    }

    Err(AppError::WriteManifest {
        path: path.to_path_buf(),
        source: last_collision.unwrap_or_else(|| {
            io::Error::new(
                io::ErrorKind::AlreadyExists,
                "could not allocate a temporary manifest file",
            )
        }),
    })
}

const fn default_version() -> u32 {
    1
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{ManagedManifest, entry_install_dir, manifest_path};

    #[test]
    fn records_plugin_paths_relative_to_the_plugins_dir() {
        let root = unique_temp_dir("record");
        let plugins_dir = root.join("plugins");
        let install_dir = plugins_dir.join("tmux-plugins").join("tmux-sensible");
        let mut manifest = ManagedManifest::default();

        let changed = manifest
            .record_plugin(
                &plugins_dir,
                "tmux-plugins/tmux-sensible",
                "tmux-plugins/tmux-sensible",
                "https://github.com/tmux-plugins/tmux-sensible",
                &install_dir,
            )
            .expect("plugin should record");

        assert!(changed);
        let entry = manifest
            .entries()
            .next()
            .expect("manifest should contain entry")
            .1;
        assert_eq!(entry.path, "tmux-plugins/tmux-sensible");
        assert_eq!(
            entry_install_dir(&plugins_dir, entry).expect("entry path should resolve"),
            install_dir
        );
    }

    #[test]
    fn rejects_manifest_paths_that_escape_the_plugins_dir() {
        let root = unique_temp_dir("escape");
        write_raw_manifest(
            &root,
            "version: 1\nplugins:\n  bad:\n    source: bad/source\n    clone_source: https://example.invalid/bad/source\n    path: ../bad\n",
        );

        let error = ManagedManifest::load_or_default(&root).expect_err("manifest should fail");

        assert!(
            error
                .to_string()
                .contains("path must contain only normal relative components")
        );
    }

    #[test]
    fn rejects_unsupported_manifest_versions() {
        let root = unique_temp_dir("unsupported-version");
        write_raw_manifest(&root, "version: 2\nplugins: {}\n");

        let error = ManagedManifest::load_or_default(&root).expect_err("manifest should fail");

        assert!(
            error
                .to_string()
                .contains("unsupported schema version 2; expected 1")
        );
    }

    #[test]
    fn rejects_manifest_entries_with_unknown_fields() {
        let root = unique_temp_dir("unknown-field");
        write_raw_manifest(
            &root,
            concat!(
                "version: 1\n",
                "plugins:\n",
                "  tmux-open:\n",
                "    source: tmux-open\n",
                "    clone_source: https://example.invalid/tmux-open\n",
                "    path: tmux-open\n",
                "    checksum: not-supported\n",
            ),
        );

        let error = ManagedManifest::load_or_default(&root).expect_err("manifest should fail");

        assert!(error.to_string().contains("unknown field `checksum`"));
    }

    fn write_raw_manifest(root: &Path, contents: &str) {
        let manifest_path = manifest_path(root);
        fs::create_dir_all(manifest_path.parent().expect("manifest should have parent"))
            .expect("manifest parent should exist");
        fs::write(&manifest_path, contents).expect("manifest should be writable");
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be monotonic")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "tpm-rs-manifest-test-{name}-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).expect("temp directory should exist");
        directory
    }
}

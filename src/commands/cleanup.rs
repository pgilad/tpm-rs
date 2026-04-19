use std::{
    collections::BTreeSet,
    fs, io,
    path::{Path, PathBuf},
};

use crate::{
    commands::{
        progress::{ProgressStream, TerminalTheme},
        resolved_paths, sync,
    },
    config::Config,
    error::{AppError, Result},
    manifest::{self, ManagedManifest},
    user_path::display_user_path,
};

const LEGACY_TPM_INSTALL_NAME: &str = "tpm";

pub fn run(config_override: Option<&Path>, plugins_override: Option<&Path>) -> Result<()> {
    let paths = resolved_paths(config_override, plugins_override)?;
    let config =
        Config::load_if_exists(&paths.config_file)?.ok_or_else(|| AppError::ConfigNotFound {
            path: paths.config_file.clone(),
        })?;

    let configured = config
        .plugins
        .iter()
        .map(|plugin_config| sync::configured_plugin(&paths, plugin_config))
        .collect::<Result<Vec<_>>>()?;
    let declared = configured
        .iter()
        .map(|plugin| plugin.install_name.clone())
        .collect::<BTreeSet<_>>();

    let mut manifest = ManagedManifest::load_or_default(&paths.plugins_dir)?;
    let mut manifest_changed = sync::adopt_configured_plugins(&mut manifest, &paths, &configured)?;
    let report = cleanup_plugins_dir(&paths.plugins_dir, &declared, &mut manifest)?;
    manifest_changed |= report.manifest_changed;
    if paths.plugins_dir.exists()
        && (manifest_changed || !manifest::manifest_path(&paths.plugins_dir).exists())
    {
        manifest.save(&paths.plugins_dir)?;
    }
    print_report(&report);

    if report.failed.is_empty() {
        Ok(())
    } else {
        Err(AppError::CommandFailed {
            command: "cleanup",
            failed_operations: report.failed.len(),
        })
    }
}

#[derive(Debug, Default)]
pub(crate) struct CleanupReport {
    pub(crate) removed: Vec<PathBuf>,
    pub(crate) preserved: Vec<PathBuf>,
    pub(crate) failed: Vec<(PathBuf, io::Error)>,
    pub(crate) manifest_changed: bool,
}

pub(crate) fn cleanup_plugins_dir(
    plugins_dir: &Path,
    declared: &BTreeSet<String>,
    manifest: &mut ManagedManifest,
) -> Result<CleanupReport> {
    if !plugins_dir.exists() {
        return Ok(CleanupReport::default());
    }

    if !plugins_dir.is_dir() {
        return Err(AppError::InspectPath {
            path: plugins_dir.to_path_buf(),
            source: io::Error::other("expected a directory"),
        });
    }

    let declared_manifest_paths = manifest
        .entries()
        .filter(|(install_name, _)| declared.contains(*install_name))
        .map(|(_, plugin)| manifest::entry_install_dir(plugins_dir, plugin))
        .collect::<Result<BTreeSet<_>>>()?;

    let mut stale_directories = manifest
        .entries()
        .filter(|(install_name, _)| !declared.contains(*install_name))
        .map(|(install_name, plugin)| {
            manifest::entry_install_dir(plugins_dir, plugin)
                .map(|path| (install_name.clone(), plugin.path.clone(), path))
        })
        .collect::<Result<Vec<_>>>()?;
    stale_directories.sort_by(|left, right| left.1.cmp(&right.1));

    let mut preserved = legacy_preserved_paths(plugins_dir, declared);
    preserved.sort();

    let mut report = CleanupReport {
        preserved,
        ..CleanupReport::default()
    };

    for (install_name, _, path) in stale_directories {
        if install_name == LEGACY_TPM_INSTALL_NAME
            || path == plugins_dir.join(LEGACY_TPM_INSTALL_NAME)
        {
            manifest.remove(&install_name);
            report.manifest_changed = true;
            continue;
        }

        if declared_manifest_paths.contains(&path) {
            manifest.remove(&install_name);
            report.manifest_changed = true;
            continue;
        }

        if !path.exists() {
            manifest.remove(&install_name);
            report.manifest_changed = true;
            continue;
        }

        if !path.is_dir() {
            report
                .failed
                .push((path, io::Error::other("expected a directory")));
            continue;
        }

        match fs::remove_dir_all(&path) {
            Ok(()) => {
                prune_empty_parent_dirs(path.parent(), plugins_dir);
                manifest.remove(&install_name);
                report.manifest_changed = true;
                report.removed.push(path);
            }
            Err(source) => report.failed.push((path, source)),
        }
    }

    Ok(report)
}

fn legacy_preserved_paths(plugins_dir: &Path, declared: &BTreeSet<String>) -> Vec<PathBuf> {
    if declared.contains(LEGACY_TPM_INSTALL_NAME) {
        return Vec::new();
    }

    let path = plugins_dir.join(LEGACY_TPM_INSTALL_NAME);
    if path.is_dir() {
        vec![path]
    } else {
        Vec::new()
    }
}

fn prune_empty_parent_dirs(path: Option<&Path>, plugins_dir: &Path) {
    let mut current = path.map(Path::to_path_buf);
    while let Some(directory) = current {
        if directory == plugins_dir {
            return;
        }

        match fs::remove_dir(&directory) {
            Ok(()) => current = directory.parent().map(Path::to_path_buf),
            Err(source)
                if matches!(
                    source.kind(),
                    io::ErrorKind::DirectoryNotEmpty | io::ErrorKind::NotFound
                ) =>
            {
                return;
            }
            Err(_) => return,
        }
    }
}

fn print_report(report: &CleanupReport) {
    let stdout_theme = TerminalTheme::detect(ProgressStream::Stdout);
    let stderr_theme = TerminalTheme::detect(ProgressStream::Stderr);

    for path in &report.removed {
        println!(
            "{} stale plugin directory {}",
            stdout_theme.success("Removed"),
            display_user_path(path)
        );
    }

    for path in &report.preserved {
        println!(
            "{} legacy TPM checkout {}",
            stdout_theme.warning("Preserved"),
            display_user_path(path)
        );
    }

    for (path, error) in &report.failed {
        eprintln!(
            "{} to remove stale plugin directory {}: {}",
            stderr_theme.failure("Failed"),
            display_user_path(path),
            error
        );
    }

    if report.removed.is_empty() && report.failed.is_empty() && report.preserved.is_empty() {
        println!(
            "{}",
            stdout_theme.muted("No stale plugin directories found")
        );
    }
}

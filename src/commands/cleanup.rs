use std::{
    collections::BTreeSet,
    fs, io,
    path::{Path, PathBuf},
};

use crate::{
    commands::resolved_paths,
    config::Config,
    error::{AppError, Result},
    plugin,
    user_path::display_user_path,
};

const LEGACY_TPM_INSTALL_NAME: &str = "tpm";

pub fn run(config_override: Option<&Path>, plugins_override: Option<&Path>) -> Result<()> {
    let paths = resolved_paths(config_override, plugins_override)?;
    let config =
        Config::load_if_exists(&paths.config_file)?.ok_or_else(|| AppError::ConfigNotFound {
            path: paths.config_file.clone(),
        })?;

    let declared = config
        .plugins
        .iter()
        .map(|plugin_config| plugin::install_name(&plugin_config.source).map(PathBuf::from))
        .collect::<Result<BTreeSet<_>>>()?;

    let report = cleanup_plugins_dir(&paths.plugins_dir, &declared)?;
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
}

pub(crate) fn cleanup_plugins_dir(
    plugins_dir: &Path,
    declared: &BTreeSet<PathBuf>,
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

    let mut stale_directories = Vec::new();
    let mut preserved = Vec::new();

    collect_cleanup_targets(
        plugins_dir,
        Path::new(""),
        declared,
        &mut stale_directories,
        &mut preserved,
    )?;

    stale_directories.sort_by(|left, right| left.0.cmp(&right.0));
    preserved.sort();

    let mut report = CleanupReport {
        preserved,
        ..CleanupReport::default()
    };

    for (_, path) in stale_directories {
        match fs::remove_dir_all(&path) {
            Ok(()) => {
                prune_empty_parent_dirs(path.parent(), plugins_dir);
                report.removed.push(path);
            }
            Err(source) => report.failed.push((path, source)),
        }
    }

    Ok(report)
}

fn collect_cleanup_targets(
    plugins_dir: &Path,
    relative_dir: &Path,
    declared: &BTreeSet<PathBuf>,
    stale_directories: &mut Vec<(String, PathBuf)>,
    preserved: &mut Vec<PathBuf>,
) -> Result<()> {
    let current_dir = if relative_dir.as_os_str().is_empty() {
        plugins_dir.to_path_buf()
    } else {
        plugins_dir.join(relative_dir)
    };

    let entries = fs::read_dir(&current_dir).map_err(|source| AppError::InspectPath {
        path: current_dir.clone(),
        source,
    })?;

    for entry in entries {
        let entry = entry.map_err(|source| AppError::InspectPath {
            path: current_dir.clone(),
            source,
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|source| AppError::InspectPath {
            path: path.clone(),
            source,
        })?;

        if !file_type.is_dir() {
            continue;
        }

        let relative_path = relative_dir.join(entry.file_name());
        if relative_path == Path::new(LEGACY_TPM_INSTALL_NAME) {
            preserved.push(path);
            continue;
        }

        if declared.contains(&relative_path) {
            continue;
        }

        if declared
            .iter()
            .any(|declared_path| declared_path.starts_with(&relative_path))
        {
            collect_cleanup_targets(
                plugins_dir,
                &relative_path,
                declared,
                stale_directories,
                preserved,
            )?;
            continue;
        }

        stale_directories.push((relative_path.display().to_string(), path));
    }

    Ok(())
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
    for path in &report.removed {
        println!("Removed stale plugin directory {}", display_user_path(path));
    }

    for path in &report.preserved {
        println!("Preserved legacy TPM checkout {}", display_user_path(path));
    }

    for (path, error) in &report.failed {
        eprintln!(
            "Failed to remove stale plugin directory {}: {}",
            display_user_path(path),
            error
        );
    }

    if report.removed.is_empty() && report.failed.is_empty() && report.preserved.is_empty() {
        println!("No stale plugin directories found");
    }
}

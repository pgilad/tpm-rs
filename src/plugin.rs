use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{
    error::{AppError, Result},
    paths::normalize_lexically,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceKind {
    Remote,
    GitHubShorthand,
    LocalPath,
}

pub(crate) fn classify_source(source: &str) -> Result<SourceKind> {
    let trimmed = source.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidPluginSource {
            plugin_source: source.to_string(),
            message: "expected a non-empty source".to_string(),
        });
    }

    if is_remote_source(trimmed) {
        return Ok(SourceKind::Remote);
    }

    if is_explicit_local_path(trimmed) {
        return Ok(SourceKind::LocalPath);
    }

    if looks_like_github_shorthand(trimmed) {
        return Ok(SourceKind::GitHubShorthand);
    }

    if looks_like_relative_local_path(trimmed) {
        return Ok(SourceKind::LocalPath);
    }

    Err(AppError::InvalidPluginSource {
        plugin_source: source.to_string(),
        message: concat!(
            "expected GitHub shorthand `owner/repo`, a full Git URL, an SSH Git URL, ",
            "an absolute local path, or a relative local path; use `./` or `../` for ",
            "local paths that could be mistaken for `owner/repo`"
        )
        .to_string(),
    })
}

pub fn install_name(source: &str) -> Result<String> {
    match classify_source(source)? {
        SourceKind::GitHubShorthand => install_name_from_relative_path(source),
        SourceKind::Remote => install_name_from_remote(source),
        SourceKind::LocalPath => install_name_from_local_path(source),
    }
}

pub fn install_dir(root: &std::path::Path, source: &str) -> Result<std::path::PathBuf> {
    Ok(normalize_lexically(&root.join(install_name(source)?)))
}

fn is_remote_source(source: &str) -> bool {
    source.contains("://") || source.starts_with("git@")
}

fn is_explicit_local_path(source: &str) -> bool {
    source.starts_with('/')
        || source == "~"
        || source.starts_with("~/")
        || source.starts_with("./")
        || source.starts_with("../")
}

fn looks_like_github_shorthand(source: &str) -> bool {
    if source.contains(':') || source.contains('\\') {
        return false;
    }

    let source = source.trim_end_matches('/');
    let mut segments = source.split('/');
    matches!(
        (segments.next(), segments.next(), segments.next()),
        (Some(owner), Some(repo), None)
            if is_simple_path_segment(owner) && is_simple_path_segment(repo)
    )
}

fn looks_like_relative_local_path(source: &str) -> bool {
    source.contains('/') && !source.contains(':') && !source.contains('\\')
}

fn is_simple_path_segment(segment: &str) -> bool {
    !segment.is_empty() && segment != "." && segment != ".."
}

fn install_name_from_local_path(source: &str) -> Result<String> {
    let trimmed = source.trim().trim_end_matches('/');

    let segment = trimmed
        .rsplit(['/', ':'])
        .find(|segment| !segment.is_empty())
        .ok_or_else(|| AppError::InvalidPluginSource {
            plugin_source: source.to_string(),
            message: "could not derive a repository or directory name".to_string(),
        })?;

    let name = segment.strip_suffix(".git").unwrap_or(segment);
    if !is_simple_path_segment(name) {
        return Err(AppError::InvalidPluginSource {
            plugin_source: source.to_string(),
            message: "could not derive a repository or directory name".to_string(),
        });
    }

    Ok(name.to_string())
}

fn install_name_from_relative_path(source: &str) -> Result<String> {
    normalize_install_path(source, source)
}

fn install_name_from_remote(source: &str) -> Result<String> {
    let trimmed = source.trim();
    if let Some(path) = trimmed.strip_prefix("file://") {
        return install_name_from_local_path(path);
    }

    let remote_path = if let Some((_, path)) = trimmed
        .strip_prefix("git@")
        .and_then(|value| value.split_once(':'))
    {
        path
    } else {
        let (_, remainder) =
            trimmed
                .split_once("://")
                .ok_or_else(|| AppError::InvalidPluginSource {
                    plugin_source: source.to_string(),
                    message: "could not derive a repository or directory name".to_string(),
                })?;
        let remainder = remainder.trim_start_matches('/');
        let (_, path) = remainder
            .split_once('/')
            .ok_or_else(|| AppError::InvalidPluginSource {
                plugin_source: source.to_string(),
                message: "could not derive a repository or directory name".to_string(),
            })?;
        path
    };

    normalize_install_path(remote_path, source)
}

fn normalize_install_path(path: &str, source: &str) -> Result<String> {
    let segments = path
        .trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();

    if segments.is_empty() {
        return Err(AppError::InvalidPluginSource {
            plugin_source: source.to_string(),
            message: "could not derive a repository or directory name".to_string(),
        });
    }

    let segment_count = segments.len();
    let mut normalized = Vec::with_capacity(segment_count);
    for (index, segment) in segments.into_iter().enumerate() {
        let segment = if index + 1 == segment_count {
            segment.strip_suffix(".git").unwrap_or(segment)
        } else {
            segment
        };

        if !is_simple_path_segment(segment) {
            return Err(AppError::InvalidPluginSource {
                plugin_source: source.to_string(),
                message: "could not derive a repository or directory name".to_string(),
            });
        }

        normalized.push(segment);
    }

    Ok(normalized.join("/"))
}

pub fn executable_entrypoints(root: &Path) -> Result<Vec<PathBuf>> {
    let entries = fs::read_dir(root).map_err(|source| AppError::InspectPath {
        path: normalize_lexically(root),
        source,
    })?;

    let mut entrypoints = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| AppError::InspectPath {
            path: normalize_lexically(root),
            source,
        })?;
        let path = entry.path();
        let metadata = fs::metadata(&path).map_err(|source| AppError::InspectPath {
            path: normalize_lexically(&path),
            source,
        })?;

        if metadata.is_file() && is_executable(&metadata) && has_tmux_entrypoint_name(&path) {
            entrypoints.push(normalize_lexically(&path));
        }
    }

    entrypoints.sort();
    Ok(entrypoints)
}

fn has_tmux_entrypoint_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".tmux"))
}

#[cfg(unix)]
fn is_executable(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;

    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_: &fs::Metadata) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    use super::{SourceKind, classify_source, executable_entrypoints, install_dir, install_name};

    #[test]
    fn derives_install_name_from_owner_repo() {
        let name = install_name("tmux-plugins/tmux-sensible").expect("source should parse");
        assert_eq!(name, "tmux-plugins/tmux-sensible");
    }

    #[test]
    fn derives_install_name_from_https_git_url() {
        let name =
            install_name("https://github.com/catppuccin/tmux.git").expect("source should parse");
        assert_eq!(name, "catppuccin/tmux");
    }

    #[test]
    fn derives_install_name_from_ssh_git_url() {
        let name = install_name("git@github.com:tmux-plugins/tmux-sensible.git")
            .expect("source should parse");
        assert_eq!(name, "tmux-plugins/tmux-sensible");
    }

    #[test]
    fn derives_install_name_from_file_url() {
        let name = install_name("file:///tmp/tmux-open.git").expect("source should parse");
        assert_eq!(name, "tmux-open");
    }

    #[test]
    fn derives_install_name_from_local_relative_path() {
        let name = install_name("../plugins/tmux-open").expect("source should parse");
        assert_eq!(name, "tmux-open");
    }

    #[test]
    fn derives_install_dir_from_root() {
        let path =
            install_dir(Path::new("/tmp/plugins"), "tmux-plugins/tmux-resurrect").expect("dir");
        assert_eq!(
            path,
            PathBuf::from("/tmp/plugins/tmux-plugins/tmux-resurrect")
        );
    }

    #[test]
    fn rejects_empty_source() {
        let error = install_name("   ").expect_err("source should fail");
        assert!(error.to_string().contains("expected a non-empty source"));
    }

    #[test]
    fn rejects_bare_single_segment_source() {
        let error = install_name("foo").expect_err("source should fail");
        assert!(
            error
                .to_string()
                .contains("expected GitHub shorthand `owner/repo`")
        );
    }

    #[test]
    fn classifies_explicit_relative_local_paths() {
        let kind = classify_source("./plugins/tmux-open").expect("source should parse");
        assert_eq!(kind, SourceKind::LocalPath);
    }

    #[test]
    fn classifies_multi_segment_relative_paths_as_local() {
        let kind = classify_source("vendor/plugins/tmux-open").expect("source should parse");
        assert_eq!(kind, SourceKind::LocalPath);
    }

    #[cfg(unix)]
    #[test]
    fn finds_only_executable_root_tmux_entrypoints_in_sorted_order() {
        let root = unique_temp_dir("entrypoints");

        write_file(&root.join("b.tmux"), "#!/bin/sh\n");
        write_file(&root.join("a.tmux"), "#!/bin/sh\n");
        write_file(&root.join("ignored.txt"), "#!/bin/sh\n");
        write_file(&root.join("nested").join("ignored.tmux"), "#!/bin/sh\n");
        set_executable(&root.join("b.tmux"));
        set_executable(&root.join("a.tmux"));
        set_executable(&root.join("nested").join("ignored.tmux"));

        let entrypoints = executable_entrypoints(&root).expect("entrypoints should load");

        assert_eq!(entrypoints, vec![root.join("a.tmux"), root.join("b.tmux")]);
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be monotonic")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("tpm-rs-plugin-test-{name}-{stamp}"));
        fs::create_dir_all(&directory).expect("temp directory should be created");
        directory
    }

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent directory should exist");
        }
        fs::write(path, contents).expect("file should be writable");
    }

    #[cfg(unix)]
    fn set_executable(path: &Path) {
        let metadata = fs::metadata(path).expect("metadata should be readable");
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).expect("permissions should update");
    }
}

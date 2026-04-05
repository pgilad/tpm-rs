use std::fs;

mod support;

use support::{run_tpm, unique_temp_dir};

#[test]
fn cleanup_removes_undeclared_plugin_directories() {
    let workspace = unique_temp_dir("cleanup-remove");
    let config_path = workspace.join("config").join("tpm.yaml");
    let plugins_dir = workspace.join("plugins");

    write_config(
        &config_path,
        concat!(
            "version: 1\n",
            "paths:\n",
            "  plugins: ../plugins\n",
            "plugins:\n",
            "- source: tmux-plugins/tmux-sensible\n",
        ),
    );
    fs::create_dir_all(plugins_dir.join("tmux-plugins").join("tmux-sensible"))
        .expect("declared plugin directory should exist");
    fs::create_dir_all(plugins_dir.join("tmux-open")).expect("stale plugin directory should exist");
    fs::create_dir_all(plugins_dir.join("zzz")).expect("stale plugin directory should exist");

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "cleanup",
        ],
    );

    assert!(
        output.status.success(),
        "cleanup should succeed: {output:?}"
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        format!(
            "Removed stale plugin directory {}\nRemoved stale plugin directory {}\n",
            plugins_dir.join("tmux-open").display(),
            plugins_dir.join("zzz").display(),
        )
    );
    assert!(
        plugins_dir
            .join("tmux-plugins")
            .join("tmux-sensible")
            .is_dir()
    );
    assert!(!plugins_dir.join("tmux-open").exists());
    assert!(!plugins_dir.join("zzz").exists());
}

#[test]
fn cleanup_removes_undeclared_namespaced_plugin_directories() {
    let workspace = unique_temp_dir("cleanup-remove-namespaced");
    let config_path = workspace.join("config").join("tpm.yaml");
    let plugins_dir = workspace.join("plugins");

    write_config(
        &config_path,
        concat!(
            "version: 1\n",
            "paths:\n",
            "  plugins: ../plugins\n",
            "plugins:\n",
            "- source: tmux-plugins/tmux-sensible\n",
        ),
    );
    fs::create_dir_all(plugins_dir.join("tmux-plugins").join("tmux-sensible"))
        .expect("declared plugin directory should exist");
    fs::create_dir_all(plugins_dir.join("tmux-plugins").join("tmux-open"))
        .expect("stale namespaced plugin directory should exist");

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "cleanup",
        ],
    );

    assert!(
        output.status.success(),
        "cleanup should succeed: {output:?}"
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        format!(
            "Removed stale plugin directory {}\n",
            plugins_dir.join("tmux-plugins").join("tmux-open").display(),
        )
    );
    assert!(
        plugins_dir
            .join("tmux-plugins")
            .join("tmux-sensible")
            .is_dir()
    );
    assert!(!plugins_dir.join("tmux-plugins").join("tmux-open").exists());
}

#[test]
fn cleanup_preserves_legacy_tpm_checkout() {
    let workspace = unique_temp_dir("cleanup-preserve-tpm");
    let config_path = workspace.join("config").join("tpm.yaml");
    let plugins_dir = workspace.join("plugins");

    write_config(
        &config_path,
        concat!(
            "version: 1\n",
            "paths:\n",
            "  plugins: ../plugins\n",
            "plugins:\n",
            "- source: tmux-plugins/tmux-sensible\n",
        ),
    );
    fs::create_dir_all(plugins_dir.join("tmux-plugins").join("tmux-sensible"))
        .expect("declared plugin directory should exist");
    fs::create_dir_all(plugins_dir.join("tpm")).expect("legacy tpm directory should exist");
    fs::create_dir_all(plugins_dir.join("tmux-open")).expect("stale plugin directory should exist");

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "cleanup",
        ],
    );

    assert!(
        output.status.success(),
        "cleanup should succeed: {output:?}"
    );
    assert!(
        String::from_utf8(output.stdout)
            .expect("stdout should be utf-8")
            .contains(&format!(
                "Preserved legacy TPM checkout {}",
                plugins_dir.join("tpm").display()
            ))
    );
    assert!(plugins_dir.join("tpm").is_dir());
    assert!(!plugins_dir.join("tmux-open").exists());
}

#[test]
fn cleanup_succeeds_when_plugins_dir_is_missing() {
    let workspace = unique_temp_dir("cleanup-missing-dir");
    let config_path = workspace.join("config").join("tpm.yaml");

    write_config(
        &config_path,
        concat!(
            "version: 1\n",
            "paths:\n",
            "  plugins: ../plugins\n",
            "plugins:\n",
            "- source: tmux-plugins/tmux-sensible\n",
        ),
    );

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "cleanup",
        ],
    );

    assert!(
        output.status.success(),
        "cleanup should succeed: {output:?}"
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        "No stale plugin directories found\n"
    );
}

#[cfg(unix)]
#[test]
fn cleanup_reports_failed_removals() {
    use std::os::unix::fs::PermissionsExt;

    let workspace = unique_temp_dir("cleanup-failure");
    let config_path = workspace.join("config").join("tpm.yaml");
    let plugins_dir = workspace.join("plugins");
    let stale_dir = plugins_dir.join("tmux-open");

    write_config(
        &config_path,
        concat!(
            "version: 1\n",
            "paths:\n",
            "  plugins: ../plugins\n",
            "plugins:\n",
            "- source: tmux-plugins/tmux-sensible\n",
        ),
    );
    fs::create_dir_all(plugins_dir.join("tmux-plugins").join("tmux-sensible"))
        .expect("declared plugin directory should exist");
    fs::create_dir_all(&stale_dir).expect("stale plugin directory should exist");

    let original_permissions = fs::metadata(&plugins_dir)
        .expect("plugins dir metadata should exist")
        .permissions();
    let mut readonly_permissions = original_permissions.clone();
    readonly_permissions.set_mode(0o555);
    fs::set_permissions(&plugins_dir, readonly_permissions)
        .expect("plugins dir permissions should be updated");

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "cleanup",
        ],
    );

    fs::set_permissions(&plugins_dir, original_permissions)
        .expect("plugins dir permissions should be restored");

    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8(output.stderr)
            .expect("stderr should be utf-8")
            .contains(&format!(
                "Failed to remove stale plugin directory {}",
                stale_dir.display()
            ))
    );
    assert!(stale_dir.exists());
}

fn write_config(path: &std::path::Path, contents: &str) {
    fs::create_dir_all(path.parent().expect("config path should have a parent"))
        .expect("config directory should be created");
    fs::write(path, contents).expect("config should be writable");
}

use std::fs;

mod support;

use support::{managed_manifest_path, run_tpm, unique_temp_dir, write_managed_manifest};

#[cfg(unix)]
use support::run_tpm_in_pty_with_env;

#[test]
fn cleanup_removes_manifest_managed_undeclared_plugin_directories() {
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
    write_managed_manifest(
        &plugins_dir,
        &[
            ("tmux-open", "tmux-open", "tmux-open"),
            ("zzz", "zzz", "zzz"),
        ],
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
fn cleanup_preserves_unmanaged_undeclared_plugin_directories() {
    let workspace = unique_temp_dir("cleanup-preserve-unmanaged");
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
    fs::create_dir_all(plugins_dir.join("tmux-open")).expect("manual directory should exist");
    fs::create_dir_all(plugins_dir.join("zzz")).expect("manual directory should exist");

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
    assert!(plugins_dir.join("tmux-open").is_dir());
    assert!(plugins_dir.join("zzz").is_dir());
    assert!(
        managed_manifest_path(&plugins_dir).is_file(),
        "cleanup should create the managed manifest when the plugins directory exists"
    );
}

#[test]
fn cleanup_does_not_remove_a_path_still_used_by_a_declared_manifest_entry() {
    let workspace = unique_temp_dir("cleanup-preserve-declared-path");
    let config_path = workspace.join("config").join("tpm.yaml");
    let plugins_dir = workspace.join("plugins");
    let checkout = plugins_dir.join("tmux-plugins").join("tmux-sensible");

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
    fs::create_dir_all(&checkout).expect("declared plugin directory should exist");
    write_managed_manifest(
        &plugins_dir,
        &[
            (
                "tmux-plugins/tmux-sensible",
                "tmux-plugins/tmux-sensible",
                "tmux-plugins/tmux-sensible",
            ),
            (
                "old-alias",
                "tmux-plugins/tmux-sensible",
                "tmux-plugins/tmux-sensible",
            ),
        ],
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
    assert!(
        checkout.is_dir(),
        "cleanup should preserve a path still used by a declared manifest entry"
    );
    let manifest =
        fs::read_to_string(managed_manifest_path(&plugins_dir)).expect("manifest should exist");
    assert!(!manifest.contains("old-alias"));
}

#[test]
fn cleanup_rejects_invalid_manifest_before_removing_stale_directories() {
    let workspace = unique_temp_dir("cleanup-invalid-manifest");
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
    fs::create_dir_all(&stale_dir).expect("stale plugin directory should exist");
    let manifest_path = managed_manifest_path(&plugins_dir);
    fs::create_dir_all(
        manifest_path
            .parent()
            .expect("manifest should have a parent"),
    )
    .expect("manifest directory should exist");
    fs::write(
        &manifest_path,
        concat!(
            "version: 1\n",
            "plugins:\n",
            "  tmux-open:\n",
            "    source: tmux-open\n",
            "    clone_source: tmux-open\n",
            "    path: ../plugins/tmux-open\n",
        ),
    )
    .expect("manifest should be writable");

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "cleanup",
        ],
    );

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        ""
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf-8");
    assert!(stderr.contains("invalid managed plugin manifest"));
    assert!(stderr.contains("path must contain only normal relative components"));
    assert!(
        stale_dir.is_dir(),
        "cleanup should fail closed before removing stale directories"
    );
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
    write_managed_manifest(
        &plugins_dir,
        &[(
            "tmux-plugins/tmux-open",
            "tmux-plugins/tmux-open",
            "tmux-plugins/tmux-open",
        )],
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
    write_managed_manifest(
        &plugins_dir,
        &[
            ("tpm", "tpm", "tmux-plugins/tpm"),
            ("tmux-open", "tmux-open", "tmux-open"),
        ],
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
fn cleanup_colorizes_human_output_in_a_terminal() {
    let workspace = unique_temp_dir("cleanup-color-terminal");
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
    write_managed_manifest(&plugins_dir, &[("tmux-open", "tmux-open", "tmux-open")]);

    let output = run_tpm_in_pty_with_env(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "cleanup",
        ],
        vec![("TERM".to_string(), "xterm-256color".to_string())],
    );

    assert!(
        output.status.success(),
        "cleanup should succeed: {output:?}"
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(stdout.contains("\u{1b}[92mRemoved\u{1b}[0m stale plugin directory "));
    assert!(stdout.contains("\u{1b}[93mPreserved\u{1b}[0m legacy TPM checkout "));
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
    write_managed_manifest(&plugins_dir, &[("tmux-open", "tmux-open", "tmux-open")]);

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

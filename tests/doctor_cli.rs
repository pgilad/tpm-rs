use std::fs;

use serde_json::Value;

mod support;

use support::{
    commit_all, git, init_repo, prepend_path, publish_repo, run_tpm, run_tpm_with_env,
    set_executable, unique_temp_dir, write_file,
};

#[cfg(unix)]
use support::run_tpm_in_pty_with_env;

#[test]
fn doctor_missing_config_prints_getting_started_guide() {
    let workspace = unique_temp_dir("doctor-missing-config");
    let config_path = workspace.join("config").join("tpm.yaml");
    let legacy_plugins_dir = workspace.join("home").join(".tmux").join("plugins");
    let bin_dir = workspace.join("bin");

    fs::create_dir_all(legacy_plugins_dir.join("tmux-sensible"))
        .expect("legacy plugin directory should be created");
    write_fake_tmux(&bin_dir);

    let output = run_tpm_with_env(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "doctor",
        ],
        vec![("PATH".to_string(), prepend_path(&bin_dir))],
    );

    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(stdout.contains(&format!("missing config file at {}", config_path.display())));
    assert!(stdout.contains("Doctor found 1 failing check(s)"));
    assert!(stdout.contains("Getting started:"));
    assert!(stdout.contains(&format!("Expected config path: {}", config_path.display())));
    assert!(stdout.contains("Existing shell TPM setup: run `tpm migrate`"));
    assert!(stdout.contains("Different tmux.conf path: run `tpm migrate --tmux-conf PATH`"));
    assert!(stdout.contains("New setup: run `tpm add tmux-plugins/tmux-sensible`"));
    assert!(stdout.contains("Then add `run-shell \"tpm load\"` to the end of `tmux.conf`"));
    assert!(stdout.contains("Finally run `tpm install`"));
    assert!(!stdout.contains("HINT legacy_plugins_dir"));
    assert_eq!(
        String::from_utf8(output.stderr).expect("stderr should be utf-8"),
        ""
    );
}

#[test]
fn doctor_prints_legacy_plugin_dir_hint_without_failing() {
    let workspace = unique_temp_dir("doctor-legacy-plugin-dir-hint");
    let home_dir = workspace.join("home");
    let config_path = home_dir.join(".config").join("tpm").join("tpm.yaml");
    let legacy_plugins_dir = home_dir.join(".tmux").join("plugins");
    let bin_dir = workspace.join("bin");

    write_config(&config_path, "version: 1\nplugins: []\n");
    fs::create_dir_all(legacy_plugins_dir.join("tmux-sensible"))
        .expect("legacy plugin directory should be created");
    write_fake_tmux(&bin_dir);

    let output = run_tpm_with_env(&workspace, ["doctor"], doctor_env(&bin_dir, &home_dir));

    assert!(output.status.success(), "doctor should succeed: {output:?}");
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(stdout.contains("HINT legacy_plugins_dir"));
    assert!(stdout.contains(
        "legacy TPM plugin directory exists at ~/.tmux/plugins; tpm-rs is using ~/.local/share/tpm/plugins"
    ));
    assert!(stdout.contains(
        "If tmux.conf no longer bootstraps legacy TPM, consider deleting the legacy directory."
    ));
    assert!(stdout.contains("Doctor completed without failing checks"));
    assert_eq!(
        String::from_utf8(output.stderr).expect("stderr should be utf-8"),
        ""
    );
}

#[test]
fn doctor_json_reports_legacy_plugin_dir_hints_without_failing() {
    let workspace = unique_temp_dir("doctor-json-legacy-plugin-dir-hint");
    let home_dir = workspace.join("home");
    let config_path = home_dir.join(".config").join("tpm").join("tpm.yaml");
    let legacy_plugins_dir = home_dir.join(".tmux").join("plugins");
    let bin_dir = workspace.join("bin");

    write_config(&config_path, "version: 1\nplugins: []\n");
    fs::create_dir_all(legacy_plugins_dir.join("tmux-sensible"))
        .expect("legacy plugin directory should be created");
    write_fake_tmux(&bin_dir);

    let output = run_tpm_with_env(
        &workspace,
        ["doctor", "--json"],
        doctor_env(&bin_dir, &home_dir),
    );

    assert!(output.status.success(), "doctor should succeed: {output:?}");
    let report: Value = serde_json::from_slice(&output.stdout).expect("json output should parse");
    let hints = report["hints"]
        .as_array()
        .expect("hints should be an array");

    assert_eq!(report["ok"], true);
    assert_eq!(report["failing_checks"], 0);
    assert_eq!(hints.len(), 1);
    assert_eq!(hints[0]["name"], "legacy_plugins_dir");
    assert!(
        hints[0]["summary"]
            .as_str()
            .expect("summary should be a string")
            .contains(&format!(
                "legacy TPM plugin directory exists at {}; tpm-rs is using {}",
                legacy_plugins_dir.display(),
                home_dir
                    .join(".local")
                    .join("share")
                    .join("tpm")
                    .join("plugins")
                    .display()
            ))
    );
    assert_eq!(
        hints[0]["detail"],
        "If tmux.conf no longer bootstraps legacy TPM, consider deleting the legacy directory."
    );
    assert_eq!(
        String::from_utf8(output.stderr).expect("stderr should be utf-8"),
        ""
    );
}

#[test]
fn doctor_suppresses_legacy_plugin_dir_hint_when_active_plugins_dir_matches() {
    let workspace = unique_temp_dir("doctor-legacy-plugin-dir-active");
    let home_dir = workspace.join("home");
    let config_path = home_dir.join(".config").join("tpm").join("tpm.yaml");
    let legacy_plugins_dir = home_dir.join(".tmux").join("plugins");
    let bin_dir = workspace.join("bin");

    write_config(
        &config_path,
        concat!(
            "version: 1\n",
            "paths:\n",
            "  plugins: ~/.tmux/plugins\n",
            "plugins: []\n",
        ),
    );
    fs::create_dir_all(legacy_plugins_dir.join("tmux-sensible"))
        .expect("legacy plugin directory should be created");
    write_fake_tmux(&bin_dir);

    let output = run_tpm_with_env(&workspace, ["doctor"], doctor_env(&bin_dir, &home_dir));

    assert!(output.status.success(), "doctor should succeed: {output:?}");
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(!stdout.contains("HINT legacy_plugins_dir"));
    assert!(!stdout.contains("legacy TPM plugin directory exists"));
    assert!(stdout.contains("Doctor completed without failing checks"));
    assert_eq!(
        String::from_utf8(output.stderr).expect("stderr should be utf-8"),
        ""
    );
}

#[test]
fn doctor_flags_plugin_checkouts_from_a_different_source() {
    let workspace = unique_temp_dir("doctor-source-mismatch");
    let expected_repo = workspace.join("author").join("tmux-sensible");
    let expected_bare_repo = workspace.join("remotes").join("tmux-sensible.git");
    let unexpected_repo = workspace.join("author").join("tmux-other");
    let unexpected_bare_repo = workspace.join("remotes").join("tmux-other.git");
    let config_path = workspace.join("config").join("tpm.yaml");
    let plugins_dir = workspace.join("plugins");
    let existing_checkout = plugins_dir.join("tmux-sensible");
    let bin_dir = workspace.join("bin");

    init_repo(&expected_repo);
    write_file(&expected_repo.join("plugin.txt"), "expected\n");
    commit_all(&expected_repo, "initial");
    publish_repo(&expected_repo, &expected_bare_repo);

    init_repo(&unexpected_repo);
    write_file(&unexpected_repo.join("plugin.txt"), "unexpected\n");
    commit_all(&unexpected_repo, "initial");
    publish_repo(&unexpected_repo, &unexpected_bare_repo);

    write_config(
        &config_path,
        &format!(
            concat!(
                "version: 1\n",
                "paths:\n",
                "  plugins: ../plugins\n",
                "plugins:\n",
                "- source: {}\n",
            ),
            expected_bare_repo.display()
        ),
    );

    git(
        &workspace,
        vec![
            "clone".to_string(),
            unexpected_bare_repo.display().to_string(),
            existing_checkout.display().to_string(),
        ],
    );
    write_fake_tmux(&bin_dir);

    let output = run_tpm_with_env(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "doctor",
        ],
        vec![("PATH".to_string(), prepend_path(&bin_dir))],
    );

    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(stdout.contains("FAIL plugin/tmux-sensible"));
    assert!(stdout.contains("plugin checkout source does not match configured source"));
    assert!(stdout.contains("Doctor found 1 failing check(s)"));
    assert_eq!(
        String::from_utf8(output.stderr).expect("stderr should be utf-8"),
        ""
    );
}

#[test]
fn doctor_reports_checkout_inspection_failures_without_aborting() {
    let workspace = unique_temp_dir("doctor-broken-checkout");
    let author_repo = workspace.join("author").join("tmux-open");
    let bare_repo = workspace.join("remotes").join("tmux-open.git");
    let config_path = workspace.join("config").join("tpm.yaml");
    let plugins_dir = workspace.join("plugins");
    let bin_dir = workspace.join("bin");

    init_repo(&author_repo);
    write_file(&author_repo.join("plugin.txt"), "initial\n");
    commit_all(&author_repo, "initial");
    publish_repo(&author_repo, &bare_repo);

    write_config(
        &config_path,
        &format!(
            concat!(
                "version: 1\n",
                "paths:\n",
                "  plugins: ../plugins\n",
                "plugins:\n",
                "- source: {}\n",
            ),
            bare_repo.display()
        ),
    );

    let install_output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "install",
        ],
    );
    assert!(
        install_output.status.success(),
        "install should succeed: {install_output:?}"
    );

    let git_index = plugins_dir.join("tmux-open").join(".git").join("index");
    fs::remove_file(&git_index).expect("git index should be removable");
    fs::create_dir(&git_index).expect("git index replacement directory should be created");

    write_fake_tmux(&bin_dir);

    let output = run_tpm_with_env(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "doctor",
        ],
        vec![("PATH".to_string(), prepend_path(&bin_dir))],
    );

    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(stdout.contains("FAIL plugin/tmux-open"));
    assert!(stdout.contains("failed to inspect plugin checkout at"));
    assert!(stdout.contains("Doctor found 1 failing check(s)"));
    assert_eq!(
        String::from_utf8(output.stderr).expect("stderr should be utf-8"),
        ""
    );
}

#[cfg(unix)]
#[test]
fn doctor_colorizes_human_output_in_a_terminal() {
    let workspace = unique_temp_dir("doctor-color-terminal");
    let config_path = workspace.join("config").join("tpm.yaml");
    let bin_dir = workspace.join("bin");

    write_config(&config_path, "version: 1\nplugins: []\n");
    write_fake_tmux(&bin_dir);

    let output = run_tpm_in_pty_with_env(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "doctor",
        ],
        vec![
            ("PATH".to_string(), prepend_path(&bin_dir)),
            ("TERM".to_string(), "xterm-256color".to_string()),
        ],
    );

    assert!(output.status.success(), "doctor should succeed: {output:?}");
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(stdout.contains("\u{1b}[92mPASS\u{1b}[0m config_file"));
    assert!(stdout.contains("\u{1b}[92mDoctor completed without failing checks\u{1b}[0m"));
}

#[cfg(unix)]
#[test]
fn doctor_disables_color_when_no_color_is_set() {
    let workspace = unique_temp_dir("doctor-no-color-terminal");
    let config_path = workspace.join("config").join("tpm.yaml");
    let bin_dir = workspace.join("bin");

    write_config(&config_path, "version: 1\nplugins: []\n");
    write_fake_tmux(&bin_dir);

    let output = run_tpm_in_pty_with_env(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "doctor",
        ],
        vec![
            ("PATH".to_string(), prepend_path(&bin_dir)),
            ("TERM".to_string(), "xterm-256color".to_string()),
            ("NO_COLOR".to_string(), "1".to_string()),
        ],
    );

    assert!(output.status.success(), "doctor should succeed: {output:?}");
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(stdout.contains("PASS config_file"));
    assert!(stdout.contains("Doctor completed without failing checks"));
    assert!(!stdout.contains("\u{1b}[92mPASS\u{1b}[0m"));
    assert!(!stdout.contains("\u{1b}[92mDoctor completed without failing checks\u{1b}[0m"));
}

fn write_fake_tmux(bin_dir: &std::path::Path) {
    let tmux_bin = bin_dir.join("tmux");
    write_file(
        &tmux_bin,
        "#!/bin/sh\nif [ \"${1:-}\" = \"-V\" ]; then\n  printf '%s\\n' 'tmux 3.6a'\n  exit 0\nfi\nexit 1\n",
    );
    set_executable(&tmux_bin);
}

fn doctor_env(bin_dir: &std::path::Path, home_dir: &std::path::Path) -> Vec<(String, String)> {
    vec![
        ("PATH".to_string(), prepend_path(bin_dir)),
        (
            "XDG_CONFIG_HOME".to_string(),
            home_dir.join(".config").display().to_string(),
        ),
        (
            "XDG_DATA_HOME".to_string(),
            home_dir.join(".local").join("share").display().to_string(),
        ),
    ]
}

fn write_config(path: &std::path::Path, contents: &str) {
    fs::create_dir_all(path.parent().expect("config path should have a parent"))
        .expect("config directory should be created");
    fs::write(path, contents).expect("config should be writable");
}

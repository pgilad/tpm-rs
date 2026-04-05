use std::fs;

use serde_json::Value;

mod support;

use support::{run_tpm, unique_temp_dir};

#[test]
fn list_missing_config_suggests_migrate_or_add() {
    let workspace = unique_temp_dir("list-missing-config");
    let config_path = workspace.join("config").join("tpm.yaml");

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "list",
        ],
    );

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        ""
    );
    assert_eq!(
        String::from_utf8(output.stderr).expect("stderr should be utf-8"),
        format!(
            "error: config `{}` does not exist; create it with `tpm migrate` or `tpm add SOURCE`\n",
            config_path.display()
        )
    );
}

#[test]
fn list_reports_non_git_directories_as_missing() {
    let workspace = unique_temp_dir("list-missing-checkout");
    let config_path = workspace.join("config").join("tpm.yaml");
    let install_dir = workspace
        .join("plugins")
        .join("tmux-plugins")
        .join("tmux-sensible");

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
    fs::create_dir_all(&install_dir).expect("placeholder checkout directory should exist");

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "list",
        ],
    );

    assert!(output.status.success(), "list should succeed: {output:?}");
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(stdout.contains("tmux-plugins/tmux-sensible  enabled   missing"));
    assert!(!stdout.contains("tmux-plugins/tmux-sensible  enabled   installed"));
}

#[test]
fn list_json_marks_invalid_checkouts_as_not_installed() {
    let workspace = unique_temp_dir("list-json-invalid");
    let config_path = workspace.join("config").join("tpm.yaml");
    let install_dir = workspace
        .join("plugins")
        .join("tmux-plugins")
        .join("tmux-sensible");

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
    fs::create_dir_all(&install_dir).expect("placeholder checkout directory should exist");

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "list",
            "--json",
        ],
    );

    assert!(output.status.success(), "list should succeed: {output:?}");
    let report: Value = serde_json::from_slice(&output.stdout).expect("json output should parse");
    let items = report.as_array().expect("list report should be an array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["name"], "tmux-plugins/tmux-sensible");
    assert_eq!(items[0]["branch"], Value::Null);
    assert_eq!(items[0]["reference"], Value::Null);
    assert_eq!(items[0]["installed"], false);
    assert_eq!(items[0]["install_dir"], install_dir.display().to_string());
}

#[test]
fn list_json_reports_branch_and_ref_configuration() {
    let workspace = unique_temp_dir("list-json-refs");
    let config_path = workspace.join("config").join("tpm.yaml");

    write_config(
        &config_path,
        concat!(
            "version: 1\n",
            "paths:\n",
            "  plugins: ../plugins\n",
            "plugins:\n",
            "- source: tmux-plugins/tmux-sensible\n",
            "  branch: main\n",
            "- source: catppuccin/tmux\n",
            "  ref: v2.1.3\n",
        ),
    );

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "list",
            "--json",
        ],
    );

    assert!(output.status.success(), "list should succeed: {output:?}");
    let report: Value = serde_json::from_slice(&output.stdout).expect("json output should parse");
    let items = report.as_array().expect("list report should be an array");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["name"], "tmux-plugins/tmux-sensible");
    assert_eq!(items[0]["branch"], "main");
    assert_eq!(items[0]["reference"], Value::Null);
    assert_eq!(items[1]["name"], "catppuccin/tmux");
    assert_eq!(items[1]["branch"], Value::Null);
    assert_eq!(items[1]["reference"], "v2.1.3");
}

#[test]
fn list_rejects_legacy_tpm_plugin_manager_in_config() {
    let workspace = unique_temp_dir("list-legacy-tpm");
    let config_path = workspace.join("config").join("tpm.yaml");

    write_config(
        &config_path,
        concat!(
            "version: 1\n",
            "plugins:\n",
            "- source: https://github.com/tmux-plugins/tpm.git\n",
        ),
    );

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "list",
        ],
    );

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .expect("stderr should be utf-8")
            .contains("the legacy TPM plugin manager is not supported")
    );
}

fn write_config(path: &std::path::Path, contents: &str) {
    fs::create_dir_all(path.parent().expect("config path should have a parent"))
        .expect("config directory should be created");
    fs::write(path, contents).expect("config should be writable");
}

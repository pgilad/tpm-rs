use std::fs;

use serde_json::Value;

mod support;

use support::{run_tpm_with_env, run_tpm_without_home_with_env, unique_temp_dir};

#[test]
fn paths_shortens_home_paths_in_human_output() {
    let workspace = unique_temp_dir("paths-home");
    let home_dir = workspace.join("home");

    let output = run_tpm_with_env(
        &workspace,
        ["paths"],
        vec![
            (
                "XDG_CONFIG_HOME".to_string(),
                home_dir.join(".config").display().to_string(),
            ),
            (
                "XDG_DATA_HOME".to_string(),
                home_dir.join(".local").join("share").display().to_string(),
            ),
            (
                "XDG_STATE_HOME".to_string(),
                home_dir.join(".local").join("state").display().to_string(),
            ),
            (
                "XDG_CACHE_HOME".to_string(),
                home_dir.join(".cache").display().to_string(),
            ),
        ],
    );

    assert!(output.status.success(), "paths should succeed: {output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        concat!(
            "Config file: ~/.config/tpm/tpm.yaml\n",
            "Config dir:  ~/.config/tpm\n",
            "Data dir:    ~/.local/share/tpm\n",
            "State dir:   ~/.local/state/tpm\n",
            "Cache dir:   ~/.cache/tpm\n",
            "Plugins dir: ~/.local/share/tpm/plugins\n",
            "Config:      missing\n",
        )
    );
}

#[test]
fn paths_resolves_without_home_when_all_required_paths_are_overridden() {
    let workspace = unique_temp_dir("paths-no-home");
    let config_path = workspace.join("config").join("tpm.yaml");
    let plugins_dir = workspace.join("plugins");
    let data_dir = workspace.join("data");
    let state_dir = workspace.join("state");
    let cache_dir = workspace.join("cache");

    let output = run_tpm_without_home_with_env(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "--plugins-dir",
            plugins_dir.to_str().expect("plugins dir should be utf-8"),
            "paths",
            "--json",
        ],
        vec![
            ("TPM_DATA_DIR".to_string(), data_dir.display().to_string()),
            ("TPM_STATE_DIR".to_string(), state_dir.display().to_string()),
            ("TPM_CACHE_DIR".to_string(), cache_dir.display().to_string()),
        ],
    );

    assert!(output.status.success(), "paths should succeed: {output:?}");
    let report: Value = serde_json::from_slice(&output.stdout).expect("json output should parse");
    assert_eq!(report["config_file"], config_path.display().to_string());
    assert_eq!(
        report["config_dir"],
        config_path
            .parent()
            .expect("config path should have a parent")
            .display()
            .to_string()
    );
    assert_eq!(report["data_dir"], data_dir.display().to_string());
    assert_eq!(report["state_dir"], state_dir.display().to_string());
    assert_eq!(report["cache_dir"], cache_dir.display().to_string());
    assert_eq!(report["plugins_dir"], plugins_dir.display().to_string());
    assert_eq!(report["config_exists"], false);
}

#[test]
fn add_with_skip_install_works_without_home_when_config_path_is_explicit() {
    let workspace = unique_temp_dir("add-no-home");
    let config_path = workspace.join("config").join("tpm.yaml");

    let output = run_tpm_without_home_with_env(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "add",
            "tmux-plugins/tmux-sensible",
            "--skip-install",
        ],
        std::iter::empty::<(&str, &str)>(),
    );

    assert!(output.status.success(), "add should succeed: {output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        format!(
            "Created {} and added tmux-plugins/tmux-sensible\n",
            config_path.display()
        ),
    );
    assert_eq!(
        fs::read_to_string(&config_path).expect("config should be readable"),
        concat!(
            "version: 1\n",
            "plugins:\n",
            "- source: tmux-plugins/tmux-sensible\n",
        )
    );
}

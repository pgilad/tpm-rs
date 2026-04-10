use std::fs;

mod support;

use support::{run_tpm_with_env, unique_temp_dir, write_file};

#[test]
fn migrate_reads_home_dot_tmux_and_writes_absolute_local_paths() {
    let workspace = unique_temp_dir("migrate-dot-tmux");
    let home_dir = workspace.join("home");
    let tmux_conf_path = home_dir.join(".tmux");
    let sourced_conf_path = home_dir.join(".config").join("tmux").join("extra.conf");
    let config_path = home_dir.join(".config").join("tpm").join("tpm.yaml");
    let local_plugin_path = home_dir.join("plugins").join("tmux-local");
    let tmux_conf = concat!(
        "# comment\n",
        "set -g @plugin 'tmux-plugins/tpm'\n",
        "set -g @plugin 'tmux-plugins/tmux-sensible'\n",
        "set-option -g @plugin 'catppuccin/tmux#stable'\n",
        "set -g @plugin './plugins/tmux-local'\n",
        "source-file ~/.config/tmux/extra.conf\n",
        "run '~/.tmux/plugins/tpm/tpm'\n",
    );

    write_file(&tmux_conf_path, tmux_conf);
    write_file(
        &sourced_conf_path,
        "set -g @plugin 'tmux-plugins/tmux-yank'\n",
    );

    let output = run_tpm_with_env(
        &workspace,
        ["migrate"],
        vec![(
            "XDG_CONFIG_HOME".to_string(),
            home_dir.join(".config").display().to_string(),
        )],
    );

    assert!(
        output.status.success(),
        "migrate should succeed: {output:?}"
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        format!(
            concat!(
                "Detected and parsed 3 plugin(s) correctly\n",
                "Skipped 1 source-file directive(s); multi-file tmux configs are not supported\n",
                "Skipped source-file on line 6: {}\n",
                "Wrote tpm.yaml to {}\n",
                "Did not modify {}\n",
                "You may still need to replace the legacy TPM bootstrap with `run-shell \"tpm load\"` at the end of the file\n",
            ),
            sourced_conf_path.display(),
            config_path.display(),
            tmux_conf_path.display(),
        ),
    );
    assert_eq!(
        String::from_utf8(output.stderr).expect("stderr should be utf-8"),
        ""
    );
    assert_eq!(
        fs::read_to_string(&config_path).expect("config should be readable"),
        format!(
            concat!(
                "version: 1\n",
                "plugins:\n",
                "- source: tmux-plugins/tmux-sensible\n",
                "- source: catppuccin/tmux\n",
                "  branch: stable\n",
                "- source: {}\n",
            ),
            local_plugin_path.display(),
        )
    );
    assert_eq!(
        fs::read_to_string(&tmux_conf_path).expect("tmux config should remain readable"),
        tmux_conf,
    );
}

#[test]
fn migrate_reads_home_dot_tmux_conf() {
    let workspace = unique_temp_dir("migrate-dot-tmux-conf");
    let home_dir = workspace.join("home");
    let tmux_conf_path = home_dir.join(".tmux.conf");
    let config_path = home_dir.join(".config").join("tpm").join("tpm.yaml");

    write_file(
        &tmux_conf_path,
        "set -g @plugin 'tmux-plugins/tmux-resurrect'\n",
    );

    let output = run_tpm_with_env(
        &workspace,
        ["migrate"],
        vec![(
            "XDG_CONFIG_HOME".to_string(),
            home_dir.join(".config").display().to_string(),
        )],
    );

    assert!(
        output.status.success(),
        "migrate should succeed: {output:?}"
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        format!(
            concat!(
                "Detected and parsed 1 plugin(s) correctly\n",
                "Skipped 0 source-file directive(s); multi-file tmux configs are not supported\n",
                "Wrote tpm.yaml to {}\n",
                "Did not modify {}\n",
                "You may still need to replace the legacy TPM bootstrap with `run-shell \"tpm load\"` at the end of the file\n",
            ),
            config_path.display(),
            tmux_conf_path.display(),
        ),
    );
    assert_eq!(
        fs::read_to_string(&config_path).expect("config should be readable"),
        concat!(
            "version: 1\n",
            "plugins:\n",
            "- source: tmux-plugins/tmux-resurrect\n",
        )
    );
}

#[test]
fn migrate_treats_v_prefixed_numeric_fragment_as_ref() {
    let workspace = unique_temp_dir("migrate-v-ref");
    let home_dir = workspace.join("home");
    let tmux_conf_path = home_dir.join(".tmux");
    let config_path = home_dir.join(".config").join("tpm").join("tpm.yaml");

    write_file(&tmux_conf_path, "set -g @plugin 'catppuccin/tmux#v0.3.6'\n");

    let output = run_tpm_with_env(
        &workspace,
        ["migrate"],
        vec![(
            "XDG_CONFIG_HOME".to_string(),
            home_dir.join(".config").display().to_string(),
        )],
    );

    assert!(
        output.status.success(),
        "migrate should succeed: {output:?}"
    );
    assert_eq!(
        fs::read_to_string(&config_path).expect("config should be readable"),
        concat!(
            "version: 1\n",
            "plugins:\n",
            "- source: catppuccin/tmux\n",
            "  ref: v0.3.6\n",
        )
    );
}

#[test]
fn migrate_treats_v_prefixed_non_numeric_fragment_as_branch() {
    let workspace = unique_temp_dir("migrate-v-branch");
    let home_dir = workspace.join("home");
    let tmux_conf_path = home_dir.join(".tmux");
    let config_path = home_dir.join(".config").join("tpm").join("tpm.yaml");

    write_file(&tmux_conf_path, "set -g @plugin 'catppuccin/tmux#vnext'\n");

    let output = run_tpm_with_env(
        &workspace,
        ["migrate"],
        vec![(
            "XDG_CONFIG_HOME".to_string(),
            home_dir.join(".config").display().to_string(),
        )],
    );

    assert!(
        output.status.success(),
        "migrate should succeed: {output:?}"
    );
    assert_eq!(
        fs::read_to_string(&config_path).expect("config should be readable"),
        concat!(
            "version: 1\n",
            "plugins:\n",
            "- source: catppuccin/tmux\n",
            "  branch: vnext\n",
        )
    );
}

#[test]
fn migrate_falls_back_to_xdg_tmux_conf() {
    let workspace = unique_temp_dir("migrate-xdg");
    let home_dir = workspace.join("home");
    let tmux_conf_path = home_dir.join(".config").join("tmux").join("tmux.conf");
    let config_path = home_dir.join(".config").join("tpm").join("tpm.yaml");

    write_file(
        &tmux_conf_path,
        "set -g @plugin 'tmux-plugins/tmux-resurrect'\n",
    );

    let output = run_tpm_with_env(
        &workspace,
        ["migrate"],
        vec![(
            "XDG_CONFIG_HOME".to_string(),
            home_dir.join(".config").display().to_string(),
        )],
    );

    assert!(
        output.status.success(),
        "migrate should succeed: {output:?}"
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        format!(
            concat!(
                "Detected and parsed 1 plugin(s) correctly\n",
                "Skipped 0 source-file directive(s); multi-file tmux configs are not supported\n",
                "Wrote tpm.yaml to {}\n",
                "Did not modify {}\n",
                "You may still need to replace the legacy TPM bootstrap with `run-shell \"tpm load\"` at the end of the file\n",
            ),
            config_path.display(),
            tmux_conf_path.display(),
        ),
    );
    assert_eq!(
        fs::read_to_string(&config_path).expect("config should be readable"),
        concat!(
            "version: 1\n",
            "plugins:\n",
            "- source: tmux-plugins/tmux-resurrect\n",
        )
    );
}

#[test]
fn migrate_refuses_to_overwrite_existing_tpm_yaml() {
    let workspace = unique_temp_dir("migrate-existing-config");
    let home_dir = workspace.join("home");
    let tmux_conf_path = home_dir.join(".tmux");
    let config_path = home_dir.join(".config").join("tpm").join("tpm.yaml");
    let existing_config = concat!(
        "version: 1\n",
        "plugins:\n",
        "- source: tmux-plugins/tmux-yank\n",
    );

    write_file(
        &tmux_conf_path,
        "set -g @plugin 'tmux-plugins/tmux-sensible'\n",
    );
    write_file(&config_path, existing_config);

    let output = run_tpm_with_env(
        &workspace,
        ["migrate"],
        vec![(
            "XDG_CONFIG_HOME".to_string(),
            home_dir.join(".config").display().to_string(),
        )],
    );

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        ""
    );
    assert!(
        String::from_utf8(output.stderr)
            .expect("stderr should be utf-8")
            .contains(&format!(
                "error: config already exists at `{}`; refusing to overwrite",
                config_path.display()
            ))
    );
    assert_eq!(
        fs::read_to_string(&config_path).expect("config should remain readable"),
        existing_config
    );
}

#[test]
fn migrate_reports_skipped_source_files_when_root_config_has_no_direct_plugins() {
    let workspace = unique_temp_dir("migrate-source-file-only");
    let home_dir = workspace.join("home");
    let tmux_conf_path = home_dir.join(".tmux.conf");
    let sourced_conf_path = home_dir.join(".config").join("tmux").join("plugins.conf");

    write_file(&tmux_conf_path, "source-file ~/.config/tmux/plugins.conf\n");
    write_file(
        &sourced_conf_path,
        "set -g @plugin 'tmux-plugins/tmux-sensible'\n",
    );

    let output = run_tpm_with_env(
        &workspace,
        ["migrate"],
        vec![(
            "XDG_CONFIG_HOME".to_string(),
            home_dir.join(".config").display().to_string(),
        )],
    );

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        ""
    );
    assert_eq!(
        String::from_utf8(output.stderr).expect("stderr should be utf-8"),
        format!(
            concat!(
                "error: no migratable tmux plugins were detected in `{}`\n",
                "Skipped 1 source-file directive(s); multi-file tmux configs are not supported\n",
                "Skipped source-file on line 1: {}\n",
                "Run `tpm migrate --tmux-conf PATH` against a tmux config file that directly contains `@plugin` lines, or merge sourced plugin declarations into one file first\n",
            ),
            tmux_conf_path.display(),
            sourced_conf_path.display(),
        )
    );
}

#[test]
fn migrate_fails_loudly_on_inline_command_sequences_with_plugin_definitions() {
    let workspace = unique_temp_dir("migrate-inline-plugin-sequence");
    let home_dir = workspace.join("home");
    let tmux_conf_path = home_dir.join(".tmux.conf");

    write_file(
        &tmux_conf_path,
        "set -g @plugin 'tmux-plugins/tmux-sensible' \\; set -g @plugin 'tmux-plugins/tmux-yank'\n",
    );

    let output = run_tpm_with_env(
        &workspace,
        ["migrate"],
        vec![(
            "XDG_CONFIG_HOME".to_string(),
            home_dir.join(".config").display().to_string(),
        )],
    );

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        ""
    );
    assert_eq!(
        String::from_utf8(output.stderr).expect("stderr should be utf-8"),
        format!(
            concat!(
                "error: tmux config `{}` line 1: ",
                "inline tmux command separators (`;`) are not supported during migration; ",
                "move each `@plugin` or `source-file` command onto its own line\n",
            ),
            tmux_conf_path.display(),
        )
    );
}

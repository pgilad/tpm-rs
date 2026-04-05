#![cfg(unix)]

use std::{fs, path::Path, path::PathBuf};

use sha2::{Digest, Sha256};

mod support;

use support::{
    commit_all, init_repo, prepend_path, publish_repo, run_tpm, run_tpm_with_env, set_executable,
    unique_temp_dir, write_file,
};

#[test]
fn load_runs_sorted_root_entrypoints_for_enabled_plugins() {
    let workspace = unique_temp_dir("load-success");
    let config_path = workspace.join("config").join("tpm.yaml");
    let plugins_dir = workspace.join("plugins");
    let load_log = workspace.join("load.log");
    let manager_log = workspace.join("manager.log");

    let first_repo = create_plugin_repo(
        &workspace,
        "tmux-first",
        &[(
            "main.tmux",
            "#!/bin/sh\nprintf '%s\\n' \"$(basename \"$0\")\" >> \"$TPM_LOAD_LOG\"\n",
            true,
        )],
    );
    let second_repo = create_plugin_repo(
        &workspace,
        "tmux-second",
        &[
            (
                "b.tmux",
                "#!/bin/sh\nprintf '%s\\n' \"$(basename \"$0\")\" >> \"$TPM_LOAD_LOG\"\n",
                true,
            ),
            (
                "a.tmux",
                "#!/bin/sh\nprintf '%s\\n' \"$(basename \"$0\")\" >> \"$TPM_LOAD_LOG\"\nprintf '%s\\n' \"$TMUX_PLUGIN_MANAGER_PATH\" >> \"$TPM_MANAGER_PATH_LOG\"\n",
                true,
            ),
            ("ignored.tmux", "#!/bin/sh\nexit 99\n", false),
            (
                "nested/ignored.tmux",
                "#!/bin/sh\nprintf 'nested\\n' >> \"$TPM_LOAD_LOG\"\n",
                true,
            ),
        ],
    );

    write_config(
        &config_path,
        &["../remotes/tmux-first.git", "../remotes/tmux-second.git"],
        Some("../plugins"),
        Some("- source: ../remotes/tmux-disabled.git\n  enabled: false\n"),
    );
    let disabled_repo = create_plugin_repo(
        &workspace,
        "tmux-disabled",
        &[("disabled.tmux", "#!/bin/sh\nexit 88\n", true)],
    );

    assert!(first_repo.exists());
    assert!(second_repo.exists());
    assert!(disabled_repo.exists());

    let install = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "install",
        ],
    );
    assert!(
        install.status.success(),
        "install should succeed: {install:?}"
    );

    let output = run_tpm_with_env(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "load",
        ],
        vec![
            (
                "TPM_LOAD_LOG".to_string(),
                load_log
                    .to_str()
                    .expect("log path should be utf-8")
                    .to_string(),
            ),
            (
                "TPM_MANAGER_PATH_LOG".to_string(),
                manager_log
                    .to_str()
                    .expect("manager log path should be utf-8")
                    .to_string(),
            ),
        ],
    );

    assert!(output.status.success(), "load should succeed: {output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        ""
    );
    assert_eq!(
        String::from_utf8(output.stderr).expect("stderr should be utf-8"),
        ""
    );
    assert_eq!(
        fs::read_to_string(&load_log).expect("load log should be readable"),
        "main.tmux\na.tmux\nb.tmux\n"
    );
    assert_eq!(
        fs::read_to_string(&manager_log).expect("manager log should be readable"),
        format!("{}/\n", plugins_dir.display())
    );
}

#[test]
fn load_reports_missing_plugin_checkouts() {
    let workspace = unique_temp_dir("load-missing");
    let config_path = workspace.join("config").join("tpm.yaml");
    let plugins_dir = workspace.join("plugins");

    write_config(
        &config_path,
        &["../remotes/tmux-sensible.git"],
        Some("../plugins"),
        None,
    );

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "load",
        ],
    );

    assert!(!output.status.success(), "load should fail: {output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        ""
    );

    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf-8");
    assert!(stderr.contains(&format!(
        "Failed to load tmux-sensible: plugin checkout is missing at {}",
        plugins_dir.join("tmux-sensible").display()
    )));
    assert!(stderr.contains("error: load reported 1 failed operations"));
}

#[test]
fn load_continues_after_a_plugin_entrypoint_failure() {
    let workspace = unique_temp_dir("load-partial-failure");
    let config_path = workspace.join("config").join("tpm.yaml");
    let load_log = workspace.join("load.log");

    let failing_repo = create_plugin_repo(
        &workspace,
        "tmux-fail",
        &[("fail.tmux", "#!/bin/sh\necho boom >&2\nexit 7\n", true)],
    );
    let succeeding_repo = create_plugin_repo(
        &workspace,
        "tmux-ok",
        &[(
            "ok.tmux",
            "#!/bin/sh\nprintf '%s\\n' \"$(basename \"$0\")\" >> \"$TPM_LOAD_LOG\"\n",
            true,
        )],
    );

    write_config(
        &config_path,
        &["../remotes/tmux-fail.git", "../remotes/tmux-ok.git"],
        Some("../plugins"),
        None,
    );

    assert!(failing_repo.exists());
    assert!(succeeding_repo.exists());

    let install = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "install",
        ],
    );
    assert!(
        install.status.success(),
        "install should succeed: {install:?}"
    );

    let output = run_tpm_with_env(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "load",
        ],
        vec![(
            "TPM_LOAD_LOG".to_string(),
            load_log
                .to_str()
                .expect("log path should be utf-8")
                .to_string(),
        )],
    );

    assert!(!output.status.success(), "load should fail: {output:?}");
    assert_eq!(
        fs::read_to_string(&load_log).expect("load log should be readable"),
        "ok.tmux\n"
    );

    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf-8");
    assert!(stderr.contains("Failed to load tmux-fail: entrypoint"));
    assert!(stderr.contains("fail.tmux"));
    assert!(stderr.contains("boom"));
    assert!(stderr.contains("error: load reported 1 failed operations"));
}

#[test]
fn load_ignores_stale_tmux_environment_when_plugins_load_successfully() {
    let workspace = unique_temp_dir("load-stale-tmux");
    let config_path = workspace.join("config").join("tpm.yaml");
    let bin_dir = workspace.join("bin");
    let tmux_bin = bin_dir.join("tmux");
    let load_log = workspace.join("load.log");

    let plugin_repo = create_plugin_repo(
        &workspace,
        "tmux-stale",
        &[(
            "load.tmux",
            "#!/bin/sh\nprintf '%s\\n' \"$(basename \"$0\")\" >> \"$TPM_LOAD_LOG\"\n",
            true,
        )],
    );

    write_file(
        &tmux_bin,
        "#!/bin/sh\nprintf 'error connecting to %s\\n' \"$TMUX\" >&2\nexit 1\n",
    );
    set_executable(&tmux_bin);

    write_config(
        &config_path,
        &["../remotes/tmux-stale.git"],
        Some("../plugins"),
        None,
    );

    assert!(plugin_repo.exists());

    let install = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "install",
        ],
    );
    assert!(
        install.status.success(),
        "install should succeed: {install:?}"
    );

    let output = run_tpm_with_env(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "load",
        ],
        vec![
            ("PATH".to_string(), prepend_path(&bin_dir)),
            ("TMUX".to_string(), "/tmp/stale-tmux-socket".to_string()),
            (
                "TPM_LOAD_LOG".to_string(),
                load_log
                    .to_str()
                    .expect("log path should be utf-8")
                    .to_string(),
            ),
        ],
    );

    assert!(output.status.success(), "load should succeed: {output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        ""
    );
    assert_eq!(
        String::from_utf8(output.stderr).expect("stderr should be utf-8"),
        ""
    );
    assert_eq!(
        fs::read_to_string(&load_log).expect("load log should be readable"),
        "load.tmux\n"
    );
}

#[test]
fn load_reports_failures_back_into_tmux_when_available() {
    let workspace = unique_temp_dir("load-tmux");
    let config_path = workspace.join("config").join("tpm.yaml");
    let plugins_dir = workspace.join("plugins");
    let tmux_log = workspace.join("tmux.log");
    let bin_dir = workspace.join("bin");
    let tmux_bin = bin_dir.join("tmux");

    write_file(
        &tmux_bin,
        "#!/bin/sh\n{\n  printf '['\n  for arg in \"$@\"; do\n    printf '%s|' \"$arg\"\n  done\n  printf ']\\n'\n} >> \"$TPM_TMUX_LOG\"\n",
    );
    set_executable(&tmux_bin);

    write_config(
        &config_path,
        &["../remotes/tmux-sensible.git"],
        Some("../plugins"),
        None,
    );

    let output = run_tpm_with_env(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "load",
        ],
        vec![
            ("PATH".to_string(), prepend_path(&bin_dir)),
            ("TMUX".to_string(), "/tmp/fake,123,0".to_string()),
            (
                "TPM_TMUX_LOG".to_string(),
                tmux_log
                    .to_str()
                    .expect("tmux log path should be utf-8")
                    .to_string(),
            ),
        ],
    );

    assert!(!output.status.success(), "load should fail: {output:?}");

    let tmux_commands = fs::read_to_string(&tmux_log).expect("tmux log should be readable");
    assert!(tmux_commands.contains("set-environment|-g|TMUX_PLUGIN_MANAGER_PATH|"));
    assert!(tmux_commands.contains(&format!("{}/|", plugins_dir.display())));
    assert!(tmux_commands.contains("display-message|[tpm] Failed to load tmux-sensible:"));
}

#[test]
fn load_writes_per_server_log_and_overwrites_it_on_next_run() {
    let workspace = unique_temp_dir("load-server-log");
    let config_path = workspace.join("config").join("tpm.yaml");
    let state_dir = workspace.join("state");
    let bin_dir = workspace.join("bin");
    let tmux_bin = bin_dir.join("tmux");
    let server_socket = "/tmp/tpm-rs-load-log";

    let plugin_repo = create_plugin_repo(
        &workspace,
        "tmux-loggable",
        &[
            ("b.tmux", "#!/bin/sh\nexit 0\n", true),
            ("a.tmux", "#!/bin/sh\nexit 0\n", true),
            ("ignored.tmux", "#!/bin/sh\nexit 99\n", false),
            ("nested/ignored.tmux", "#!/bin/sh\nexit 88\n", true),
        ],
    );

    write_file(&tmux_bin, "#!/bin/sh\nexit 0\n");
    set_executable(&tmux_bin);

    write_config(
        &config_path,
        &["../remotes/tmux-loggable.git"],
        Some("../plugins"),
        None,
    );

    assert!(plugin_repo.exists());

    let install = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "install",
        ],
    );
    assert!(
        install.status.success(),
        "install should succeed: {install:?}"
    );

    let env = vec![
        ("PATH".to_string(), prepend_path(&bin_dir)),
        ("TMUX".to_string(), format!("{server_socket},123,0")),
        ("TPM_STATE_DIR".to_string(), state_dir.display().to_string()),
    ];
    let output = run_tpm_with_env(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "load",
        ],
        env.clone(),
    );

    assert!(output.status.success(), "load should succeed: {output:?}");

    let log_path = load_server_log_path(&state_dir, server_socket);
    let contents = fs::read_to_string(&log_path).expect("server log should be readable");
    assert!(contents.contains("tmux server socket: /tmp/tpm-rs-load-log"));
    assert!(contents.contains("plugin tmux-loggable: loading from"));
    assert!(contents.contains("plugin tmux-loggable: discovered entrypoint"));
    assert!(contents.contains("a.tmux"));
    assert!(contents.contains("b.tmux"));
    assert!(contents.contains("plugin tmux-loggable: entrypoint succeeded"));
    assert!(contents.contains("plugin tmux-loggable: loaded successfully in "));
    assert!(contents.contains("load completed successfully in "));
    assert!(!contents.contains("nested/ignored.tmux"));
    assert!(contents.contains("ms"));

    fs::write(&log_path, "sentinel\n").expect("server log should be writable");

    let rerun = run_tpm_with_env(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "load",
        ],
        env,
    );
    assert!(rerun.status.success(), "load should succeed: {rerun:?}");

    let overwritten = fs::read_to_string(&log_path).expect("server log should be readable");
    assert!(!overwritten.contains("sentinel"));
    assert!(overwritten.contains("load completed successfully in "));
}

#[test]
fn load_does_not_write_a_server_log_outside_tmux() {
    let workspace = unique_temp_dir("load-no-server-log");
    let config_path = workspace.join("config").join("tpm.yaml");
    let state_dir = workspace.join("state");

    let plugin_repo = create_plugin_repo(
        &workspace,
        "tmux-no-log",
        &[("load.tmux", "#!/bin/sh\nexit 0\n", true)],
    );

    write_config(
        &config_path,
        &["../remotes/tmux-no-log.git"],
        Some("../plugins"),
        None,
    );

    assert!(plugin_repo.exists());

    let install = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "install",
        ],
    );
    assert!(
        install.status.success(),
        "install should succeed: {install:?}"
    );

    fs::create_dir_all(&state_dir).expect("state dir should be created");
    let output = run_tpm_with_env(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "load",
        ],
        vec![("TPM_STATE_DIR".to_string(), state_dir.display().to_string())],
    );

    assert!(output.status.success(), "load should succeed: {output:?}");

    let load_logs = fs::read_dir(&state_dir)
        .expect("state dir should be readable")
        .map(|entry| entry.expect("state dir entry should be readable").path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("load-") && name.ends_with(".log"))
        })
        .collect::<Vec<_>>();
    assert!(load_logs.is_empty(), "no server log should be written");
}

#[test]
fn load_writes_failure_details_to_the_server_log() {
    let workspace = unique_temp_dir("load-server-log-failure");
    let config_path = workspace.join("config").join("tpm.yaml");
    let state_dir = workspace.join("state");
    let bin_dir = workspace.join("bin");
    let tmux_bin = bin_dir.join("tmux");
    let server_socket = "/tmp/tpm-rs-load-log-failure";

    let failing_repo = create_plugin_repo(
        &workspace,
        "tmux-fail-log",
        &[("fail.tmux", "#!/bin/sh\necho boom >&2\nexit 7\n", true)],
    );

    write_file(&tmux_bin, "#!/bin/sh\nexit 0\n");
    set_executable(&tmux_bin);

    write_config(
        &config_path,
        &["../remotes/tmux-fail-log.git"],
        Some("../plugins"),
        None,
    );

    assert!(failing_repo.exists());

    let install = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "install",
        ],
    );
    assert!(
        install.status.success(),
        "install should succeed: {install:?}"
    );

    let output = run_tpm_with_env(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "load",
        ],
        vec![
            ("PATH".to_string(), prepend_path(&bin_dir)),
            ("TMUX".to_string(), format!("{server_socket},456,0")),
            ("TPM_STATE_DIR".to_string(), state_dir.display().to_string()),
        ],
    );

    assert!(!output.status.success(), "load should fail: {output:?}");

    let log_path = load_server_log_path(&state_dir, server_socket);
    let contents = fs::read_to_string(&log_path).expect("server log should be readable");
    assert!(contents.contains("plugin tmux-fail-log: running entrypoint"));
    assert!(contents.contains("fail.tmux"));
    assert!(contents.contains("plugin tmux-fail-log: entrypoint failed"));
    assert!(contents.contains("plugin tmux-fail-log: failed in "));
    assert!(contents.contains("boom"));
    assert!(contents.contains("load completed with 1 failed operations in "));
}

fn create_plugin_repo(workspace: &Path, name: &str, files: &[(&str, &str, bool)]) -> PathBuf {
    let author_repo = workspace.join("author").join(name);
    let bare_repo = workspace.join("remotes").join(format!("{name}.git"));

    init_repo(&author_repo);
    for (relative_path, contents, executable) in files {
        let path = author_repo.join(relative_path);
        write_file(&path, contents);
        if *executable {
            set_executable(&path);
        }
    }

    commit_all(&author_repo, "initial");
    publish_repo(&author_repo, &bare_repo);
    bare_repo
}

fn write_config(
    path: &Path,
    sources: &[&str],
    plugins_dir: Option<&str>,
    extra_plugin_block: Option<&str>,
) {
    let mut contents = String::from("version: 1\n");
    if let Some(plugins_dir) = plugins_dir {
        contents.push_str("paths:\n");
        contents.push_str(&format!("  plugins: {plugins_dir}\n"));
    }
    contents.push_str("plugins:\n");
    for source in sources {
        contents.push_str(&format!("- source: {source}\n"));
    }
    if let Some(extra_plugin_block) = extra_plugin_block {
        contents.push_str(extra_plugin_block);
    }

    write_file(path, &contents);
}

fn load_server_log_path(state_dir: &Path, server_socket: &str) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(server_socket.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    state_dir.join(format!("load-{hash}.log"))
}

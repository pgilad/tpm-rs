use std::fs;

mod support;

use support::{
    commit_all, git, init_repo, publish_repo, run_git, run_tpm, unique_temp_dir, write_file,
};

#[test]
fn install_missing_config_suggests_migrate_or_add() {
    let workspace = unique_temp_dir("install-missing-config");
    let config_path = workspace.join("config").join("tpm.yaml");

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "install",
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
fn install_clones_configured_plugins_and_skips_existing_checkouts() {
    let workspace = unique_temp_dir("install-configured");
    let author_repo = workspace.join("author").join("tmux-sensible");
    let bare_repo = workspace.join("remotes").join("tmux-sensible.git");
    let config_path = workspace.join("config").join("tpm.yaml");
    let plugins_dir = workspace.join("plugins");

    init_repo(&author_repo);
    write_file(&author_repo.join("plugin.txt"), "v1\n");
    commit_all(&author_repo, "initial");
    publish_repo(&author_repo, &bare_repo);

    fs::create_dir_all(
        config_path
            .parent()
            .expect("config path should have a parent directory"),
    )
    .expect("config directory should exist");
    fs::write(
        &config_path,
        concat!(
            "version: 1\n",
            "paths:\n",
            "  plugins: ../plugins\n",
            "plugins:\n",
            "- source: ../remotes/tmux-sensible.git\n",
        ),
    )
    .expect("config should be writable");

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "install",
        ],
    );

    assert!(
        output.status.success(),
        "install should succeed: {output:?}"
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        format!(
            "Installed tmux-sensible into {}\n",
            plugins_dir.join("tmux-sensible").display()
        )
    );
    assert_eq!(
        fs::read_to_string(plugins_dir.join("tmux-sensible").join("plugin.txt"))
            .expect("installed file should be readable"),
        "v1\n"
    );

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "install",
        ],
    );

    assert!(
        output.status.success(),
        "second install should succeed: {output:?}"
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        format!(
            "Skipped already installed tmux-sensible at {}\n",
            plugins_dir.join("tmux-sensible").display()
        )
    );
}

#[test]
fn install_checks_out_configured_branch() {
    let workspace = unique_temp_dir("install-branch");
    let author_repo = workspace.join("author").join("tmux-sensible");
    let bare_repo = workspace.join("remotes").join("tmux-sensible.git");
    let config_path = workspace.join("config").join("tpm.yaml");
    let plugins_dir = workspace.join("plugins");

    init_repo(&author_repo);
    write_file(&author_repo.join("plugin.txt"), "main\n");
    commit_all(&author_repo, "initial");
    git(&author_repo, ["checkout", "-b", "stable"]);
    write_file(&author_repo.join("plugin.txt"), "stable\n");
    commit_all(&author_repo, "stable");
    git(&author_repo, ["checkout", "main"]);
    publish_repo(&author_repo, &bare_repo);

    fs::create_dir_all(
        config_path
            .parent()
            .expect("config path should have a parent directory"),
    )
    .expect("config directory should exist");
    fs::write(
        &config_path,
        format!(
            concat!(
                "version: 1\n",
                "paths:\n",
                "  plugins: ../plugins\n",
                "plugins:\n",
                "- source: {}\n",
                "  branch: stable\n",
            ),
            bare_repo.display(),
        ),
    )
    .expect("config should be writable");

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "install",
        ],
    );

    assert!(
        output.status.success(),
        "install should succeed: {output:?}"
    );
    assert_eq!(
        fs::read_to_string(plugins_dir.join("tmux-sensible").join("plugin.txt"))
            .expect("installed file should be readable"),
        "stable\n"
    );
    let branch = String::from_utf8(
        run_git(
            &plugins_dir.join("tmux-sensible"),
            ["branch", "--show-current"],
        )
        .stdout,
    )
    .expect("branch output should be utf-8");
    assert_eq!(branch.trim(), "stable");
}

#[test]
fn install_rejects_ref_that_names_remote_branch() {
    let workspace = unique_temp_dir("install-ref-branch");
    let author_repo = workspace.join("author").join("tmux-sensible");
    let bare_repo = workspace.join("remotes").join("tmux-sensible.git");
    let config_path = workspace.join("config").join("tpm.yaml");
    let plugins_dir = workspace.join("plugins");

    init_repo(&author_repo);
    write_file(&author_repo.join("plugin.txt"), "main\n");
    commit_all(&author_repo, "initial");
    publish_repo(&author_repo, &bare_repo);

    fs::create_dir_all(
        config_path
            .parent()
            .expect("config path should have a parent directory"),
    )
    .expect("config directory should exist");
    fs::write(
        &config_path,
        format!(
            concat!(
                "version: 1\n",
                "paths:\n",
                "  plugins: ../plugins\n",
                "plugins:\n",
                "- source: {}\n",
                "  ref: main\n",
            ),
            bare_repo.display(),
        ),
    )
    .expect("config should be writable");

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "install",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf-8");
    assert!(stderr.contains("Failed to install tmux-sensible"));
    assert!(stderr.contains("configured ref `main` names a remote branch"));
    assert!(!plugins_dir.join("tmux-sensible").exists());
}

#[test]
fn install_rejects_explicit_plugin_sources() {
    let workspace = unique_temp_dir("install-explicit");

    let output = run_tpm(&workspace, ["install", "./remotes/tmux-open.git"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .expect("stderr should be utf-8")
            .contains("unexpected argument './remotes/tmux-open.git' found")
    );
}

#[test]
fn install_preserves_plugin_output_order_for_mixed_outcomes() {
    let workspace = unique_temp_dir("install-order");
    let first_repo = workspace.join("author").join("tmux-first");
    let first_bare_repo = workspace.join("remotes").join("tmux-first.git");
    let second_repo = workspace.join("author").join("tmux-second");
    let second_bare_repo = workspace.join("remotes").join("tmux-second.git");
    let config_path = workspace.join("config").join("tpm.yaml");
    let plugins_dir = workspace.join("plugins");
    let first_checkout = plugins_dir.join("tmux-first");

    init_repo(&first_repo);
    write_file(&first_repo.join("plugin.txt"), "first\n");
    commit_all(&first_repo, "initial");
    publish_repo(&first_repo, &first_bare_repo);

    init_repo(&second_repo);
    write_file(&second_repo.join("plugin.txt"), "second\n");
    commit_all(&second_repo, "initial");
    publish_repo(&second_repo, &second_bare_repo);

    fs::create_dir_all(
        config_path
            .parent()
            .expect("config path should have a parent directory"),
    )
    .expect("config directory should exist");
    fs::write(
        &config_path,
        format!(
            concat!(
                "version: 1\n",
                "paths:\n",
                "  plugins: ../plugins\n",
                "plugins:\n",
                "- source: {}\n",
                "- source: {}\n",
            ),
            first_bare_repo.display(),
            second_bare_repo.display(),
        ),
    )
    .expect("config should be writable");

    fs::create_dir_all(&plugins_dir).expect("plugins directory should exist");
    git(
        &workspace,
        vec![
            "clone".to_string(),
            first_bare_repo.display().to_string(),
            first_checkout.display().to_string(),
        ],
    );

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "install",
        ],
    );

    assert!(
        output.status.success(),
        "install should succeed: {output:?}"
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        format!(
            concat!(
                "Skipped already installed tmux-first at {}\n",
                "Installed tmux-second into {}\n",
            ),
            first_checkout.display(),
            plugins_dir.join("tmux-second").display(),
        )
    );
}

#[test]
fn install_rejects_existing_checkouts_from_a_different_source() {
    let workspace = unique_temp_dir("install-mismatch");
    let expected_repo = workspace.join("author").join("tmux-sensible");
    let expected_bare_repo = workspace.join("remotes").join("tmux-sensible.git");
    let unexpected_repo = workspace.join("author").join("tmux-other");
    let unexpected_bare_repo = workspace.join("remotes").join("tmux-other.git");
    let config_path = workspace.join("config").join("tpm.yaml");
    let plugins_dir = workspace.join("plugins");
    let existing_checkout = plugins_dir.join("tmux-sensible");

    init_repo(&expected_repo);
    write_file(&expected_repo.join("plugin.txt"), "expected\n");
    commit_all(&expected_repo, "initial");
    publish_repo(&expected_repo, &expected_bare_repo);

    init_repo(&unexpected_repo);
    write_file(&unexpected_repo.join("plugin.txt"), "unexpected\n");
    commit_all(&unexpected_repo, "initial");
    publish_repo(&unexpected_repo, &unexpected_bare_repo);

    fs::create_dir_all(
        config_path
            .parent()
            .expect("config path should have a parent directory"),
    )
    .expect("config directory should exist");
    fs::write(
        &config_path,
        format!(
            concat!(
                "version: 1\n",
                "paths:\n",
                "  plugins: ../plugins\n",
                "plugins:\n",
                "- source: {}\n",
            ),
            expected_bare_repo.display()
        ),
    )
    .expect("config should be writable");

    git(
        &workspace,
        vec![
            "clone".to_string(),
            unexpected_bare_repo.display().to_string(),
            existing_checkout.display().to_string(),
        ],
    );

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "install",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf-8");
    assert!(stderr.contains("Failed to install tmux-sensible"));
    assert!(stderr.contains("plugin checkout source does not match configured source"));
    assert!(stderr.contains(&expected_bare_repo.display().to_string()));
    assert_eq!(
        fs::read_to_string(existing_checkout.join("plugin.txt"))
            .expect("existing checkout should remain readable"),
        "unexpected\n"
    );
}

use std::fs;

mod support;

use support::{
    commit_all, git, init_repo, publish_repo, run_git, run_tpm, unique_temp_dir, write_file,
};

#[cfg(unix)]
use support::{normalize_terminal_output, run_tpm_in_pty_with_env};

#[test]
fn update_fast_forwards_default_branch_plugins_and_reports_when_current() {
    let workspace = unique_temp_dir("update-branch");
    let author_repo = workspace.join("author").join("tmux-sensible");
    let bare_repo = workspace.join("remotes").join("tmux-sensible.git");
    let config_path = workspace.join("config").join("tpm.yaml");
    let plugins_dir = workspace.join("plugins");

    init_repo(&author_repo);
    write_file(&author_repo.join("plugin.txt"), "v1\n");
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

    write_file(&author_repo.join("plugin.txt"), "v2\n");
    commit_all(&author_repo, "second");
    git(&author_repo, ["push", "origin", "main"]);

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "update",
        ],
    );

    assert!(output.status.success(), "update should succeed: {output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        format!(
            "Updated tmux-sensible in {}\n",
            plugins_dir.join("tmux-sensible").display()
        )
    );
    assert_eq!(
        fs::read_to_string(plugins_dir.join("tmux-sensible").join("plugin.txt"))
            .expect("updated file should be readable"),
        "v2\n"
    );

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "update",
        ],
    );

    assert!(
        output.status.success(),
        "second update should succeed: {output:?}"
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        format!(
            "Already up to date tmux-sensible at {}\n",
            plugins_dir.join("tmux-sensible").display()
        )
    );
}

#[test]
fn update_fast_forwards_explicit_branch_plugins() {
    let workspace = unique_temp_dir("update-explicit-branch");
    let author_repo = workspace.join("author").join("tmux-sensible");
    let bare_repo = workspace.join("remotes").join("tmux-sensible.git");
    let config_path = workspace.join("config").join("tpm.yaml");
    let plugins_dir = workspace.join("plugins");

    init_repo(&author_repo);
    write_file(&author_repo.join("plugin.txt"), "main-v1\n");
    commit_all(&author_repo, "initial");
    git(&author_repo, ["checkout", "-b", "stable"]);
    write_file(&author_repo.join("plugin.txt"), "stable-v1\n");
    commit_all(&author_repo, "stable-v1");
    git(&author_repo, ["checkout", "main"]);
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
                "  branch: stable\n",
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
    assert_eq!(
        fs::read_to_string(plugins_dir.join("tmux-sensible").join("plugin.txt"))
            .expect("installed file should be readable"),
        "stable-v1\n"
    );

    git(&author_repo, ["checkout", "stable"]);
    write_file(&author_repo.join("plugin.txt"), "stable-v2\n");
    commit_all(&author_repo, "stable-v2");
    git(&author_repo, ["push", "origin", "stable"]);

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "update",
        ],
    );

    assert!(output.status.success(), "update should succeed: {output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        format!(
            "Updated tmux-sensible in {}\n",
            plugins_dir.join("tmux-sensible").display()
        )
    );
    assert_eq!(
        fs::read_to_string(plugins_dir.join("tmux-sensible").join("plugin.txt"))
            .expect("updated file should be readable"),
        "stable-v2\n"
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
fn update_switches_no_ref_plugins_to_the_remote_default_branch() {
    let workspace = unique_temp_dir("update-default-branch-switch");
    let author_repo = workspace.join("author").join("tmux-sensible");
    let bare_repo = workspace.join("remotes").join("tmux-sensible.git");
    let config_path = workspace.join("config").join("tpm.yaml");
    let plugins_dir = workspace.join("plugins");

    init_repo(&author_repo);
    write_file(&author_repo.join("plugin.txt"), "main-v1\n");
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

    git(&author_repo, ["checkout", "-b", "stable"]);
    write_file(&author_repo.join("plugin.txt"), "stable-v1\n");
    commit_all(&author_repo, "stable-v1");
    git(&author_repo, ["push", "origin", "stable"]);
    git(&bare_repo, ["symbolic-ref", "HEAD", "refs/heads/stable"]);

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "update",
        ],
    );

    assert!(output.status.success(), "update should succeed: {output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        format!(
            "Updated tmux-sensible in {}\n",
            plugins_dir.join("tmux-sensible").display()
        )
    );
    assert_eq!(
        fs::read_to_string(plugins_dir.join("tmux-sensible").join("plugin.txt"))
            .expect("updated file should be readable"),
        "stable-v1\n"
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
fn update_keeps_tag_pinned_plugins_fixed() {
    let workspace = unique_temp_dir("update-tag");
    let author_repo = workspace.join("author").join("tmux-continuum");
    let bare_repo = workspace.join("remotes").join("tmux-continuum.git");
    let config_path = workspace.join("config").join("tpm.yaml");
    let plugins_dir = workspace.join("plugins");

    init_repo(&author_repo);
    write_file(&author_repo.join("plugin.txt"), "v1\n");
    commit_all(&author_repo, "initial");
    git(&author_repo, ["tag", "v1.0.0"]);
    publish_repo(&author_repo, &bare_repo);

    write_file(&author_repo.join("plugin.txt"), "v2\n");
    commit_all(&author_repo, "second");
    git(&author_repo, ["push", "origin", "main", "--tags"]);

    write_config(
        &config_path,
        &format!(
            concat!(
                "version: 1\n",
                "paths:\n",
                "  plugins: ../plugins\n",
                "plugins:\n",
                "- source: {}\n",
                "  ref: v1.0.0\n",
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

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "update",
        ],
    );

    assert!(output.status.success(), "update should succeed: {output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        format!(
            "Kept pinned tmux-continuum at ref v1.0.0 in {}\n",
            plugins_dir.join("tmux-continuum").display()
        )
    );
    assert_eq!(
        fs::read_to_string(plugins_dir.join("tmux-continuum").join("plugin.txt"))
            .expect("tag-pinned file should be readable"),
        "v1\n"
    );
}

#[test]
fn update_keeps_commit_pinned_plugins_fixed() {
    let workspace = unique_temp_dir("update-sha");
    let author_repo = workspace.join("author").join("tmux-continuum");
    let bare_repo = workspace.join("remotes").join("tmux-continuum.git");
    let config_path = workspace.join("config").join("tpm.yaml");
    let plugins_dir = workspace.join("plugins");

    init_repo(&author_repo);
    write_file(&author_repo.join("plugin.txt"), "v1\n");
    commit_all(&author_repo, "initial");
    let initial_sha = String::from_utf8(run_git(&author_repo, ["rev-parse", "HEAD"]).stdout)
        .expect("sha output should be utf-8")
        .trim()
        .to_string();
    publish_repo(&author_repo, &bare_repo);

    write_file(&author_repo.join("plugin.txt"), "v2\n");
    commit_all(&author_repo, "second");
    git(&author_repo, ["push", "origin", "main"]);

    write_config(
        &config_path,
        &format!(
            concat!(
                "version: 1\n",
                "paths:\n",
                "  plugins: ../plugins\n",
                "plugins:\n",
                "- source: {}\n",
                "  ref: {}\n",
            ),
            bare_repo.display(),
            initial_sha,
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

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "update",
        ],
    );

    assert!(output.status.success(), "update should succeed: {output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        format!(
            "Kept pinned tmux-continuum at ref {} in {}\n",
            initial_sha,
            plugins_dir.join("tmux-continuum").display()
        )
    );
    assert_eq!(
        fs::read_to_string(plugins_dir.join("tmux-continuum").join("plugin.txt"))
            .expect("sha-pinned file should be readable"),
        "v1\n"
    );
}

#[test]
fn update_preserves_plugin_output_order_for_mixed_outcomes() {
    let workspace = unique_temp_dir("update-order");
    let first_repo = workspace.join("author").join("tmux-first");
    let first_bare_repo = workspace.join("remotes").join("tmux-first.git");
    let second_repo = workspace.join("author").join("tmux-second");
    let second_bare_repo = workspace.join("remotes").join("tmux-second.git");
    let config_path = workspace.join("config").join("tpm.yaml");
    let plugins_dir = workspace.join("plugins");

    init_repo(&first_repo);
    write_file(&first_repo.join("plugin.txt"), "first-v1\n");
    commit_all(&first_repo, "initial");
    publish_repo(&first_repo, &first_bare_repo);

    init_repo(&second_repo);
    write_file(&second_repo.join("plugin.txt"), "second-v1\n");
    commit_all(&second_repo, "initial");
    publish_repo(&second_repo, &second_bare_repo);

    write_config(
        &config_path,
        &format!(
            concat!(
                "version: 1\n",
                "paths:\n",
                "  plugins: ../plugins\n",
                "plugins:\n",
                "- source: {}\n",
                "- source: {}\n",
            ),
            first_bare_repo.display(),
            second_bare_repo.display()
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

    write_file(&second_repo.join("plugin.txt"), "second-v2\n");
    commit_all(&second_repo, "second");
    git(&second_repo, ["push", "origin", "main"]);

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "update",
        ],
    );

    assert!(output.status.success(), "update should succeed: {output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        format!(
            concat!(
                "Already up to date tmux-first at {}\n",
                "Updated tmux-second in {}\n",
            ),
            plugins_dir.join("tmux-first").display(),
            plugins_dir.join("tmux-second").display(),
        )
    );
}

#[test]
fn update_fails_for_dirty_repositories() {
    let workspace = unique_temp_dir("update-dirty");
    let author_repo = workspace.join("author").join("tmux-open");
    let bare_repo = workspace.join("remotes").join("tmux-open.git");
    let config_path = workspace.join("config").join("tpm.yaml");
    let plugins_dir = workspace.join("plugins");

    init_repo(&author_repo);
    write_file(&author_repo.join("plugin.txt"), "v1\n");
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

    write_file(&plugins_dir.join("tmux-open").join("plugin.txt"), "dirty\n");

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "update",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf-8");
    assert!(stderr.contains("Failed to update tmux-open"));
    assert!(stderr.contains("uncommitted tracked changes"));
}

#[test]
fn update_rejects_existing_checkouts_from_a_different_source() {
    let workspace = unique_temp_dir("update-mismatch");
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

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "update",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        ""
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf-8");
    assert!(stderr.contains("Failed to update tmux-sensible"));
    assert!(stderr.contains("plugin checkout source does not match configured source"));
    assert!(stderr.contains(&expected_bare_repo.display().to_string()));
    assert_eq!(
        fs::read_to_string(existing_checkout.join("plugin.txt"))
            .expect("existing checkout should remain readable"),
        "unexpected\n"
    );
}

#[cfg(unix)]
#[test]
fn update_colorizes_interactive_terminal_output() {
    let workspace = unique_temp_dir("update-color-terminal");
    let current_repo = workspace.join("author").join("tmux-current");
    let current_bare_repo = workspace.join("remotes").join("tmux-current.git");
    let updated_repo = workspace.join("author").join("tmux-update");
    let updated_bare_repo = workspace.join("remotes").join("tmux-update.git");
    let pinned_repo = workspace.join("author").join("tmux-pinned");
    let pinned_bare_repo = workspace.join("remotes").join("tmux-pinned.git");
    let config_path = workspace.join("config").join("tpm.yaml");
    let plugins_dir = workspace.join("plugins");

    init_repo(&current_repo);
    write_file(&current_repo.join("plugin.txt"), "current-v1\n");
    commit_all(&current_repo, "initial");
    publish_repo(&current_repo, &current_bare_repo);

    init_repo(&updated_repo);
    write_file(&updated_repo.join("plugin.txt"), "update-v1\n");
    commit_all(&updated_repo, "initial");
    publish_repo(&updated_repo, &updated_bare_repo);

    init_repo(&pinned_repo);
    write_file(&pinned_repo.join("plugin.txt"), "pinned-v1\n");
    commit_all(&pinned_repo, "initial");
    git(&pinned_repo, ["tag", "v1.0.0"]);
    publish_repo(&pinned_repo, &pinned_bare_repo);

    write_config(
        &config_path,
        &format!(
            concat!(
                "version: 1\n",
                "paths:\n",
                "  plugins: ../plugins\n",
                "plugins:\n",
                "- source: {}\n",
                "- source: {}\n",
                "- source: {}\n",
                "  ref: v1.0.0\n",
            ),
            current_bare_repo.display(),
            updated_bare_repo.display(),
            pinned_bare_repo.display()
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

    write_file(&updated_repo.join("plugin.txt"), "update-v2\n");
    commit_all(&updated_repo, "second");
    git(&updated_repo, ["push", "origin", "main"]);

    write_file(&pinned_repo.join("plugin.txt"), "pinned-v2\n");
    commit_all(&pinned_repo, "second");
    git(&pinned_repo, ["push", "origin", "main", "--tags"]);

    let output = run_tpm_in_pty_with_env(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "update",
        ],
        vec![("TERM".to_string(), "xterm-256color".to_string())],
    );

    assert!(
        output.status.success(),
        "interactive update should succeed: {output:?}"
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(stdout.contains(&format!("Updating 3 plugins in {}", plugins_dir.display())));
    assert!(stdout.contains("  [1/3] tmux-current... \u{1b}[93malready up to date\u{1b}[0m"));
    assert!(stdout.contains("  [2/3] tmux-update... \u{1b}[92mupdated\u{1b}[0m"));
    assert!(stdout.contains("  [3/3] tmux-pinned... \u{1b}[93mpinned to ref v1.0.0\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[92m1 updated\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[93m1 already up to date\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[93m1 pinned\u{1b}[0m"));
}

#[cfg(unix)]
#[test]
fn update_shows_interactive_progress_in_a_terminal() {
    let workspace = unique_temp_dir("update-interactive");
    let current_repo = workspace.join("author").join("tmux-current");
    let current_bare_repo = workspace.join("remotes").join("tmux-current.git");
    let updated_repo = workspace.join("author").join("tmux-update");
    let updated_bare_repo = workspace.join("remotes").join("tmux-update.git");
    let pinned_repo = workspace.join("author").join("tmux-pinned");
    let pinned_bare_repo = workspace.join("remotes").join("tmux-pinned.git");
    let config_path = workspace.join("config").join("tpm.yaml");
    let plugins_dir = workspace.join("plugins");

    init_repo(&current_repo);
    write_file(&current_repo.join("plugin.txt"), "current-v1\n");
    commit_all(&current_repo, "initial");
    publish_repo(&current_repo, &current_bare_repo);

    init_repo(&updated_repo);
    write_file(&updated_repo.join("plugin.txt"), "update-v1\n");
    commit_all(&updated_repo, "initial");
    publish_repo(&updated_repo, &updated_bare_repo);

    init_repo(&pinned_repo);
    write_file(&pinned_repo.join("plugin.txt"), "pinned-v1\n");
    commit_all(&pinned_repo, "initial");
    git(&pinned_repo, ["tag", "v1.0.0"]);
    publish_repo(&pinned_repo, &pinned_bare_repo);

    write_config(
        &config_path,
        &format!(
            concat!(
                "version: 1\n",
                "paths:\n",
                "  plugins: ../plugins\n",
                "plugins:\n",
                "- source: {}\n",
                "- source: {}\n",
                "- source: {}\n",
                "  ref: v1.0.0\n",
            ),
            current_bare_repo.display(),
            updated_bare_repo.display(),
            pinned_bare_repo.display()
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

    write_file(&updated_repo.join("plugin.txt"), "update-v2\n");
    commit_all(&updated_repo, "second");
    git(&updated_repo, ["push", "origin", "main"]);

    write_file(&pinned_repo.join("plugin.txt"), "pinned-v2\n");
    commit_all(&pinned_repo, "second");
    git(&pinned_repo, ["push", "origin", "main", "--tags"]);

    let output = run_tpm_in_pty_with_env(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "update",
        ],
        vec![("NO_COLOR".to_string(), "1".to_string())],
    );

    assert!(
        output.status.success(),
        "interactive update should succeed: {output:?}"
    );
    let terminal = normalize_terminal_output(&output.stdout);
    assert!(terminal.contains(&format!(
        "Updating 3 plugins in {}\n",
        plugins_dir.display()
    )));
    assert!(terminal.contains("  [1/3] tmux-current... already up to date\n"));
    assert!(terminal.contains("  [2/3] tmux-update... updated\n"));
    assert!(terminal.contains("  [3/3] tmux-pinned... pinned to ref v1.0.0\n"));
    assert!(
        terminal.contains("1 updated, 1 already up to date, 1 pinned, 0 realigned, 0 failed.\n")
    );
    assert!(terminal.contains("Done in "));
    assert!(!terminal.contains("[9"));
}

fn write_config(path: &std::path::Path, contents: &str) {
    fs::create_dir_all(path.parent().expect("config path should have a parent"))
        .expect("config directory should be created");
    fs::write(path, contents).expect("config should be writable");
}

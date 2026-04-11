use std::fs;

mod support;

use support::{commit_all, git, init_repo, publish_repo, run_tpm, unique_temp_dir, write_file};

#[cfg(unix)]
use support::run_tpm_in_pty_with_env;

#[test]
fn sync_cleans_up_installs_missing_plugins_and_updates_existing_plugins_without_double_work() {
    let workspace = unique_temp_dir("sync-efficient");
    let current_repo = workspace.join("author").join("tmux-current");
    let current_bare_repo = workspace.join("remotes").join("tmux-current.git");
    let new_repo = workspace.join("author").join("tmux-new");
    let new_bare_repo = workspace.join("remotes").join("tmux-new.git");
    let config_path = workspace.join("config").join("tpm.yaml");
    let plugins_dir = workspace.join("plugins");
    let stale_dir = plugins_dir.join("tmux-stale");

    init_repo(&current_repo);
    write_file(&current_repo.join("plugin.txt"), "current-v1\n");
    commit_all(&current_repo, "initial");
    publish_repo(&current_repo, &current_bare_repo);

    init_repo(&new_repo);
    write_file(&new_repo.join("plugin.txt"), "new-v1\n");
    commit_all(&new_repo, "initial");
    publish_repo(&new_repo, &new_bare_repo);

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
            current_bare_repo.display(),
            new_bare_repo.display(),
        ),
    );

    fs::create_dir_all(&plugins_dir).expect("plugins directory should exist");
    git(
        &workspace,
        vec![
            "clone".to_string(),
            current_bare_repo.display().to_string(),
            plugins_dir.join("tmux-current").display().to_string(),
        ],
    );
    fs::create_dir_all(&stale_dir).expect("stale directory should exist");

    write_file(&current_repo.join("plugin.txt"), "current-v2\n");
    commit_all(&current_repo, "second");
    git(&current_repo, ["push", "origin", "main"]);

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "sync",
        ],
    );

    assert!(output.status.success(), "sync should succeed: {output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        format!(
            concat!(
                "Removed stale plugin directory {}\n",
                "Updated tmux-current in {}\n",
                "Installed tmux-new into {}\n",
            ),
            stale_dir.display(),
            plugins_dir.join("tmux-current").display(),
            plugins_dir.join("tmux-new").display(),
        )
    );
    assert_eq!(
        String::from_utf8(output.stderr).expect("stderr should be utf-8"),
        ""
    );
    assert_eq!(
        fs::read_to_string(plugins_dir.join("tmux-current").join("plugin.txt"))
            .expect("updated file should be readable"),
        "current-v2\n"
    );
    assert_eq!(
        fs::read_to_string(plugins_dir.join("tmux-new").join("plugin.txt"))
            .expect("installed file should be readable"),
        "new-v1\n"
    );
    assert!(!stale_dir.exists(), "stale directory should be removed");
}

#[test]
fn sync_continues_after_an_update_failure_and_still_installs_missing_plugins() {
    let workspace = unique_temp_dir("sync-continue-after-failure");
    let dirty_repo = workspace.join("author").join("tmux-dirty");
    let dirty_bare_repo = workspace.join("remotes").join("tmux-dirty.git");
    let new_repo = workspace.join("author").join("tmux-new");
    let new_bare_repo = workspace.join("remotes").join("tmux-new.git");
    let config_path = workspace.join("config").join("tpm.yaml");
    let plugins_dir = workspace.join("plugins");
    let stale_dir = plugins_dir.join("tmux-stale");

    init_repo(&dirty_repo);
    write_file(&dirty_repo.join("plugin.txt"), "dirty-v1\n");
    commit_all(&dirty_repo, "initial");
    publish_repo(&dirty_repo, &dirty_bare_repo);

    init_repo(&new_repo);
    write_file(&new_repo.join("plugin.txt"), "new-v1\n");
    commit_all(&new_repo, "initial");
    publish_repo(&new_repo, &new_bare_repo);

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
            dirty_bare_repo.display(),
            new_bare_repo.display(),
        ),
    );

    fs::create_dir_all(&plugins_dir).expect("plugins directory should exist");
    git(
        &workspace,
        vec![
            "clone".to_string(),
            dirty_bare_repo.display().to_string(),
            plugins_dir.join("tmux-dirty").display().to_string(),
        ],
    );
    fs::create_dir_all(&stale_dir).expect("stale directory should exist");
    write_file(
        &plugins_dir.join("tmux-dirty").join("plugin.txt"),
        "dirty-local\n",
    );

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "sync",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        format!(
            concat!(
                "Removed stale plugin directory {}\n",
                "Installed tmux-new into {}\n",
            ),
            stale_dir.display(),
            plugins_dir.join("tmux-new").display(),
        )
    );

    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf-8");
    assert!(stderr.contains("Failed to update tmux-dirty"));
    assert!(stderr.contains("uncommitted tracked changes"));
    assert!(stderr.contains("error: sync reported 1 failed operations"));
    assert_eq!(
        fs::read_to_string(plugins_dir.join("tmux-new").join("plugin.txt"))
            .expect("installed file should be readable"),
        "new-v1\n"
    );
    assert_eq!(
        fs::read_to_string(plugins_dir.join("tmux-dirty").join("plugin.txt"))
            .expect("dirty file should remain readable"),
        "dirty-local\n"
    );
    assert!(!stale_dir.exists(), "stale directory should be removed");
}

#[cfg(unix)]
#[test]
fn sync_colorizes_interactive_terminal_output() {
    let workspace = unique_temp_dir("sync-color-terminal");
    let dirty_repo = workspace.join("author").join("tmux-dirty");
    let dirty_bare_repo = workspace.join("remotes").join("tmux-dirty.git");
    let updated_repo = workspace.join("author").join("tmux-update");
    let updated_bare_repo = workspace.join("remotes").join("tmux-update.git");
    let pinned_repo = workspace.join("author").join("tmux-pinned");
    let pinned_bare_repo = workspace.join("remotes").join("tmux-pinned.git");
    let new_repo = workspace.join("author").join("tmux-new");
    let new_bare_repo = workspace.join("remotes").join("tmux-new.git");
    let config_path = workspace.join("config").join("tpm.yaml");
    let plugins_dir = workspace.join("plugins");
    let dirty_checkout = plugins_dir.join("tmux-dirty");
    let new_checkout = plugins_dir.join("tmux-new");
    let stale_dir = plugins_dir.join("tmux-stale");

    init_repo(&dirty_repo);
    write_file(&dirty_repo.join("plugin.txt"), "dirty-v1\n");
    commit_all(&dirty_repo, "initial");
    publish_repo(&dirty_repo, &dirty_bare_repo);

    init_repo(&updated_repo);
    write_file(&updated_repo.join("plugin.txt"), "update-v1\n");
    commit_all(&updated_repo, "initial");
    publish_repo(&updated_repo, &updated_bare_repo);

    init_repo(&pinned_repo);
    write_file(&pinned_repo.join("plugin.txt"), "pinned-v1\n");
    commit_all(&pinned_repo, "initial");
    git(&pinned_repo, ["tag", "v1.0.0"]);
    publish_repo(&pinned_repo, &pinned_bare_repo);

    init_repo(&new_repo);
    write_file(&new_repo.join("plugin.txt"), "new-v1\n");
    commit_all(&new_repo, "initial");
    publish_repo(&new_repo, &new_bare_repo);

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
                "- source: {}\n",
            ),
            dirty_bare_repo.display(),
            updated_bare_repo.display(),
            pinned_bare_repo.display(),
            new_bare_repo.display(),
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

    fs::create_dir_all(&stale_dir).expect("stale directory should exist");

    write_file(&updated_repo.join("plugin.txt"), "update-v2\n");
    commit_all(&updated_repo, "second");
    git(&updated_repo, ["push", "origin", "main"]);

    write_file(&pinned_repo.join("plugin.txt"), "pinned-v2\n");
    commit_all(&pinned_repo, "second");
    git(&pinned_repo, ["push", "origin", "main", "--tags"]);

    write_file(&dirty_checkout.join("plugin.txt"), "dirty-local\n");
    fs::remove_dir_all(&new_checkout).expect("new checkout should be removable");

    let output = run_tpm_in_pty_with_env(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "sync",
        ],
        vec![("TERM".to_string(), "xterm-256color".to_string())],
    );

    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(stdout.contains(&format!("Syncing 4 plugins in {}", plugins_dir.display())));
    assert!(stdout.contains(&format!(
        "\u{1b}[92mRemoved stale plugin directory\u{1b}[0m {}",
        stale_dir.display()
    )));
    assert!(stdout.contains("  [1/4] tmux-dirty... \u{1b}[91mfailed\u{1b}[0m"));
    assert!(stdout.contains(&format!(
        "\u{1b}[91m         plugin checkout has uncommitted tracked changes: {}\u{1b}[0m",
        dirty_checkout.display()
    )));
    assert!(stdout.contains("  [2/4] tmux-update... \u{1b}[92mupdated\u{1b}[0m"));
    assert!(stdout.contains("  [3/4] tmux-pinned... \u{1b}[93mpinned to ref v1.0.0\u{1b}[0m"));
    assert!(stdout.contains("  [4/4] tmux-new... \u{1b}[92minstalled\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[92m1 removed\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[92m1 installed\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[92m1 updated\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[93m1 pinned\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[91m1 failed\u{1b}[0m"));
}

#[cfg(unix)]
#[test]
fn sync_disables_color_when_no_color_is_set() {
    let workspace = unique_temp_dir("sync-no-color-terminal");
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
            bare_repo.display(),
        ),
    );

    let output = run_tpm_in_pty_with_env(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "sync",
        ],
        vec![
            ("TERM".to_string(), "xterm-256color".to_string()),
            ("NO_COLOR".to_string(), "1".to_string()),
        ],
    );

    assert!(output.status.success(), "sync should succeed: {output:?}");
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(stdout.contains(&format!("Syncing 1 plugin in {}", plugins_dir.display())));
    assert!(stdout.contains("  [1/1] tmux-sensible... installed"));
    assert!(stdout.contains("Done in "));
    assert!(stdout.contains(
        "0 removed, 1 installed, 0 updated, 0 already up to date, 0 pinned, 0 realigned, 0 failed."
    ));
    assert!(!stdout.contains("\u{1b}["));
}

fn write_config(path: &std::path::Path, contents: &str) {
    fs::create_dir_all(path.parent().expect("config path should have a parent"))
        .expect("config directory should be created");
    fs::write(path, contents).expect("config should be writable");
}

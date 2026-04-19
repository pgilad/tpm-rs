use std::fs;

mod support;

use support::{
    commit_all, init_repo, managed_manifest_path, publish_repo, run_tpm, unique_temp_dir,
    write_file,
};

#[test]
fn add_with_skip_install_creates_config_with_reference() {
    let workspace = unique_temp_dir("add-create");
    let config_path = workspace.join("config").join("tpm.yaml");
    let plugins_dir = workspace.join("plugins");

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "--plugins-dir",
            plugins_dir.to_str().expect("plugins dir should be utf-8"),
            "add",
            "catppuccin/tmux",
            "--ref",
            "v2.1.3",
            "--skip-install",
        ],
    );

    assert!(
        output.status.success(),
        "command should succeed: {output:?}"
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        format!(
            "Created {} and added catppuccin/tmux\n",
            config_path.display()
        ),
    );
    assert_eq!(
        fs::read_to_string(&config_path).expect("config should exist"),
        concat!(
            "version: 1\n",
            "plugins:\n",
            "- source: catppuccin/tmux\n",
            "  ref: v2.1.3\n",
        )
    );
    assert!(
        !managed_manifest_path(&plugins_dir).exists(),
        "add --skip-install should not create the managed manifest"
    );
}

#[test]
fn add_with_skip_install_creates_config_with_branch() {
    let workspace = unique_temp_dir("add-create-branch");
    let config_path = workspace.join("config").join("tpm.yaml");

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "add",
            "tmux-plugins/tmux-sensible",
            "--branch",
            "main",
            "--skip-install",
        ],
    );

    assert!(
        output.status.success(),
        "command should succeed: {output:?}"
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        format!(
            "Created {} and added tmux-plugins/tmux-sensible\n",
            config_path.display()
        ),
    );
    assert_eq!(
        fs::read_to_string(&config_path).expect("config should exist"),
        concat!(
            "version: 1\n",
            "plugins:\n",
            "- source: tmux-plugins/tmux-sensible\n",
            "  branch: main\n",
        )
    );
}

#[test]
fn add_creates_config_and_installs_by_default() {
    let workspace = unique_temp_dir("add-create-install");
    let author_repo = workspace.join("author").join("tmux-open");
    let bare_repo = workspace.join("remotes").join("tmux-open.git");
    let config_path = workspace.join("config").join("tpm.yaml");
    let plugins_dir = workspace.join("plugins");

    init_repo(&author_repo);
    write_file(&author_repo.join("plugin.txt"), "open\n");
    commit_all(&author_repo, "initial");
    publish_repo(&author_repo, &bare_repo);

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "--plugins-dir",
            plugins_dir.to_str().expect("plugins dir should be utf-8"),
            "add",
            bare_repo.to_str().expect("repo path should be utf-8"),
        ],
    );

    assert!(
        output.status.success(),
        "command should succeed: {output:?}"
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        format!(
            concat!(
                "Created {} and added tmux-open\n",
                "Installed tmux-open into {}\n",
            ),
            config_path.display(),
            plugins_dir.join("tmux-open").display(),
        ),
    );
    assert_eq!(
        fs::read_to_string(&config_path).expect("config should exist"),
        format!(
            concat!("version: 1\n", "plugins:\n", "- source: {}\n",),
            bare_repo.display(),
        )
    );
    assert_eq!(
        fs::read_to_string(plugins_dir.join("tmux-open").join("plugin.txt"))
            .expect("installed file should be readable"),
        "open\n"
    );
    let manifest =
        fs::read_to_string(managed_manifest_path(&plugins_dir)).expect("manifest should exist");
    assert!(manifest.contains("tmux-open:"));
    assert!(manifest.contains("path: tmux-open"));
}

#[test]
fn add_rejects_duplicate_install_names() {
    let workspace = unique_temp_dir("add-duplicate");
    let config_path = workspace.join("config").join("tpm.yaml");
    fs::create_dir_all(
        config_path
            .parent()
            .expect("config path should have a parent"),
    )
    .expect("config directory should be created");
    fs::write(
        &config_path,
        concat!(
            "version: 1\n",
            "plugins:\n",
            "- source: tmux-plugins/tmux-sensible\n",
        ),
    )
    .expect("config should be writable");

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "add",
            "https://github.com/tmux-plugins/tmux-sensible.git",
        ],
    );

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .expect("stderr should be utf-8")
            .contains("already configured by `tmux-plugins/tmux-sensible`")
    );
    assert_eq!(
        fs::read_to_string(&config_path).expect("config should remain readable"),
        concat!(
            "version: 1\n",
            "plugins:\n",
            "- source: tmux-plugins/tmux-sensible\n",
        )
    );
}

#[test]
fn add_allows_same_repo_names_from_different_owners() {
    let workspace = unique_temp_dir("add-distinct-owners");
    let config_path = workspace.join("config").join("tpm.yaml");
    fs::create_dir_all(
        config_path
            .parent()
            .expect("config path should have a parent"),
    )
    .expect("config directory should be created");
    fs::write(
        &config_path,
        concat!("version: 1\n", "plugins:\n", "- source: pgilad/plugin-a\n",),
    )
    .expect("config should be writable");

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "add",
            "tmux-plugins/plugin-a",
            "--skip-install",
        ],
    );

    assert!(
        output.status.success(),
        "command should succeed: {output:?}"
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        format!("Added tmux-plugins/plugin-a to {}\n", config_path.display()),
    );
    assert_eq!(
        fs::read_to_string(&config_path).expect("config should remain readable"),
        concat!(
            "version: 1\n",
            "plugins:\n",
            "- source: pgilad/plugin-a\n",
            "- source: tmux-plugins/plugin-a\n",
        )
    );
}

#[test]
fn add_installs_only_the_added_plugin_by_default() {
    let workspace = unique_temp_dir("add-install");
    let author_repo = workspace.join("author").join("tmux-open");
    let bare_repo = workspace.join("remotes").join("tmux-open.git");
    let config_path = workspace.join("config").join("tpm.yaml");
    let plugins_dir = workspace.join("plugins");

    init_repo(&author_repo);
    write_file(&author_repo.join("plugin.txt"), "open\n");
    commit_all(&author_repo, "initial");
    publish_repo(&author_repo, &bare_repo);

    fs::create_dir_all(
        config_path
            .parent()
            .expect("config path should have a parent"),
    )
    .expect("config directory should be created");
    fs::write(
        &config_path,
        concat!(
            "version: 1\n",
            "plugins:\n",
            "- source: tmux-plugins/tmux-sensible\n",
        ),
    )
    .expect("config should be writable");

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "--plugins-dir",
            plugins_dir.to_str().expect("plugins dir should be utf-8"),
            "add",
            bare_repo.to_str().expect("repo path should be utf-8"),
        ],
    );

    assert!(
        output.status.success(),
        "command should succeed: {output:?}"
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        format!(
            concat!("Added tmux-open to {}\n", "Installed tmux-open into {}\n",),
            config_path.display(),
            plugins_dir.join("tmux-open").display(),
        ),
    );
    assert_eq!(
        fs::read_to_string(&config_path).expect("config should remain readable"),
        format!(
            concat!(
                "version: 1\n",
                "plugins:\n",
                "- source: tmux-plugins/tmux-sensible\n",
                "- source: {}\n",
            ),
            bare_repo.display(),
        )
    );
    assert_eq!(
        fs::read_to_string(plugins_dir.join("tmux-open").join("plugin.txt"))
            .expect("installed file should be readable"),
        "open\n"
    );
    let manifest =
        fs::read_to_string(managed_manifest_path(&plugins_dir)).expect("manifest should exist");
    assert!(manifest.contains("tmux-open:"));
    assert!(manifest.contains("path: tmux-open"));
    assert!(
        !manifest.contains("tmux-sensible:"),
        "add should not mark unrelated configured plugins as managed"
    );
    assert!(
        !plugins_dir.join("tmux-sensible").exists(),
        "add should install only the added plugin by default"
    );
}

#[test]
fn add_rejects_unsupported_source_format() {
    let workspace = unique_temp_dir("add-invalid-source");
    let config_path = workspace.join("config").join("tpm.yaml");

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "add",
            "foo",
        ],
    );

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .expect("stderr should be utf-8")
            .contains("expected GitHub shorthand `owner/repo`")
    );
    assert!(!config_path.exists(), "config should not be created");
}

#[test]
fn add_rejects_legacy_tpm_plugin_manager() {
    let workspace = unique_temp_dir("add-legacy-tpm");
    let config_path = workspace.join("config").join("tpm.yaml");

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "add",
            "tmux-plugins/tpm",
        ],
    );

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .expect("stderr should be utf-8")
            .contains("the legacy TPM plugin manager is not supported")
    );
    assert!(!config_path.exists(), "config should not be created");
}

#[test]
fn remove_rewrites_config_and_preserves_paths() {
    let workspace = unique_temp_dir("remove");
    let config_path = workspace.join("config").join("tpm.yaml");
    fs::create_dir_all(
        config_path
            .parent()
            .expect("config path should have a parent"),
    )
    .expect("config directory should be created");
    fs::write(
        &config_path,
        concat!(
            "version: 1\n",
            "paths:\n",
            "  plugins: ../plugins\n",
            "plugins:\n",
            "- source: tmux-plugins/tmux-sensible\n",
            "- source: catppuccin/tmux\n",
            "  ref: v2.1.3\n",
        ),
    )
    .expect("config should be writable");

    let output = run_tpm(
        &workspace,
        [
            "--config",
            config_path.to_str().expect("config path should be utf-8"),
            "remove",
            "catppuccin/tmux",
        ],
    );

    assert!(
        output.status.success(),
        "command should succeed: {output:?}"
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        format!("Removed catppuccin/tmux from {}\n", config_path.display()),
    );
    assert_eq!(
        fs::read_to_string(&config_path).expect("config should remain readable"),
        concat!(
            "version: 1\n",
            "paths:\n",
            "  plugins: ../plugins\n",
            "plugins:\n",
            "- source: tmux-plugins/tmux-sensible\n",
        )
    );
}

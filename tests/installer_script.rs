use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

mod support;

use support::{prepend_path, set_executable, unique_temp_dir, write_file};

#[test]
fn rendered_installer_defaults_to_the_embedded_release_version() {
    let workspace = unique_temp_dir("installer-rendered-default");
    let releases_dir = workspace.join("releases");
    let install_dir = workspace.join("bin");
    let rendered_script = workspace.join("install.sh");
    let version = "v1.2.3";
    let target = "x86_64-unknown-linux-gnu";

    create_release_asset(
        &releases_dir,
        version,
        target,
        &format!("tpm-{target}"),
        version,
    );

    let render = render_installer(version, &rendered_script);
    assert!(
        render.status.success(),
        "render-installer should succeed: {}",
        describe_output(&render)
    );

    let output = run_installer(
        &rendered_script,
        &workspace,
        [
            "--dir",
            install_dir.to_str().expect("install dir should be utf-8"),
            "--target",
            target,
        ],
        default_installer_envs(&workspace, &releases_dir),
    );

    assert!(
        output.status.success(),
        "rendered installer should succeed: {}",
        describe_output(&output)
    );
    assert_eq!(
        fs::read_to_string(install_dir.join("tpm")).expect("installed tpm should be readable"),
        format!("{version}\n")
    );
}

#[test]
fn rendered_installer_keeps_version_overrides() {
    let workspace = unique_temp_dir("installer-rendered-override");
    let releases_dir = workspace.join("releases");
    let install_dir = workspace.join("bin");
    let rendered_script = workspace.join("install.sh");
    let target = "x86_64-unknown-linux-gnu";

    create_release_asset(
        &releases_dir,
        "v1.0.0",
        target,
        &format!("tpm-{target}"),
        "default",
    );
    create_release_asset(
        &releases_dir,
        "v2.0.0",
        target,
        &format!("tpm-{target}"),
        "env",
    );
    create_release_asset(
        &releases_dir,
        "v3.0.0",
        target,
        &format!("tpm-{target}"),
        "flag",
    );

    let render = render_installer("v1.0.0", &rendered_script);
    assert!(
        render.status.success(),
        "render-installer should succeed: {}",
        describe_output(&render)
    );

    let output = run_installer(
        &rendered_script,
        &workspace,
        [
            "--dir",
            install_dir.to_str().expect("install dir should be utf-8"),
            "--target",
            target,
            "--version",
            "v3.0.0",
        ],
        envs_with_path(
            &workspace,
            &releases_dir,
            env::var_os("PATH").expect("PATH should be set"),
            &[("TPM_INSTALL_VERSION", "v2.0.0")],
        ),
    );

    assert!(
        output.status.success(),
        "rendered installer with overrides should succeed: {}",
        describe_output(&output)
    );
    assert_eq!(
        fs::read_to_string(install_dir.join("tpm")).expect("installed tpm should be readable"),
        "flag\n"
    );
}

#[test]
fn installer_fails_early_on_musl_linux() {
    let workspace = unique_temp_dir("installer-musl");
    let fake_bin = workspace.join("fake-bin");
    let install_dir = workspace.join("bin");

    write_executable_script(
        &fake_bin.join("uname"),
        r#"#!/usr/bin/env sh
case "${1:-}" in
  -s)
    printf '%s\n' 'Linux'
    ;;
  -m)
    printf '%s\n' 'x86_64'
    ;;
  *)
    exit 1
    ;;
esac
"#,
    );
    write_executable_script(
        &fake_bin.join("ldd"),
        r#"#!/usr/bin/env sh
printf '%s\n' 'musl libc (x86_64)'
"#,
    );

    let output = run_installer(
        &install_script_path(),
        &workspace,
        [
            "--dir",
            install_dir.to_str().expect("install dir should be utf-8"),
        ],
        envs_with_path(
            &workspace,
            &workspace.join("unused-releases"),
            prepend_path(&fake_bin).into(),
            &[],
        ),
    );

    assert!(
        !output.status.success(),
        "musl install should fail: {}",
        describe_output(&output)
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf-8");
    assert!(
        stderr.contains("musl-based Linux is not supported"),
        "stderr should mention musl support: {stderr}"
    );
    assert!(
        stderr.contains("Supported release targets"),
        "stderr should list supported targets: {stderr}"
    );
    assert!(
        !install_dir.join("tpm").exists(),
        "installer should fail before writing a binary"
    );
}

#[test]
fn installer_prefers_arm64_archive_under_rosetta() {
    let workspace = unique_temp_dir("installer-rosetta");
    let releases_dir = workspace.join("releases");
    let install_dir = workspace.join("bin");
    let fake_bin = workspace.join("fake-bin");

    create_release_asset(
        &releases_dir,
        "v1.0.0",
        "aarch64-apple-darwin",
        "tpm-aarch64-apple-darwin",
        "native-arm64",
    );

    write_executable_script(
        &fake_bin.join("uname"),
        r#"#!/usr/bin/env sh
case "${1:-}" in
  -s)
    printf '%s\n' 'Darwin'
    ;;
  -m)
    printf '%s\n' 'x86_64'
    ;;
  *)
    exit 1
    ;;
esac
"#,
    );
    write_executable_script(
        &fake_bin.join("sysctl"),
        r#"#!/usr/bin/env sh
case "$*" in
  "-in sysctl.proc_translated")
    printf '%s\n' '1'
    ;;
  "-in hw.optional.arm64")
    printf '%s\n' '1'
    ;;
  *)
    exit 1
    ;;
esac
"#,
    );

    let output = run_installer(
        &install_script_path(),
        &workspace,
        [
            "--dir",
            install_dir.to_str().expect("install dir should be utf-8"),
            "--version",
            "v1.0.0",
        ],
        envs_with_path(
            &workspace,
            &releases_dir,
            prepend_path(&fake_bin).into(),
            &[],
        ),
    );

    assert!(
        output.status.success(),
        "rosetta install should succeed: {}",
        describe_output(&output)
    );
    assert_eq!(
        fs::read_to_string(install_dir.join("tpm")).expect("installed tpm should be readable"),
        "native-arm64\n"
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(
        stdout.contains("Downloading tpm for aarch64-apple-darwin"),
        "stdout should mention the arm64 target: {stdout}"
    );
}

#[test]
fn installer_prints_summary_and_shell_specific_path_hint() {
    let workspace = unique_temp_dir("installer-output");
    let releases_dir = workspace.join("releases");
    let install_dir = workspace.join("bin");
    let target = "x86_64-unknown-linux-gnu";

    create_release_asset(
        &releases_dir,
        "v1.0.0",
        target,
        &format!("tpm-{target}"),
        "installed",
    );

    let output = run_installer(
        &install_script_path(),
        &workspace,
        [
            "--dir",
            install_dir.to_str().expect("install dir should be utf-8"),
            "--version",
            "v1.0.0",
            "--target",
            target,
        ],
        envs_with_path(
            &workspace,
            &releases_dir,
            env::var_os("PATH").expect("PATH should be set"),
            &[("SHELL", "/bin/zsh")],
        ),
    );

    assert!(
        output.status.success(),
        "install should succeed: {}",
        describe_output(&output)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(
        stdout.contains("tpm installer"),
        "stdout should include the installer heading: {stdout}"
    );
    assert!(
        stdout.contains("version: v1.0.0"),
        "stdout should include the selected version: {stdout}"
    );
    assert!(
        stdout.contains(&format!("target: {target}")),
        "stdout should include the selected target: {stdout}"
    );
    assert!(
        stdout.contains(&format!("install: {}", install_dir.join("tpm").display())),
        "stdout should include the install path: {stdout}"
    );
    assert!(
        stdout.contains(&format!("Downloading tpm for {target}")),
        "stdout should include the download step: {stdout}"
    );
    assert!(
        stdout.contains("==> Verifying checksum"),
        "stdout should include the checksum step: {stdout}"
    );
    assert!(
        stdout.contains("==> Installing tpm"),
        "stdout should include the install step: {stdout}"
    );
    assert!(
        stdout.contains(&format!("installed: {}", install_dir.join("tpm").display())),
        "stdout should include the success line: {stdout}"
    );
    assert!(
        stdout.contains(&format!(
            "Run: {} --version",
            install_dir.join("tpm").display()
        )),
        "stdout should include the post-install check command: {stdout}"
    );
    assert!(
        stdout.contains("Add this to ~/.zshrc:"),
        "stdout should include a zsh-specific PATH hint: {stdout}"
    );
    assert!(
        stdout.contains(&format!("export PATH=\"{}:$PATH\"", install_dir.display())),
        "stdout should include the PATH export snippet: {stdout}"
    );

    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf-8");
    assert!(
        stderr.contains(&format!(
            "warning: {} is not on PATH",
            install_dir.display()
        )),
        "stderr should include the PATH warning: {stderr}"
    );
}

#[test]
fn installer_does_not_clobber_a_preexisting_fixed_temp_path() {
    let workspace = unique_temp_dir("installer-preexisting-temp");
    let releases_dir = workspace.join("releases");
    let install_dir = workspace.join("bin");
    let target = "x86_64-unknown-linux-gnu";

    fs::create_dir_all(&install_dir).expect("install dir should be created");
    write_file(&install_dir.join("tpm.tmp"), "sentinel\n");
    create_release_asset(
        &releases_dir,
        "v1.0.0",
        target,
        &format!("tpm-{target}"),
        "installed",
    );

    let output = run_installer(
        &install_script_path(),
        &workspace,
        [
            "--dir",
            install_dir.to_str().expect("install dir should be utf-8"),
            "--version",
            "v1.0.0",
            "--target",
            target,
        ],
        default_installer_envs(&workspace, &releases_dir),
    );

    assert!(
        output.status.success(),
        "install should succeed: {}",
        describe_output(&output)
    );
    assert_eq!(
        fs::read_to_string(install_dir.join("tpm")).expect("installed tpm should be readable"),
        "installed\n"
    );
    assert_eq!(
        fs::read_to_string(install_dir.join("tpm.tmp"))
            .expect("fixed temp sentinel should remain readable"),
        "sentinel\n"
    );
}

#[test]
fn installer_requires_a_checksum_tool() {
    let workspace = unique_temp_dir("installer-checksum-required");
    let fake_bin = workspace.join("fake-bin");
    let install_dir = workspace.join("bin");
    let target = "x86_64-unknown-linux-gnu";

    fs::create_dir_all(&install_dir).expect("install dir should be created");
    write_file(&install_dir.join("tpm"), "existing\n");
    mirror_commands_into(
        &fake_bin,
        &[
            "awk", "chmod", "cp", "mkdir", "mktemp", "mv", "rm", "sh", "tar", "tr", "uname",
        ],
    );

    let output = run_installer(
        &install_script_path(),
        &workspace,
        [
            "--dir",
            install_dir.to_str().expect("install dir should be utf-8"),
            "--target",
            target,
        ],
        envs_with_path(
            &workspace,
            &workspace.join("unused-releases"),
            fake_bin.as_os_str().to_os_string(),
            &[],
        ),
    );

    assert!(
        !output.status.success(),
        "install should fail without a checksum tool: {}",
        describe_output(&output)
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf-8");
    assert!(
        stderr.contains("missing required checksum tool: shasum or sha256sum"),
        "stderr should mention the missing checksum tool: {stderr}"
    );
    assert_eq!(
        fs::read_to_string(install_dir.join("tpm")).expect("existing tpm should be readable"),
        "existing\n"
    );
    assert!(
        !install_dir.join("tpm.tmp").exists(),
        "temporary install path should not be created on early failure"
    );
}

#[test]
fn installer_keeps_the_existing_binary_when_archive_layout_is_unexpected() {
    let workspace = unique_temp_dir("installer-atomic");
    let releases_dir = workspace.join("releases");
    let install_dir = workspace.join("bin");
    let target = "x86_64-unknown-linux-gnu";

    fs::create_dir_all(&install_dir).expect("install dir should be created");
    write_file(&install_dir.join("tpm"), "existing\n");

    create_release_asset(&releases_dir, "v1.0.0", target, "unexpected-dir", "wrong");

    let output = run_installer(
        &install_script_path(),
        &workspace,
        [
            "--dir",
            install_dir.to_str().expect("install dir should be utf-8"),
            "--version",
            "v1.0.0",
            "--target",
            target,
        ],
        default_installer_envs(&workspace, &releases_dir),
    );

    assert!(
        !output.status.success(),
        "install should fail for an unexpected archive layout: {}",
        describe_output(&output)
    );
    assert_eq!(
        fs::read_to_string(install_dir.join("tpm")).expect("existing tpm should be readable"),
        "existing\n"
    );
    assert!(
        !install_dir.join("tpm.tmp").exists(),
        "temporary install path should be cleaned up on failure"
    );
}

fn render_installer(version: &str, output_path: &Path) -> Output {
    Command::new("bash")
        .current_dir(repo_root())
        .arg(render_installer_path())
        .arg(version)
        .arg(output_path)
        .output()
        .expect("render-installer should run")
}

fn run_installer<const N: usize>(
    script_path: &Path,
    cwd: &Path,
    args: [&str; N],
    envs: Vec<(String, std::ffi::OsString)>,
) -> Output {
    let mut command = Command::new("sh");
    command.current_dir(cwd).arg(script_path).args(args);
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().expect("installer should run")
}

fn default_installer_envs(
    workspace: &Path,
    releases_dir: &Path,
) -> Vec<(String, std::ffi::OsString)> {
    envs_with_path(
        workspace,
        releases_dir,
        env::var_os("PATH").expect("PATH should be set"),
        &[],
    )
}

fn envs_with_path(
    workspace: &Path,
    releases_dir: &Path,
    path: std::ffi::OsString,
    extra_envs: &[(&str, &str)],
) -> Vec<(String, std::ffi::OsString)> {
    let home_dir = workspace.join("home");
    fs::create_dir_all(&home_dir).expect("home dir should be created");

    let mut envs = vec![
        ("HOME".to_string(), home_dir.into_os_string()),
        ("PATH".to_string(), path),
        (
            "TPM_INSTALL_BASE_URL".to_string(),
            file_url(releases_dir).into(),
        ),
    ];

    envs.extend(
        extra_envs
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).into())),
    );

    envs
}

fn create_release_asset(
    releases_dir: &Path,
    version: &str,
    target: &str,
    archive_root: &str,
    binary_contents: &str,
) {
    let staging_root = releases_dir
        .join("staging")
        .join(format!("{version}-{target}-{archive_root}"));
    let archive_dir = staging_root.join(archive_root);
    fs::create_dir_all(&archive_dir).expect("archive dir should be created");
    write_file(&archive_dir.join("tpm"), &format!("{binary_contents}\n"));
    write_file(&archive_dir.join("README.md"), "README\n");
    write_file(&archive_dir.join("CHANGELOG.md"), "CHANGELOG\n");
    write_file(&archive_dir.join("LICENSE"), "MIT\n");

    let version_dir = releases_dir.join("download").join(version);
    fs::create_dir_all(&version_dir).expect("version dir should be created");

    let archive_path = version_dir.join(format!("tpm-{target}.tar.gz"));
    let tar_output = Command::new("tar")
        .arg("-czf")
        .arg(&archive_path)
        .arg("-C")
        .arg(&staging_root)
        .arg(archive_root)
        .output()
        .expect("tar should run");
    assert!(
        tar_output.status.success(),
        "tar should succeed: {}",
        describe_output(&tar_output)
    );

    let checksum_output = checksum_command()
        .arg(&archive_path)
        .output()
        .expect("checksum command should run");
    assert!(
        checksum_output.status.success(),
        "checksum should succeed: {}",
        describe_output(&checksum_output)
    );
    let checksum = String::from_utf8(checksum_output.stdout)
        .expect("checksum output should be utf-8")
        .split_whitespace()
        .next()
        .expect("checksum output should include a hash")
        .to_string();

    write_file(
        &version_dir.join(format!("tpm-{target}.tar.gz.sha256")),
        &format!("{checksum}\n"),
    );
}

fn checksum_command() -> Command {
    if command_exists("shasum") {
        let mut command = Command::new("shasum");
        command.arg("-a").arg("256");
        return command;
    }

    if command_exists("sha256sum") {
        return Command::new("sha256sum");
    }

    panic!("expected shasum or sha256sum to be available");
}

fn mirror_commands_into(directory: &Path, commands: &[&str]) {
    fs::create_dir_all(directory).expect("fake bin directory should be created");

    for command in commands {
        let target = absolute_command_path(command);
        write_executable_script(
            &directory.join(command),
            &format!(
                "#!/bin/sh\nexec {:?} \"$@\"\n",
                target.display().to_string()
            ),
        );
    }
}

fn absolute_command_path(command: &str) -> PathBuf {
    let output = Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {command}"))
        .output()
        .expect("shell should resolve command paths");

    assert!(
        output.status.success(),
        "expected `{command}` to be available: {}",
        describe_output(&output)
    );

    let path = String::from_utf8(output.stdout)
        .expect("resolved command path should be utf-8")
        .trim()
        .to_string();
    assert!(
        !path.is_empty(),
        "expected a non-empty resolved path for `{command}`"
    );

    PathBuf::from(path)
}

fn command_exists(command: &str) -> bool {
    env::var_os("PATH")
        .map(|path| env::split_paths(&path).any(|entry| entry.join(command).exists()))
        .unwrap_or(false)
}

fn write_executable_script(path: &Path, contents: &str) {
    write_file(path, contents);
    set_executable(path);
}

fn file_url(path: &Path) -> String {
    let absolute = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    format!("file://{}", absolute.display())
}

fn install_script_path() -> PathBuf {
    repo_root().join("scripts/install.sh")
}

fn render_installer_path() -> PathBuf {
    repo_root().join("scripts/render-installer.sh")
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn describe_output(output: &Output) -> String {
    format!(
        "status={:?}, stdout={:?}, stderr={:?}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

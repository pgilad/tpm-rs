#![allow(dead_code)]

use std::{
    env,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

pub fn run_tpm<I, S>(cwd: &Path, args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    run_tpm_with_env(cwd, args, std::iter::empty::<(&str, &str)>())
}

pub fn run_tpm_with_env<I, S, E, K, V>(cwd: &Path, args: I, envs: E) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
    E: IntoIterator<Item = (K, V)>,
    K: AsRef<OsStr>,
    V: AsRef<OsStr>,
{
    run_binary_with_env(Path::new(env!("CARGO_BIN_EXE_tpm")), cwd, args, envs)
}

#[cfg(unix)]
pub fn run_tpm_in_pty<I, S>(cwd: &Path, args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    run_tpm_in_pty_with_env(cwd, args, std::iter::empty::<(&str, &str)>())
}

#[cfg(unix)]
pub fn run_tpm_in_pty_with_env<I, S, E, K, V>(cwd: &Path, args: I, envs: E) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
    E: IntoIterator<Item = (K, V)>,
    K: AsRef<OsStr>,
    V: AsRef<OsStr>,
{
    run_binary_in_pty_with_env(Path::new(env!("CARGO_BIN_EXE_tpm")), cwd, args, envs)
}

pub fn run_tpm_without_home_with_env<I, S, E, K, V>(cwd: &Path, args: I, envs: E) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
    E: IntoIterator<Item = (K, V)>,
    K: AsRef<OsStr>,
    V: AsRef<OsStr>,
{
    let mut command = Command::new(Path::new(env!("CARGO_BIN_EXE_tpm")));
    command
        .current_dir(cwd)
        .env_remove("HOME")
        .env_remove("TMUX")
        .env_remove("TMUX_PANE")
        .args(args);
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().expect("tpm command should run")
}

pub fn run_binary_with_env<I, S, E, K, V>(binary: &Path, cwd: &Path, args: I, envs: E) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
    E: IntoIterator<Item = (K, V)>,
    K: AsRef<OsStr>,
    V: AsRef<OsStr>,
{
    let home_dir = test_home_dir(cwd);

    let mut command = Command::new(binary);
    command
        .current_dir(cwd)
        .env("HOME", &home_dir)
        .env_remove("TMUX")
        .env_remove("TMUX_PANE")
        .args(args);
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().expect("tpm command should run")
}

#[cfg(unix)]
pub fn run_binary_in_pty_with_env<I, S, E, K, V>(
    binary: &Path,
    cwd: &Path,
    args: I,
    envs: E,
) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
    E: IntoIterator<Item = (K, V)>,
    K: AsRef<OsStr>,
    V: AsRef<OsStr>,
{
    let home_dir = test_home_dir(cwd);
    let args = args
        .into_iter()
        .map(|arg| arg.as_ref().to_os_string())
        .collect::<Vec<_>>();

    let mut command = script_command(binary, &args);
    command
        .current_dir(cwd)
        .env("HOME", &home_dir)
        .env_remove("TMUX")
        .env_remove("TMUX_PANE")
        .env_remove("NO_COLOR")
        .env_remove("CLICOLOR")
        .env_remove("CLICOLOR_FORCE");
    for (key, value) in envs {
        command.env(key, value);
    }
    command
        .output()
        .expect("tpm command should run in a pseudo-terminal")
}

pub fn normalize_terminal_output(output: &[u8]) -> String {
    let mut rendered = String::new();

    for byte in output {
        match byte {
            b'\r' => {}
            0x08 | 0x7f => {
                rendered.pop();
            }
            b'\n' | b'\t' => rendered.push(*byte as char),
            0x20..=0x7e => rendered.push(*byte as char),
            _ => {}
        }
    }

    rendered
}

fn test_home_dir(cwd: &Path) -> PathBuf {
    let home_dir = cwd.join("home");
    fs::create_dir_all(&home_dir).expect("home directory should be created");
    fs::write(
        home_dir.join(".gitconfig"),
        concat!("[protocol \"file\"]\n", "\tallow = always\n",),
    )
    .expect("git config should be writable");
    home_dir
}

pub fn unique_temp_dir(name: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let directory =
        std::env::temp_dir().join(format!("tpm-rs-test-{name}-{}-{stamp}", std::process::id()));
    fs::create_dir_all(&directory).expect("temp directory should be created");
    directory
}

pub fn git<I, S>(cwd: &Path, args: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = run_git(cwd, args);
    assert!(
        output.status.success(),
        "git command should succeed: {output:?}"
    );
}

pub fn run_git<I, S>(cwd: &Path, args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new("git")
        .arg("-c")
        .arg("commit.gpgSign=false")
        .arg("-c")
        .arg("tag.gpgSign=false")
        .arg("-c")
        .arg("protocol.file.allow=always")
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "TPM Test")
        .env("GIT_AUTHOR_EMAIL", "tpm-test@example.com")
        .env("GIT_COMMITTER_NAME", "TPM Test")
        .env("GIT_COMMITTER_EMAIL", "tpm-test@example.com")
        .args(args)
        .output()
        .expect("git command should run")
}

pub fn init_repo(path: &Path) {
    fs::create_dir_all(path).expect("repo directory should be created");
    git(path, ["init", "--initial-branch=main"]);
}

pub fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("parent directory should be created");
    }
    fs::write(path, contents).expect("file should be writable");
}

#[cfg(unix)]
pub fn set_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let metadata = fs::metadata(path).expect("metadata should be readable");
    let mut permissions = metadata.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("permissions should be writable");
}

pub fn prepend_path(path: &Path) -> String {
    match env::var_os("PATH") {
        Some(existing) => format!("{}:{}", path.display(), existing.to_string_lossy()),
        None => path.display().to_string(),
    }
}

pub fn commit_all(repo: &Path, message: &str) {
    git(repo, ["add", "."]);
    git(repo, ["commit", "-m", message]);
}

pub fn publish_repo(author_repo: &Path, bare_repo: &Path) {
    if let Some(parent) = bare_repo.parent() {
        fs::create_dir_all(parent).expect("bare repo parent should exist");
    }

    let bare_repo_str = bare_repo.to_str().expect("bare repo path should be utf-8");
    let author_repo_str = author_repo
        .to_str()
        .expect("author repo path should be utf-8");
    let root = author_repo
        .parent()
        .expect("author repo should have a parent directory");

    git(root, ["clone", "--bare", author_repo_str, bare_repo_str]);
    git(author_repo, ["remote", "add", "origin", bare_repo_str]);
}

#[cfg(target_os = "macos")]
fn script_command(binary: &Path, args: &[std::ffi::OsString]) -> Command {
    let mut command = Command::new("script");
    command.arg("-q").arg("/dev/null").arg(binary).args(args);
    command
}

#[cfg(target_os = "linux")]
fn script_command(binary: &Path, args: &[std::ffi::OsString]) -> Command {
    let mut rendered = Vec::with_capacity(args.len() + 1);
    rendered.push(shell_quote(binary.as_os_str()));
    rendered.extend(args.iter().map(|arg| shell_quote(arg)));

    let mut command = Command::new("script");
    command
        .arg("-q")
        .arg("-e")
        .arg("-c")
        .arg(rendered.join(" "))
        .arg("/dev/null");
    command
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn script_command(binary: &Path, args: &[std::ffi::OsString]) -> Command {
    let mut command = Command::new(binary);
    command.args(args);
    command
}

#[cfg(target_os = "linux")]
fn shell_quote(value: &OsStr) -> String {
    let rendered = value.to_string_lossy().replace('\'', "'\"'\"'");
    format!("'{rendered}'")
}

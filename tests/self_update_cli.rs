use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use flate2::{Compression, write::GzEncoder};
use sha2::{Digest, Sha256};
use tar::{Builder, EntryType};

mod support;

use support::{run_binary_with_env, set_executable, unique_temp_dir};

#[test]
fn self_update_replaces_the_running_binary_when_a_newer_release_exists() {
    let workspace = unique_temp_dir("self-update-newer");
    let releases_dir = workspace.join("releases");
    let installed_binary = install_test_binary(&workspace);
    let current_version = binary_version(&installed_binary);

    write_release_asset(
        &releases_dir,
        "test-target",
        "2099.01.01-1",
        "updated-release",
    );

    let output = run_binary_with_env(
        &installed_binary,
        &workspace,
        ["self-update"],
        [
            (
                "TPM_INSTALL_BASE_URL",
                releases_dir
                    .to_str()
                    .expect("releases path should be utf-8"),
            ),
            ("TPM_SELF_UPDATE_TARGET", "test-target"),
        ],
    );

    assert!(
        output.status.success(),
        "self-update should succeed: {output:?}"
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        format!(
            "Updated tpm from {current_version} to 2099.01.01-1 at {}\n",
            installed_binary.display()
        )
    );
    assert_eq!(binary_version(&installed_binary), "2099.01.01-1");
}

#[test]
fn self_update_reports_when_the_current_binary_is_already_latest() {
    let workspace = unique_temp_dir("self-update-current");
    let releases_dir = workspace.join("releases");
    let installed_binary = install_test_binary(&workspace);
    let before = fs::read(&installed_binary).expect("installed binary should be readable");
    let current_version = binary_version(&installed_binary);

    write_release_asset(
        &releases_dir,
        "test-target",
        &current_version,
        "same-release",
    );

    let output = run_binary_with_env(
        &installed_binary,
        &workspace,
        ["self-update"],
        [
            (
                "TPM_INSTALL_BASE_URL",
                releases_dir
                    .to_str()
                    .expect("releases path should be utf-8"),
            ),
            ("TPM_SELF_UPDATE_TARGET", "test-target"),
        ],
    );

    assert!(
        output.status.success(),
        "self-update should succeed: {output:?}"
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        format!(
            "Already up to date tpm {current_version} at {}\n",
            installed_binary.display()
        )
    );
    assert_eq!(
        fs::read(&installed_binary).expect("installed binary should still be readable"),
        before
    );
}

#[test]
fn self_update_keeps_the_existing_binary_when_checksum_verification_fails() {
    let workspace = unique_temp_dir("self-update-checksum");
    let releases_dir = workspace.join("releases");
    let installed_binary = install_test_binary(&workspace);
    let before = fs::read(&installed_binary).expect("installed binary should be readable");

    write_release_asset(
        &releases_dir,
        "test-target",
        "2099.01.01-1",
        "updated-release",
    );
    let checksum_path = releases_dir
        .join("latest")
        .join("download")
        .join("tpm-test-target.tar.gz.sha256");
    fs::write(&checksum_path, "not-a-real-checksum\n").expect("checksum should be writable");

    let output = run_binary_with_env(
        &installed_binary,
        &workspace,
        ["self-update"],
        [
            (
                "TPM_INSTALL_BASE_URL",
                releases_dir
                    .to_str()
                    .expect("releases path should be utf-8"),
            ),
            ("TPM_SELF_UPDATE_TARGET", "test-target"),
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8(output.stderr)
            .expect("stderr should be utf-8")
            .contains("checksum mismatch for downloaded archive")
    );
    assert_eq!(
        fs::read(&installed_binary).expect("installed binary should still be readable"),
        before
    );
}

#[test]
fn self_update_does_not_downgrade_dotted_versions() {
    let workspace = unique_temp_dir("self-update-no-downgrade");
    let releases_dir = workspace.join("releases");
    let installed_binary = install_test_binary(&workspace);
    let before = fs::read(&installed_binary).expect("installed binary should be readable");
    let current_version = binary_version(&installed_binary);

    write_release_asset(&releases_dir, "test-target", "0.0.1", "older-release");

    let output = run_binary_with_env(
        &installed_binary,
        &workspace,
        ["self-update"],
        [
            (
                "TPM_INSTALL_BASE_URL",
                releases_dir
                    .to_str()
                    .expect("releases path should be utf-8"),
            ),
            ("TPM_SELF_UPDATE_TARGET", "test-target"),
        ],
    );

    assert!(
        output.status.success(),
        "self-update should succeed: {output:?}"
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        format!(
            "Current tpm {current_version} at {} is newer than available release 0.0.1\n",
            installed_binary.display()
        )
    );
    assert_eq!(
        fs::read(&installed_binary).expect("installed binary should still be readable"),
        before
    );
}

fn install_test_binary(workspace: &Path) -> PathBuf {
    let installed_binary = workspace.join("bin").join("tpm");
    if let Some(parent) = installed_binary.parent() {
        fs::create_dir_all(parent).expect("binary directory should be created");
    }
    fs::copy(env!("CARGO_BIN_EXE_tpm"), &installed_binary).expect("test binary should be copied");
    set_executable(&installed_binary);
    installed_binary
}

fn binary_version(binary: &Path) -> String {
    let output = Command::new(binary)
        .arg("--version")
        .output()
        .expect("binary should report its version");
    assert!(
        output.status.success(),
        "binary version command should succeed: {output:?}"
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    stdout
        .trim()
        .strip_prefix("tpm ")
        .unwrap_or(stdout.trim())
        .to_string()
}

fn write_release_asset(releases_dir: &Path, target: &str, version: &str, body: &str) {
    let download_dir = releases_dir.join("latest").join("download");
    fs::create_dir_all(&download_dir).expect("download directory should be created");

    let archive_bytes = release_archive_bytes(target, version, body);
    let archive_name = format!("tpm-{target}.tar.gz");
    let archive_path = download_dir.join(&archive_name);
    fs::write(&archive_path, &archive_bytes).expect("archive should be writable");

    let checksum = format!("{:x}", Sha256::digest(&archive_bytes));
    fs::write(
        download_dir.join(format!("{archive_name}.sha256")),
        format!("{checksum}\n"),
    )
    .expect("checksum should be writable");
}

fn release_archive_bytes(target: &str, version: &str, body: &str) -> Vec<u8> {
    let mut buffer = Vec::new();
    {
        let encoder = GzEncoder::new(&mut buffer, Compression::default());
        let mut archive = Builder::new(encoder);
        let script = format!(
            "#!/usr/bin/env sh\nif [ \"${{1:-}}\" = \"--version\" ] || [ \"${{1:-}}\" = \"-V\" ]; then\n  printf '%s\\n' 'tpm {version}'\n  exit 0\nfi\nprintf '%s\\n' '{body}'\n"
        );
        let archive_path = format!("tpm-{target}/tpm");
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(EntryType::Regular);
        header.set_mode(0o755);
        header.set_size(script.len() as u64);
        header.set_cksum();
        archive
            .append_data(&mut header, archive_path, script.as_bytes())
            .expect("archive data should be appended");
        let encoder = archive.into_inner().expect("archive should finish");
        encoder.finish().expect("encoder should finish");
    }
    buffer
}

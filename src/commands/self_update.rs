use std::{
    env,
    ffi::{OsStr, OsString},
    fs::{self, OpenOptions},
    io::{self, Read},
    path::{Path, PathBuf},
    process::{self, Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use tar::Archive;

use crate::{
    error::{AppError, Result},
    user_path::display_user_path,
    version::{self, VersionStatus},
};

const DEFAULT_REPO: &str = "pgilad/tpm-rs";
const INSTALL_BASE_URL_ENV: &str = "TPM_INSTALL_BASE_URL";
const INSTALL_REPO_ENV: &str = "TPM_INSTALL_REPO";
const SELF_UPDATE_TARGET_ENV: &str = "TPM_SELF_UPDATE_TARGET";

pub fn run() -> Result<()> {
    let current_executable = env::current_exe().map_err(AppError::CurrentExecutable)?;
    let current_version = version::DISPLAY_VERSION;
    let target = update_target();
    let base_url = release_base_url();
    let archive_name = format!("tpm-{target}.tar.gz");
    let archive_source = release_asset_path(&base_url, &archive_name);
    let checksum_source = release_asset_path(&base_url, &format!("{archive_name}.sha256"));

    let temp_dir = TempDir::new("self-update")?;
    let archive_path = temp_dir.path().join(&archive_name);
    let checksum_path = temp_dir.path().join(format!("{archive_name}.sha256"));

    download(&archive_source, &archive_path)?;
    download(&checksum_source, &checksum_path)?;
    verify_checksum(&archive_path, &checksum_path)?;
    extract_archive(&archive_path, temp_dir.path())?;

    let extracted_binary = temp_dir.path().join(format!("tpm-{target}")).join("tpm");
    if !extracted_binary.is_file() {
        return Err(AppError::SelfUpdate {
            message: format!(
                "downloaded archive did not contain the expected tpm-{target}/tpm path"
            ),
        });
    }

    let latest_version = installed_version(&extracted_binary)?;
    match version::compare_available_version(current_version, &latest_version) {
        VersionStatus::Same => {
            println!(
                "Already up to date tpm {current_version} at {}",
                display_user_path(&current_executable)
            );
            Ok(())
        }
        VersionStatus::CurrentIsNewer => {
            println!(
                "Current tpm {current_version} at {} is newer than available release {latest_version}",
                display_user_path(&current_executable)
            );
            Ok(())
        }
        VersionStatus::NewerAvailable => {
            replace_executable(&extracted_binary, &current_executable)?;
            println!(
                "Updated tpm from {current_version} to {latest_version} at {}",
                display_user_path(&current_executable)
            );
            Ok(())
        }
    }
}

fn update_target() -> String {
    env::var(SELF_UPDATE_TARGET_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| version::BUILD_TARGET.to_string())
}

fn release_base_url() -> String {
    if let Ok(base_url) = env::var(INSTALL_BASE_URL_ENV)
        && !base_url.trim().is_empty()
    {
        return base_url.trim_end_matches('/').to_string();
    }

    let repo = env::var(INSTALL_REPO_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_REPO.to_string());
    format!("https://github.com/{repo}/releases")
}

fn release_asset_path(base_url: &str, asset_name: &str) -> String {
    format!(
        "{}/latest/download/{asset_name}",
        base_url.trim_end_matches('/')
    )
}

fn download(source: &str, destination: &Path) -> Result<()> {
    if let Some(local_path) = local_source_path(source) {
        fs::copy(&local_path, destination).map_err(|source| AppError::SelfUpdatePath {
            path: local_path,
            source,
        })?;
        return Ok(());
    }

    match run_download_command("curl", source, destination, |command| {
        command.arg("-fsSL").arg(source).arg("-o").arg(destination);
    }) {
        DownloadCommandResult::Success => return Ok(()),
        DownloadCommandResult::NotFound => {}
        DownloadCommandResult::Failed(message) => {
            return Err(AppError::SelfUpdate { message });
        }
    }

    match run_download_command("wget", source, destination, |command| {
        command.arg("-qO").arg(destination).arg(source);
    }) {
        DownloadCommandResult::Success => Ok(()),
        DownloadCommandResult::NotFound => Err(AppError::SelfUpdate {
            message: "self-update requires curl or wget to download release assets".to_string(),
        }),
        DownloadCommandResult::Failed(message) => Err(AppError::SelfUpdate { message }),
    }
}

fn local_source_path(source: &str) -> Option<PathBuf> {
    if source.starts_with("https://") || source.starts_with("http://") {
        None
    } else if let Some(path) = source.strip_prefix("file://") {
        Some(PathBuf::from(path))
    } else {
        Some(PathBuf::from(source))
    }
}

fn run_download_command(
    program: &str,
    source: &str,
    destination: &Path,
    configure: impl FnOnce(&mut Command),
) -> DownloadCommandResult {
    let mut command = Command::new(program);
    configure(&mut command);

    match command.output() {
        Ok(output) if output.status.success() => DownloadCommandResult::Success,
        Ok(output) => DownloadCommandResult::Failed(command_failure(program, source, &output)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => DownloadCommandResult::NotFound,
        Err(error) => DownloadCommandResult::Failed(format!(
            "{program} could not download {source} to {}: {error}",
            display_user_path(destination)
        )),
    }
}

fn command_failure(program: &str, source: &str, output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let detail = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("exited with status {}", output.status)
    };

    format!("{program} failed to download {source}: {detail}")
}

fn verify_checksum(archive_path: &Path, checksum_path: &Path) -> Result<()> {
    let expected_checksum =
        fs::read_to_string(checksum_path).map_err(|source| AppError::SelfUpdatePath {
            path: checksum_path.to_path_buf(),
            source,
        })?;
    let expected_checksum = expected_checksum
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();

    if expected_checksum.is_empty() {
        return Err(AppError::SelfUpdate {
            message: format!(
                "downloaded checksum file {} was empty",
                display_user_path(checksum_path)
            ),
        });
    }

    let actual_checksum = sha256_file(archive_path)?;
    if actual_checksum != expected_checksum {
        return Err(AppError::SelfUpdate {
            message: "checksum mismatch for downloaded archive".to_string(),
        });
    }

    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).map_err(|source| AppError::SelfUpdatePath {
        path: path.to_path_buf(),
        source,
    })?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 16 * 1024];

    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|source| AppError::SelfUpdatePath {
                path: path.to_path_buf(),
                source,
            })?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }

    Ok(format!("{:x}", digest.finalize()))
}

fn extract_archive(archive_path: &Path, destination: &Path) -> Result<()> {
    let archive_file = fs::File::open(archive_path).map_err(|source| AppError::SelfUpdatePath {
        path: archive_path.to_path_buf(),
        source,
    })?;
    let decoder = GzDecoder::new(archive_file);
    let mut archive = Archive::new(decoder);
    archive
        .unpack(destination)
        .map_err(|source| AppError::SelfUpdate {
            message: format!(
                "failed to extract downloaded archive {}: {source}",
                display_user_path(archive_path)
            ),
        })
}

fn installed_version(path: &Path) -> Result<String> {
    let output = Command::new(path)
        .arg("--version")
        .output()
        .map_err(|source| AppError::SelfUpdatePath {
            path: path.to_path_buf(),
            source,
        })?;

    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(AppError::SelfUpdate {
            message: format!(
                "downloaded tpm binary at {} failed to report its version{}",
                display_user_path(path),
                if detail.is_empty() {
                    String::new()
                } else {
                    format!(": {detail}")
                }
            ),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let version = stdout.strip_prefix("tpm ").unwrap_or(&stdout).trim();
    if version.is_empty() {
        return Err(AppError::SelfUpdate {
            message: format!(
                "downloaded tpm binary at {} reported an empty version string",
                display_user_path(path)
            ),
        });
    }

    Ok(version.to_string())
}

fn replace_executable(replacement: &Path, destination: &Path) -> Result<()> {
    let parent = destination.parent().ok_or_else(|| AppError::SelfUpdate {
        message: format!(
            "current executable path {} does not have a parent directory",
            display_user_path(destination)
        ),
    })?;
    let file_name = destination
        .file_name()
        .ok_or_else(|| AppError::SelfUpdate {
            message: format!(
                "current executable path {} does not have a file name",
                display_user_path(destination)
            ),
        })?
        .to_os_string();
    let (temporary_path, mut temporary_file) =
        create_temporary_replacement_file(parent, &file_name)?;

    if let Err(error) = copy_replacement_file(replacement, &temporary_path, &mut temporary_file) {
        drop(temporary_file);
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }

    let permissions = fs::metadata(replacement)
        .map_err(|source| AppError::SelfUpdatePath {
            path: replacement.to_path_buf(),
            source,
        })?
        .permissions();
    if let Err(source) = temporary_file.set_permissions(permissions) {
        drop(temporary_file);
        let _ = fs::remove_file(&temporary_path);
        return Err(AppError::SelfUpdatePath {
            path: temporary_path,
            source,
        });
    }

    if let Err(source) = temporary_file.sync_all() {
        drop(temporary_file);
        let _ = fs::remove_file(&temporary_path);
        return Err(AppError::SelfUpdatePath {
            path: temporary_path,
            source,
        });
    }
    drop(temporary_file);

    if let Err(source) = fs::rename(&temporary_path, destination) {
        let _ = fs::remove_file(&temporary_path);
        return Err(AppError::SelfUpdate {
            message: format!(
                "failed to replace {} with the downloaded binary: {source}",
                display_user_path(destination)
            ),
        });
    }

    Ok(())
}

fn create_temporary_replacement_file(
    parent: &Path,
    file_name: &OsStr,
) -> Result<(PathBuf, fs::File)> {
    let mut last_collision = None;
    for attempt in 0..16_u8 {
        let mut temporary_name = OsString::from(".");
        temporary_name.push(file_name);
        temporary_name.push(format!(".tmp.{}", temporary_file_suffix(attempt)));
        let temporary_path = parent.join(temporary_name);

        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
        {
            Ok(file) => return Ok((temporary_path, file)),
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {
                last_collision = Some(source);
            }
            Err(source) => {
                return Err(AppError::SelfUpdatePath {
                    path: temporary_path,
                    source,
                });
            }
        }
    }

    Err(AppError::SelfUpdate {
        message: format!(
            "could not allocate a temporary file under {}: {}",
            display_user_path(parent),
            last_collision.unwrap_or_else(|| {
                io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "all temporary replacement file names collided",
                )
            })
        ),
    })
}

fn copy_replacement_file(
    replacement: &Path,
    temporary_path: &Path,
    temporary_file: &mut fs::File,
) -> Result<()> {
    let mut replacement_file =
        fs::File::open(replacement).map_err(|source| AppError::SelfUpdatePath {
            path: replacement.to_path_buf(),
            source,
        })?;

    io::copy(&mut replacement_file, temporary_file)
        .map(|_| ())
        .map_err(|source| AppError::SelfUpdatePath {
            path: temporary_path.to_path_buf(),
            source,
        })
}

fn temporary_file_suffix(attempt: u8) -> String {
    let mut bytes = [0_u8; 16];
    if fill_random_bytes(&mut bytes).is_ok() {
        return format!("{}-{attempt}", hex_bytes(&bytes));
    }

    let fallback = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("{}-{fallback}-{attempt}", process::id())
}

#[cfg(unix)]
fn fill_random_bytes(bytes: &mut [u8]) -> io::Result<()> {
    fs::File::open("/dev/urandom")?.read_exact(bytes)
}

#[cfg(not(unix))]
fn fill_random_bytes(_: &mut [u8]) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "random temporary file suffix generation is unsupported on this platform",
    ))
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

enum DownloadCommandResult {
    Success,
    NotFound,
    Failed(String),
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Result<Self> {
        let base = env::temp_dir();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after the Unix epoch")
            .as_nanos();

        for attempt in 0..16_u8 {
            let candidate = base.join(format!("tpm-{prefix}-{}-{now}-{attempt}", process::id()));
            match fs::create_dir(&candidate) {
                Ok(()) => return Ok(Self { path: candidate }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(source) => {
                    return Err(AppError::CreateDirectory {
                        path: candidate,
                        source,
                    });
                }
            }
        }

        Err(AppError::SelfUpdate {
            message: format!(
                "failed to allocate a temporary directory under {}",
                display_user_path(&base)
            ),
        })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

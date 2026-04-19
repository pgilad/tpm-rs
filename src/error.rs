use std::{io, path::PathBuf, process::ExitCode};

use thiserror::Error;

use crate::user_path::display_user_path;

pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("HOME is not set; TPM requires a home directory on supported platforms")]
    HomeDirectoryMissing,
    #[error("invalid environment variable syntax in `{value}`")]
    InvalidEnvironmentSyntax { value: String },
    #[error("environment variable `{key}` referenced by `{value}` is not set")]
    MissingEnvironmentVariable { key: String, value: String },
    #[error("failed to read config `{}`: {source}", display_user_path(path))]
    ReadConfig {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to create directory `{}`: {source}", display_user_path(path))]
    CreateDirectory {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to read tmux config `{}`: {source}", display_user_path(path))]
    ReadTmuxConfig {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to write config `{}`: {source}", display_user_path(path))]
    WriteConfig {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to inspect path `{}`: {source}", display_user_path(path))]
    InspectPath {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(
        "config `{}` does not exist; create it with `tpm migrate` or `tpm add SOURCE`",
        display_user_path(path)
    )]
    ConfigNotFound { path: PathBuf },
    #[error("failed to resolve the current working directory: {0}")]
    CurrentDirectory(#[source] io::Error),
    #[error("failed to resolve the current executable path: {0}")]
    CurrentExecutable(#[source] io::Error),
    #[error("invalid config `{}`: {message}", display_user_path(path))]
    InvalidConfig { path: PathBuf, message: String },
    #[error(
        "failed to read managed plugin manifest `{}`: {source}",
        display_user_path(path)
    )]
    ReadManifest {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(
        "failed to write managed plugin manifest `{}`: {source}",
        display_user_path(path)
    )]
    WriteManifest {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(
        "invalid managed plugin manifest `{}`: {message}",
        display_user_path(path)
    )]
    InvalidManifest { path: PathBuf, message: String },
    #[error("failed to serialize config `{}`: {source}", display_user_path(path))]
    SerializeConfig {
        path: PathBuf,
        #[source]
        source: serde_yaml::Error,
    },
    #[error("invalid plugin source `{plugin_source}`: {message}")]
    InvalidPluginSource {
        plugin_source: String,
        message: String,
    },
    #[error(
        "plugin `{plugin_source}` resolves to install directory `{install_name}` already configured by `{existing_source}`"
    )]
    PluginAlreadyConfigured {
        plugin_source: String,
        install_name: String,
        existing_source: String,
    },
    #[error("plugin `{name}` is not configured")]
    PluginNotConfigured { name: String },
    #[error("{command} reported {failing_checks} failing checks")]
    ChecksFailed {
        command: &'static str,
        failing_checks: usize,
    },
    #[error("{command} reported {failed_operations} failed operations")]
    CommandFailed {
        command: &'static str,
        failed_operations: usize,
    },
    #[error("failed to serialize JSON output: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{message}")]
    Migration { message: String },
    #[error(
        "self-update failed while accessing `{}`: {source}",
        display_user_path(path)
    )]
    SelfUpdatePath {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("self-update failed: {message}")]
    SelfUpdate { message: String },
    #[error("{command} is not implemented yet")]
    NotImplemented { command: &'static str },
}

impl AppError {
    pub fn exit_code(&self) -> ExitCode {
        match self {
            Self::InvalidConfig { .. }
            | Self::InvalidManifest { .. }
            | Self::ConfigNotFound { .. }
            | Self::InvalidPluginSource { .. }
            | Self::PluginAlreadyConfigured { .. }
            | Self::PluginNotConfigured { .. }
            | Self::InvalidEnvironmentSyntax { .. }
            | Self::MissingEnvironmentVariable { .. }
            | Self::Migration { .. } => ExitCode::from(2),
            Self::HomeDirectoryMissing => ExitCode::from(3),
            Self::ChecksFailed { .. } | Self::CommandFailed { .. } => ExitCode::from(1),
            Self::NotImplemented { .. } => ExitCode::from(4),
            Self::ReadConfig { .. }
            | Self::ReadTmuxConfig { .. }
            | Self::CreateDirectory { .. }
            | Self::WriteConfig { .. }
            | Self::InspectPath { .. }
            | Self::CurrentDirectory(_)
            | Self::CurrentExecutable(_)
            | Self::ReadManifest { .. }
            | Self::WriteManifest { .. }
            | Self::SerializeConfig { .. }
            | Self::Json(_)
            | Self::SelfUpdatePath { .. }
            | Self::SelfUpdate { .. } => ExitCode::FAILURE,
        }
    }

    pub fn should_print_stderr(&self) -> bool {
        !matches!(self, Self::ChecksFailed { .. })
    }
}

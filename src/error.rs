use std::{io, path::PathBuf, process::ExitCode};

use thiserror::Error;

pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("HOME is not set; TPM requires a home directory on supported platforms")]
    HomeDirectoryMissing,
    #[error("invalid environment variable syntax in `{value}`")]
    InvalidEnvironmentSyntax { value: String },
    #[error("environment variable `{key}` referenced by `{value}` is not set")]
    MissingEnvironmentVariable { key: String, value: String },
    #[error("failed to read config `{path}`: {source}")]
    ReadConfig {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to create directory `{path}`: {source}")]
    CreateDirectory {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to read tmux config `{path}`: {source}")]
    ReadTmuxConfig {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to write config `{path}`: {source}")]
    WriteConfig {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to inspect path `{path}`: {source}")]
    InspectPath {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("config `{path}` does not exist; create it with `tpm migrate` or `tpm add SOURCE`")]
    ConfigNotFound { path: PathBuf },
    #[error("failed to resolve the current working directory: {0}")]
    CurrentDirectory(#[source] io::Error),
    #[error("failed to resolve the current executable path: {0}")]
    CurrentExecutable(#[source] io::Error),
    #[error("invalid config `{path}`: {message}")]
    InvalidConfig { path: PathBuf, message: String },
    #[error("failed to serialize config `{path}`: {source}")]
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
    #[error("self-update failed while accessing `{path}`: {source}")]
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

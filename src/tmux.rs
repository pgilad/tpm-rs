use std::{env, ffi::OsStr, fmt, path::Path, process::Command};

#[derive(Debug, Clone)]
pub(crate) struct TmuxCommandError {
    command: String,
    detail: String,
}

impl fmt::Display for TmuxCommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.command, self.detail)
    }
}

pub(crate) fn plugin_manager_path(plugins_dir: &Path) -> String {
    let path = plugins_dir.to_string_lossy();
    if path.ends_with(std::path::MAIN_SEPARATOR) {
        path.into_owned()
    } else {
        format!("{path}{}", std::path::MAIN_SEPARATOR)
    }
}

pub(crate) fn set_global_environment(
    key: &str,
    value: &str,
) -> std::result::Result<(), TmuxCommandError> {
    if !inside_tmux() {
        return Ok(());
    }

    run(["set-environment", "-g", key, value]).map(|_| ())
}

pub(crate) fn display_message(message: &str) -> std::result::Result<(), TmuxCommandError> {
    if !inside_tmux() {
        return Ok(());
    }

    run(["display-message", message]).map(|_| ())
}

fn inside_tmux() -> bool {
    env::var_os("TMUX")
        .filter(|value| !value.is_empty())
        .is_some()
}

fn run<I, S>(args: I) -> std::result::Result<String, TmuxCommandError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args = args
        .into_iter()
        .map(|arg| arg.as_ref().to_os_string())
        .collect::<Vec<_>>();
    let command = format_command(&args);

    let output = Command::new("tmux")
        .args(&args)
        .output()
        .map_err(|source| TmuxCommandError {
            command: command.clone(),
            detail: source.to_string(),
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if output.status.success() {
        Ok(stdout)
    } else {
        let detail = if stderr.is_empty() {
            if stdout.is_empty() {
                format!("exited with status {}", output.status)
            } else {
                format!("exited with status {}: {}", output.status, stdout)
            }
        } else {
            format!("exited with status {}: {}", output.status, stderr)
        };

        Err(TmuxCommandError { command, detail })
    }
}

fn format_command(args: &[std::ffi::OsString]) -> String {
    if args.is_empty() {
        return "tmux".to_string();
    }

    format!(
        "tmux {}",
        args.iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ")
    )
}

use std::{env, io, path::Path, time::Duration};

#[derive(Debug, Clone, Copy)]
pub(crate) enum ProgressStream {
    Stdout,
    Stderr,
}

impl ProgressStream {
    pub(crate) fn write(self, message: &str) {
        match self {
            Self::Stdout => {
                print!("{message}");
                let _ = io::Write::flush(&mut io::stdout());
            }
            Self::Stderr => {
                eprint!("{message}");
                let _ = io::Write::flush(&mut io::stderr());
            }
        }
    }

    pub(crate) fn write_line(self, message: &str) {
        match self {
            Self::Stdout => println!("{message}"),
            Self::Stderr => eprintln!("{message}"),
        }
    }
}

pub(crate) fn pluralize(count: usize, singular: &str) -> String {
    if count == 1 {
        singular.to_string()
    } else {
        format!("{singular}s")
    }
}

pub(crate) fn display_user_path(path: &Path) -> String {
    let home = env::var_os("HOME").map(std::path::PathBuf::from);
    display_user_path_with_home(path, home.as_deref())
}

pub(crate) fn indent_detail(detail: &str) -> String {
    detail.replace('\n', "\n         ")
}

pub(crate) fn format_duration(duration: Duration) -> String {
    if duration.as_secs() >= 1 {
        format!("{:.1}s", duration.as_secs_f64())
    } else {
        format!("{}ms", duration.as_millis())
    }
}

fn display_user_path_with_home(path: &Path, home: Option<&Path>) -> String {
    if let Some(home) = home
        && let Ok(relative) = path.strip_prefix(home)
    {
        return if relative.as_os_str().is_empty() {
            "~".to_string()
        } else {
            format!("~/{}", relative.display())
        };
    }

    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use std::{path::Path, time::Duration};

    use super::{display_user_path_with_home, format_duration, indent_detail};

    #[test]
    fn shortens_home_prefixed_paths_for_human_output() {
        assert_eq!(
            display_user_path_with_home(
                Path::new("/Users/pgilad/.local/share/tpm/plugins"),
                Some(Path::new("/Users/pgilad"))
            ),
            "~/.local/share/tpm/plugins"
        );
    }

    #[test]
    fn leaves_non_home_paths_unchanged_for_human_output() {
        assert_eq!(
            display_user_path_with_home(
                Path::new("/tmp/tpm/plugins"),
                Some(Path::new("/Users/pgilad"))
            ),
            "/tmp/tpm/plugins"
        );
    }

    #[test]
    fn indents_multiline_failure_details() {
        assert_eq!(indent_detail("first\nsecond"), "first\n         second");
    }

    #[test]
    fn formats_short_and_long_durations() {
        assert_eq!(format_duration(Duration::from_millis(240)), "240ms");
        assert_eq!(format_duration(Duration::from_millis(1250)), "1.2s");
    }
}

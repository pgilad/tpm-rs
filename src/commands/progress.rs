use std::{io, time::Duration};

pub(crate) use crate::user_path::display_user_path;

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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{format_duration, indent_detail};

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

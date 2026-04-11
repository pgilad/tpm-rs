use std::{
    env,
    ffi::OsStr,
    io::{self, IsTerminal},
    time::Duration,
};

pub(crate) use crate::user_path::display_user_path;

#[derive(Debug, Clone, Copy)]
pub(crate) enum ProgressStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct TerminalTheme {
    color: bool,
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

    pub(crate) fn is_terminal(self) -> bool {
        match self {
            Self::Stdout => io::stdout().is_terminal(),
            Self::Stderr => io::stderr().is_terminal(),
        }
    }
}

impl TerminalTheme {
    pub(crate) fn detect(stream: ProgressStream) -> Self {
        Self {
            color: stream_supports_color(stream),
        }
    }

    pub(crate) fn success(self, text: &str) -> String {
        self.paint(text, "92")
    }

    pub(crate) fn warning(self, text: &str) -> String {
        self.paint(text, "93")
    }

    pub(crate) fn info(self, text: &str) -> String {
        self.paint(text, "96")
    }

    pub(crate) fn failure(self, text: &str) -> String {
        self.paint(text, "91")
    }

    pub(crate) fn muted(self, text: &str) -> String {
        self.paint(text, "2")
    }

    fn paint(self, text: &str, code: &str) -> String {
        if self.color {
            format!("\u{1b}[{code}m{text}\u{1b}[0m")
        } else {
            text.to_string()
        }
    }
}

fn stream_supports_color(stream: ProgressStream) -> bool {
    if env::var_os("NO_COLOR").is_some() {
        return false;
    }

    if env::var_os("CLICOLOR_FORCE").is_some_and(|value| value != OsStr::new("0")) {
        return true;
    }

    if !stream.is_terminal() {
        return false;
    }

    if env::var_os("CLICOLOR").is_some_and(|value| value == OsStr::new("0")) {
        return false;
    }

    env::var_os("TERM").is_none_or(|value| value != OsStr::new("dumb"))
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

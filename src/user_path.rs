use std::{
    env,
    path::{Path, PathBuf},
};

pub(crate) fn display_user_path(path: &Path) -> String {
    let home = env::var_os("HOME").map(PathBuf::from);
    display_user_path_with_home(path, home.as_deref())
}

fn display_user_path_with_home(path: &Path, home: Option<&Path>) -> String {
    if let Some(home) = home
        && !home.as_os_str().is_empty()
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
    use std::path::Path;

    use super::display_user_path_with_home;

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
    fn shortens_home_itself_for_human_output() {
        assert_eq!(
            display_user_path_with_home(
                Path::new("/Users/pgilad"),
                Some(Path::new("/Users/pgilad"))
            ),
            "~"
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
}

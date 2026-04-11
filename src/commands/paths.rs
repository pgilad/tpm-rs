use crate::{
    commands::{
        progress::{ProgressStream, TerminalTheme},
        resolved_paths,
    },
    error::Result,
    user_path::display_user_path,
};

pub fn run(
    config_override: Option<&std::path::Path>,
    plugins_override: Option<&std::path::Path>,
    json: bool,
) -> Result<()> {
    let paths = resolved_paths(config_override, plugins_override)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&paths)?);
        return Ok(());
    }

    let theme = TerminalTheme::detect(ProgressStream::Stdout);

    println!(
        "{} {}",
        theme.info("Config file:"),
        display_user_path(&paths.config_file)
    );
    println!(
        "{} {}",
        theme.info("Config dir: "),
        display_user_path(&paths.config_dir)
    );
    println!(
        "{} {}",
        theme.info("Data dir:   "),
        display_user_path(&paths.data_dir)
    );
    println!(
        "{} {}",
        theme.info("State dir:  "),
        display_user_path(&paths.state_dir)
    );
    println!(
        "{} {}",
        theme.info("Cache dir:  "),
        display_user_path(&paths.cache_dir)
    );
    println!(
        "{} {}",
        theme.info("Plugins dir:"),
        display_user_path(&paths.plugins_dir)
    );
    println!(
        "{} {}",
        theme.info("Config:     "),
        if paths.config_exists {
            theme.success("present")
        } else {
            theme.failure("missing")
        }
    );

    Ok(())
}

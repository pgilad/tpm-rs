use crate::{commands::resolved_paths, error::Result, user_path::display_user_path};

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

    println!("Config file: {}", display_user_path(&paths.config_file));
    println!("Config dir:  {}", display_user_path(&paths.config_dir));
    println!("Data dir:    {}", display_user_path(&paths.data_dir));
    println!("State dir:   {}", display_user_path(&paths.state_dir));
    println!("Cache dir:   {}", display_user_path(&paths.cache_dir));
    println!("Plugins dir: {}", display_user_path(&paths.plugins_dir));
    println!(
        "Config:      {}",
        if paths.config_exists {
            "present"
        } else {
            "missing"
        }
    );

    Ok(())
}

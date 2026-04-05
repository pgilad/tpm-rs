use crate::{commands::resolved_paths, error::Result};

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

    println!("Config file: {}", paths.config_file.display());
    println!("Config dir:  {}", paths.config_dir.display());
    println!("Data dir:    {}", paths.data_dir.display());
    println!("State dir:   {}", paths.state_dir.display());
    println!("Cache dir:   {}", paths.cache_dir.display());
    println!("Plugins dir: {}", paths.plugins_dir.display());
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

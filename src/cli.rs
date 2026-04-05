use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueHint};

#[derive(Debug, Parser)]
#[command(
    name = "tpm",
    version = crate::version::DISPLAY_VERSION,
    about = "A tmux plugin manager CLI",
    long_about = None
)]
pub struct Cli {
    #[arg(
        long,
        global = true,
        value_name = "PATH",
        value_hint = ValueHint::FilePath,
        help = "Override the config file path"
    )]
    pub config: Option<PathBuf>,

    #[arg(
        long = "plugins-dir",
        global = true,
        value_name = "PATH",
        value_hint = ValueHint::DirPath,
        help = "Override the plugin checkout directory"
    )]
    pub plugins_dir: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Load installed plugins inside tmux
    Load,
    /// Install configured plugins from tpm.yaml
    Install,
    /// Update installed plugins
    Update {
        #[arg(value_name = "PLUGIN")]
        plugins: Vec<String>,
    },
    /// Update the tpm CLI in place from the latest release
    #[command(name = "self-update")]
    SelfUpdate,
    /// Remove undeclared plugin directories
    Cleanup,
    /// List configured and installed plugins
    List {
        #[arg(long)]
        json: bool,
    },
    /// Validate the current TPM setup and suggest next steps when needed
    Doctor {
        #[arg(long)]
        json: bool,
    },
    /// Add a plugin to tpm.yaml, creating it if needed, and install it by default
    Add {
        source: String,
        #[arg(long, conflicts_with = "reference", help = "Track a remote branch")]
        branch: Option<String>,
        #[arg(
            long = "ref",
            conflicts_with = "branch",
            help = "Pin a tag or commit SHA"
        )]
        reference: Option<String>,
        #[arg(long, help = "Only update tpm.yaml without installing the plugin")]
        skip_install: bool,
    },
    /// Migrate plugins from a tmux config into tpm.yaml without modifying tmux.conf
    Migrate {
        #[arg(
            long = "tmux-conf",
            value_name = "PATH",
            value_hint = ValueHint::FilePath,
            help = "Read plugins from a specific tmux config file"
        )]
        tmux_conf: Option<PathBuf>,
    },
    /// Remove a plugin from tpm.yaml
    Remove { name: String },
    /// Print resolved TPM paths
    Paths {
        #[arg(long)]
        json: bool,
    },
}

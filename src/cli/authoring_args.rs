use std::path::PathBuf;

use clap::{Args, Subcommand, ValueEnum};

#[derive(Clone, Copy, Default, ValueEnum)]
pub enum AuthoringFormat {
    #[default]
    Text,
    Json,
}

#[derive(Args)]
pub struct ValidateArgs {
    /// Path to the .lua flow file
    pub flow: PathBuf,
    /// Treat validation warnings as failures
    #[arg(long)]
    pub strict: bool,
    /// Output format
    #[arg(long, value_enum, default_value = "text")]
    pub format: AuthoringFormat,
    /// Declare a top-level context key supplied by the caller (repeatable)
    #[arg(long = "context-key")]
    pub context_keys: Vec<String>,
}

#[derive(Args)]
pub struct NodesArgs {
    #[command(subcommand)]
    pub command: Option<NodesCommand>,
}

#[derive(Subcommand)]
pub enum NodesCommand {
    /// Show the installed output contract for a supported node
    Describe {
        node: String,
        #[arg(long, value_enum, default_value = "text")]
        format: AuthoringFormat,
    },
}

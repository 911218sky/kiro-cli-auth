use clap::{Parser, Subcommand};

/// Main CLI structure parsed by clap
#[derive(Parser)]
#[command(name = "kiro-cli-auth")]
#[command(about = "Kiro CLI account manager", long_about = None)]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Login and add account
    Login {
        #[arg(help = "Account alias (optional, uses email prefix if not provided)")]
        alias: Option<String>,
    },
    /// List all accounts
    List,
    /// Show current account
    Current,
    /// Remove an account
    Remove { 
        #[arg(help = "Account alias (optional, will show interactive multi-select if not provided)")]
        alias: Option<String> 
    },
    /// Switch to another account
    Switch { 
        #[arg(help = "Account alias (optional, will show interactive menu if not provided)")]
        alias: Option<String> 
    },
    /// Export account(s) to directory
    Export {
        #[arg(help = "Account alias (optional, will show interactive multi-select if not provided)")]
        alias: Option<String>,
        #[arg(short, long, help = "Output directory path")]
        output: String,
    },
    /// Import account(s) from directory
    Import {
        #[arg(help = "Input directory path")]
        file: String,
        #[arg(long, help = "Overwrite existing accounts")]
        force: bool,
    },
    /// Clean duplicate accounts
    Clean,
    /// Logout (remove local data file)
    Logout,
    /// Update account information from API
    Update {
        #[arg(help = "Account alias (optional, interactive selection if not provided)")]
        alias: Option<String>,
        #[arg(long, help = "Update all accounts without prompting")]
        all: bool,
    },
    /// Run integration tests
    Test,
    /// Update kiro-cli-auth to the latest version
    SelfUpdate {
        #[arg(long, help = "Force update even if already up to date")]
        force: bool,
    },
}

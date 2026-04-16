mod cli;
mod core;
mod ui;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Commands};
use core::commands::*;
use core::fs::FileManager;

fn main() -> Result<()> {
    let cli = Cli::parse();
    let fm = FileManager::new()?;

    // Route CLI commands to their handlers
    let result = match cli.command {
        Commands::Login { alias } => cmd_login(&fm, alias),
        Commands::List => cmd_list(&fm),
        Commands::Current => cmd_current(&fm),
        Commands::Remove { alias } => cmd_remove(&fm, alias),
        Commands::Switch { alias } => cmd_switch(&fm, alias),
        Commands::Export { alias, output } => cmd_export(&fm, alias, &output),
        Commands::Import { file, force } => cmd_import(&fm, &file, force),
        Commands::Clean => cmd_clean(&fm),
        Commands::Logout => cmd_logout(&fm),
        Commands::Update { alias, all } => cmd_update(&fm, alias, all),
        Commands::Test => cmd_test(),
        Commands::SelfUpdate { force } => cmd_self_update(force),
    };

    // Silently exit on user cancellation (Ctrl-C / Esc)
    if let Err(e) = &result {
        if e.downcast_ref::<ui::UserCancelled>().is_some() {
            return Ok(());
        }
    }

    result
}
// Trigger release


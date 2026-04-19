// Modular command structure
mod display;
mod utils;

// Command modules
mod login;
mod list;
mod current;
mod remove;
mod switch;

// Re-export command functions
pub use login::cmd_login;
pub use list::cmd_list;
pub use current::cmd_current;
pub use remove::cmd_remove;
pub use switch::cmd_switch;

// Temporary: Keep remaining commands in this file until fully extracted
use anyhow::{anyhow, Result};
use std::fs;

use crate::ui;
use crate::core::auth::token::{extract_account_info, extract_token};
use crate::core::cache::AccountCache;
use crate::core::config;
use crate::core::data::db;
use crate::core::transfer::{Exporter, Importer, Updater};
use crate::core::fs::FileManager;
use display::format_account_display;
use utils::fetch_accounts_with_usage;

pub fn cmd_export(fm: &FileManager, alias: Option<String>, output: &str) -> Result<()> {
    let conn = fm.get_db_connection()?;
    let accounts = db::list_accounts(&conn)?;
    
    if accounts.is_empty() {
        return Err(anyhow!("No accounts available"));
    }

    println!("{}", ui::yellow("⚠️  SECURITY WARNING:"));
    println!("{}", ui::yellow("   Exported files contain sensitive authentication tokens."));
    println!("{}", ui::yellow("   Keep exported files secure and delete them after use."));
    println!();

    let aliases_to_export = if let Some(a) = alias {
        vec![a]
    } else {
        let kiro_data = fm.kiro_data_path()?;
        let current_email = if kiro_data.exists() {
            extract_account_info(&kiro_data).ok().map(|(e, _)| e)
        } else {
            None
        };

        let spinner = utils::create_spinner("Fetching account info...");

        let cache_path = config::cache_db_path();
        let cache = AccountCache::new(&cache_path)?;
        let mut results = fetch_accounts_with_usage(&accounts, &kiro_data, current_email.as_ref(), &cache, false);
        spinner.finish_and_clear();

        // Sort accounts by trial expiry days (ascending)
        utils::sort_by_trial_days(&mut results);

        let items: Vec<String> = results.iter()
            .map(|(account, is_current, info_opt)| {
                format_account_display(account, *is_current, info_opt.as_ref())
            })
            .collect();
        
        let indices = ui::multi_select("Select accounts to export (Space to select, Enter to confirm)", &items)?;
        
        if indices.is_empty() {
            println!("{}", ui::yellow("No accounts selected, exporting all"));
            vec![]
        } else {
            indices.iter().map(|&i| results[i].0.alias.clone()).collect()
        }
    };

    let exporter = Exporter::new(fm.clone());
    exporter.export(&aliases_to_export, output)?;

    let count = if aliases_to_export.is_empty() { 
        accounts.len() 
    } else { 
        aliases_to_export.len() 
    };
    
    println!("{} Exported {} account(s) to {}", 
        ui::green("✓"), 
        count, 
        ui::cyan(output)
    );
    Ok(())
}

pub fn cmd_import(fm: &FileManager, file: &str, force: bool) -> Result<()> {
    let importer = Importer::new(fm.clone());
    importer.import(file, force)?;

    println!("{} Imported accounts from {}", ui::green("✓"), ui::cyan(file));
    Ok(())
}

pub fn cmd_clean(fm: &FileManager) -> Result<()> {
    let conn = fm.get_db_connection()?;
    let accounts = db::list_accounts(&conn)?;
    
    let mut invalid_snapshots = Vec::new();
    for account in &accounts {
        let snapshot_path = std::path::Path::new(&account.snapshot_path);
        if !snapshot_path.exists() {
            invalid_snapshots.push(account.alias.clone());
        } else {
            if extract_token(snapshot_path).is_err() {
                invalid_snapshots.push(account.alias.clone());
            }
        }
    }
    
    for alias in &invalid_snapshots {
        db::remove_account(&conn, alias)?;
        println!("{} Removed invalid account: {}", ui::yellow("⚠"), ui::cyan(alias));
    }
    
    let accounts = db::list_accounts(&conn)?;
    let mut seen = std::collections::HashMap::new();
    let mut to_remove = Vec::new();
    
    for account in accounts.iter().rev() {
        if seen.contains_key(&account.email) {
            to_remove.push(account.alias.clone());
        } else {
            seen.insert(account.email.clone(), true);
        }
    }
    
    for alias in &to_remove {
        db::remove_account(&conn, alias)?;
        println!("{} Removed duplicate account: {}", ui::yellow("⚠"), ui::cyan(alias));
    }
    
    let total_removed = invalid_snapshots.len() + to_remove.len();
    
    if total_removed > 0 {
        println!("\n{} Cleaned {} account(s)", ui::green("✓"), total_removed);
    } else {
        println!("{} No issues found", ui::green("✓"));
    }
    
    Ok(())
}

pub fn cmd_logout(fm: &FileManager) -> Result<()> {
    let kiro_data = fm.kiro_data_path()?;
    
    if !kiro_data.exists() {
        println!("{}", ui::yellow("Not logged in"));
        return Ok(());
    }

    if let Err(e) = fs::remove_file(&kiro_data) {
        #[cfg(target_os = "windows")]
        if e.raw_os_error() == Some(32) {
            return Err(anyhow!("Cannot logout: Kiro is running and has locked the file.\nPlease close Kiro (kiro-cli and kiro-account-manager) and try again."));
        }
        return Err(anyhow!("Failed to remove login data: {}", e));
    }
    println!("{} Logged out (local data removed)", ui::green("✓"));
    Ok(())
}

pub fn cmd_update(fm: &FileManager, alias: Option<String>, all: bool) -> Result<()> {
    let conn = fm.get_db_connection()?;
    let accounts = db::list_accounts(&conn)?;
    
    if accounts.is_empty() {
        return Err(anyhow!("No accounts available"));
    }

    let aliases_to_update = if all {
        accounts.iter().map(|a| a.alias.clone()).collect()
    } else if let Some(a) = alias {
        vec![a]
    } else {
        let items: Vec<String> = accounts.iter()
            .map(|a| format!("{} ({})", a.alias, a.email))
            .collect();
        
        let indices = ui::multi_select("Select accounts to update", &items)?;
        
        if indices.is_empty() {
            println!("{}", ui::yellow("No accounts selected"));
            return Ok(());
        }
        
        indices.iter().map(|&i| accounts[i].alias.clone()).collect()
    };

    println!("{} Updating {} account(s)...", ui::cyan("→"), aliases_to_update.len());
    
    let updater = Updater::new(fm.clone());
    let results = updater.update_multiple(&aliases_to_update)?;

    println!("\n{}", ui::bold("Update Results:"));
    println!("{}", "─".repeat(70));
    
    for result in &results {
        if result.success {
            if result.changes.is_empty() {
                println!("{} {} - No changes", ui::green("✓"), ui::cyan(&result.alias));
            } else {
                println!("{} {} - Updated:", ui::green("✓"), ui::cyan(&result.alias));
                for change in &result.changes {
                    println!("  {}", change);
                }
            }
        } else {
            println!("{} {} - {}", 
                ui::red("✗"), 
                ui::cyan(&result.alias), 
                result.error.as_ref().unwrap_or(&"Unknown error".to_string())
            );
        }
    }
    
    println!("{}", "─".repeat(70));
    
    let success_count = results.iter().filter(|r| r.success).count();
    println!("{} {}/{} accounts updated successfully\n", 
        ui::bold("Summary:"), 
        success_count, 
        results.len()
    );
    
    Ok(())
}

pub fn cmd_self_update(_force: bool) -> Result<()> {
    println!("{}", ui::yellow("Self-update command not yet refactored"));
    Ok(())
}

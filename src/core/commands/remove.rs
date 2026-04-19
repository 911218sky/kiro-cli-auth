use anyhow::{anyhow, Result};
use std::fs;

use crate::ui;
use crate::core::auth::token::extract_account_info;
use crate::core::cache::AccountCache;
use crate::core::config;
use crate::core::data::db;
use crate::core::fs::FileManager;
use super::display::format_account_display;
use super::utils::fetch_accounts_with_usage;

pub fn cmd_remove(fm: &FileManager, alias: Option<String>) -> Result<()> {
    let conn = fm.get_db_connection()?;
    let accounts = db::list_accounts(&conn)?;
    
    if accounts.is_empty() {
        return Err(anyhow!("No accounts available"));
    }

    let aliases_to_remove = if let Some(a) = alias {
        vec![a]
    } else {
        let kiro_data = fm.kiro_data_path()?;
        let current_email = if kiro_data.exists() {
            extract_account_info(&kiro_data).ok().map(|(e, _)| e)
        } else {
            None
        };

        let spinner = super::utils::create_spinner("Fetching account info...");

        let cache_path = config::cache_db_path();
        let cache = AccountCache::new(&cache_path)?;
        let mut results = fetch_accounts_with_usage(&accounts, &kiro_data, current_email.as_ref(), &cache, false);
        spinner.finish_and_clear();

        // Sort accounts by trial expiry days (ascending)
        super::utils::sort_by_trial_days(&mut results);

        let items: Vec<String> = results.iter()
            .map(|(account, is_current, info_opt)| {
                format_account_display(account, *is_current, info_opt.as_ref())
            })
            .collect();
        
        let indices = ui::multi_select("Select accounts to remove (Space to select, Enter to confirm)", &items)?;
        
        if indices.is_empty() {
            println!("{}", ui::yellow("No accounts selected"));
            return Ok(());
        }
        
        indices.iter().map(|&i| accounts[i].alias.clone()).collect()
    };

    for alias in &aliases_to_remove {
        let account = db::find_account(&conn, alias)?
            .ok_or_else(|| anyhow!("Account '{}' not found", alias))?;
        
        let snapshot_path = std::path::Path::new(&account.snapshot_path);
        if snapshot_path.exists() {
            fs::remove_file(snapshot_path)?;
        }
        
        let cache_path = config::cache_db_path();
        if let Ok(cache) = AccountCache::new(&cache_path) {
            let _ = cache.remove(&account.email);
        }
        
        db::remove_account(&conn, alias)?;
        println!("{} Account '{}' removed", ui::green("✓"), ui::cyan(alias));
    }
    
    Ok(())
}

use anyhow::Result;

use crate::ui;
use crate::core::auth::token::extract_account_info;
use crate::core::cache::AccountCache;
use crate::core::config;
use crate::core::data::db;
use crate::core::fs::FileManager;
use super::display::format_account_display;
use super::utils::fetch_accounts_with_usage;

pub fn cmd_list(fm: &FileManager, no_cache: bool) -> Result<()> {
    let conn = fm.get_db_connection()?;
    let accounts = db::list_accounts(&conn)?;
    let kiro_data = fm.kiro_data_path()?;
    
    let current_email = if kiro_data.exists() {
        extract_account_info(&kiro_data).ok().map(|(e, _)| e)
    } else {
        None
    };

    if accounts.is_empty() {
        println!("{}", ui::yellow("No accounts found"));
        return Ok(());
    }

    let spinner = super::utils::create_spinner("Fetching account info...");

    let cache_path = config::cache_db_path();
    let cache = AccountCache::new(&cache_path)?;
    let mut results = fetch_accounts_with_usage(&accounts, &kiro_data, current_email.as_ref(), &cache, no_cache);

    spinner.finish_and_clear();

    // Sort accounts by trial expiry days (ascending)
    super::utils::sort_by_trial_days(&mut results);

    println!("\n{}", ui::bold("Accounts:"));
    println!("{}", "─".repeat(90));

    for (account, is_current, info_opt) in &results {
        println!("{}", format_account_display(account, *is_current, info_opt.as_ref()));
    }

    println!("{}", "─".repeat(90));
    println!("{} {} accounts\n", ui::bold("Total:"), accounts.len());
    
    // Calculate usage statistics across all accounts
    let mut total_current: f64 = 0.0;
    let mut total_limit: f64 = 0.0;
    let mut active_accounts = 0;
    
    for (_, _, info_opt) in &results {
        if let Some(info) = info_opt {
            if !info.is_banned {
                total_current += info.current_usage;
                total_limit += info.usage_limit;
                active_accounts += 1;
            }
        }
    }
    
    // Display usage summary if there are active accounts
    if active_accounts > 0 {
        let total_remaining = total_limit - total_current;
        let total_percent = if total_limit > 0.0 {
            (total_current / total_limit * 100.0) as i32
        } else {
            0
        };
        
        println!("{}", ui::bold("Usage Summary:"));
        println!("  Active accounts: {}", ui::cyan(&active_accounts.to_string()));
        println!("  Total used:      {} / {}", 
            ui::yellow(&format!("{:.0}", total_current)),
            ui::green(&format!("{:.0}", total_limit))
        );
        println!("  Total remaining: {} ({}%)", 
            ui::green(&format!("{:.0}", total_remaining)),
            if total_percent > 90 {
                ui::red(&format!("{}", 100 - total_percent))
            } else if total_percent > 70 {
                ui::yellow(&format!("{}", 100 - total_percent))
            } else {
                ui::green(&format!("{}", 100 - total_percent))
            }
        );
        println!();
    }
    
    Ok(())
}

use anyhow::Result;
use crate::ui;
use crate::core::auth::api::get_account_info;
use crate::core::auth::token::{extract_account_info, extract_token};
use crate::core::data::db;
use crate::core::fs::FileManager;

pub fn cmd_current(fm: &FileManager) -> Result<()> {
    let kiro_data = fm.kiro_data_path()?;
    
    if !kiro_data.exists() {
        println!("{}", ui::yellow("Not logged in"));
        return Ok(());
    }

    let (email, provider) = extract_account_info(&kiro_data)?;
    let conn = fm.get_db_connection()?;
    let accounts = db::list_accounts(&conn)?;
    let account = accounts.iter().find(|a| a.email == email);
    
    println!("\n{}", ui::bold("Current Account:"));
    println!("{}", "─".repeat(50));
    
    if let Some(account) = account {
        println!("{} {}", ui::bold("Alias:"), ui::cyan(&account.alias));
    }
    
    println!("{} {}", ui::bold("Email:"), email);
    println!("{} {}", ui::bold("Provider:"), provider);
    
    if let Ok(token) = extract_token(&kiro_data) {
        if let Ok(info) = get_account_info(&token) {
            println!("{} {}", ui::bold("Subscription:"), info.subscription_type);
            println!("{} {}", ui::bold("Status:"), info.status);
            
            let percentage = if info.usage_limit > 0.0 {
                (info.current_usage / info.usage_limit * 100.0) as u32
            } else {
                0
            };
            
            let usage_display = format!("{}/{} ({}%)", 
                info.current_usage as i32, 
                info.usage_limit as i32,
                percentage
            );
            
            let colored_usage = if percentage >= 90 {
                ui::red(&usage_display)
            } else if percentage >= 70 {
                ui::yellow(&usage_display)
            } else {
                ui::green(&usage_display)
            };
            
            println!("{} {}", ui::bold("Usage:"), colored_usage);
        }
    }
    
    if account.is_none() {
        println!("\n{}", ui::yellow("⚠ Account not in registry. Run 'kiro-cli-auth login' to add it."));
    }
    
    println!("{}\n", "─".repeat(50));
    
    Ok(())
}

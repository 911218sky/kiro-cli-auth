use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use std::fs;
use std::process::Command;
use std::thread;
use std::time::Duration;
use indicatif::{ProgressBar, ProgressStyle};
use rusqlite::params;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::ui;
use crate::core::auth::api::{get_account_info, refresh_token, refresh_token_oidc, AccountInfo};
use crate::core::auth::token::{extract_account_info, extract_token, extract_refresh_token, update_token, read_aws_sso_credentials};
use crate::core::data::db;
use crate::core::transfer::{Exporter, Importer, Updater};
use crate::core::fs::FileManager;
use crate::core::models::Account;

/// Format account display string consistently across all commands
fn format_account_display(account: &Account, is_current: bool, info_opt: Option<&AccountInfo>) -> String {
    let prefix = if is_current { 
        ui::cyan("● ") 
    } else { 
        ui::cyan("○ ") 
    };
    
    let provider = match account.provider.as_str() {
        "google" => ui::magenta("[Google]"),
        "builder-id" => ui::cyan("[AWS]"),
        _ => format!("[{}]", account.provider),
    };
    
    let alias = ui::bold(&ui::cyan(&account.alias));
    let email = ui::dimmed(&account.email);
    
    if let Some(info) = info_opt {
        let sub_type = match info.subscription_type.as_str() {
            "Free" => ui::yellow(&info.subscription_type),
            "Pro" => ui::cyan(&info.subscription_type),
            "Pro+" => ui::magenta(&info.subscription_type),
            "Enterprise" => ui::green(&info.subscription_type),
            _ => info.subscription_type.clone(),
        };
        
        let status = if info.is_banned {
            ui::red("✗ Banned")
        } else {
            ui::green("✓ Active")
        };
        
        if info.usage_limit > 0.0 {
            let percentage = (info.current_usage / info.usage_limit * 100.0) as u32;
            let usage_detail = if percentage >= 90 {
                ui::red(&format!("[{}/{} ({}%)]", 
                    info.current_usage as i32, 
                    info.usage_limit as i32, 
                    percentage))
            } else if percentage >= 70 {
                ui::yellow(&format!("[{}/{} ({}%)]", 
                    info.current_usage as i32, 
                    info.usage_limit as i32, 
                    percentage))
            } else {
                ui::green(&format!("[{}/{} ({}%)]", 
                    info.current_usage as i32, 
                    info.usage_limit as i32, 
                    percentage))
            };
            
            format!("{}{} {} ({}) {} {} {}", 
                prefix, provider, alias, email, 
                sub_type, status, usage_detail)
        } else {
            format!("{}{} {} ({}) {} {}", 
                prefix, provider, alias, email, 
                sub_type, status)
        }
    } else {
        format!("{}{} {} ({})", prefix, provider, alias, email)
    }
}

/// Fetch account usage info concurrently for multiple accounts
/// Returns (Account, is_current, usage_info) tuples
fn fetch_accounts_with_usage(
    accounts: &[Account],
    kiro_data: &std::path::PathBuf,
    current_email: Option<&String>,
) -> Vec<(Account, bool, Option<crate::core::auth::api::AccountInfo>)> {
    let handles: Vec<_> = accounts.iter().map(|account| {
        let account = account.clone();
        let is_current = current_email == Some(&account.email);
        let kiro_data = kiro_data.clone();
        
        thread::spawn(move || {
            let snapshot_path = std::path::Path::new(&account.snapshot_path);
            
            // Use live data for current account, snapshot for others
            let db_path: Option<std::path::PathBuf> = if is_current && kiro_data.exists() {
                Some(kiro_data)
            } else if snapshot_path.exists() {
                Some(snapshot_path.to_path_buf())
            } else {
                None
            };

            let info = if let Some(path) = db_path {
                match extract_token(&path) {
                    Ok(token) => {
                        match get_account_info(&token) {
                            Ok(info) => Some(info),
                            Err(_) => {
                                // Token expired, try refresh
                                if let Ok(refresh_tok) = extract_refresh_token(&path) {
                                    if let Ok(refresh_resp) = refresh_token(&refresh_tok) {
                                        let new_refresh = refresh_resp.refresh_token.as_deref();
                                        if let Err(e) = update_token(&path, &refresh_resp.access_token, new_refresh) {
                                            eprintln!("warn: failed to persist refreshed token for {}: {}", path.display(), e);
                                        }
                                        get_account_info(&refresh_resp.access_token).ok()
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            }
                        }
                    },
                    Err(_) => None,
                }
            } else {
                None
            };
            
            (account, is_current, info)
        })
    }).collect();

    // Collect results, filtering out panicked threads
    handles.into_iter().filter_map(|h| match h.join() {
        Ok(result) => Some(result),
        Err(payload) => {
            let msg = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("<unknown panic>");
            eprintln!("warn: a background account-info thread panicked and was skipped: {}", msg);
            None
        }
    }).collect()
}

pub fn cmd_login(fm: &FileManager, alias: Option<String>) -> Result<()> {
    let kiro_data = fm.kiro_data_path()?;
    let conn = fm.get_db_connection()?;
    
    // Save current account if it exists and isn't already registered
    if kiro_data.exists() {
        let should_remove = true;
        
        if let Ok((current_email, _)) = extract_account_info(&kiro_data) {
            let accounts = db::list_accounts(&conn)?;
            
            if accounts.iter().all(|a| a.email != current_email) {
                let current_alias = current_email.split('@').next().unwrap_or("current").to_string();
                println!("{} Saving current account as '{}'...", ui::cyan("→"), ui::cyan(&current_alias));
                let snapshot_path = fm.account_snapshot_path(&current_alias);
                if let Some(parent) = snapshot_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(&kiro_data, &snapshot_path)?;
                
                #[cfg(unix)]
                fs::set_permissions(&snapshot_path, fs::Permissions::from_mode(0o600))?;
                
                let account = Account {
                    id: uuid::Uuid::new_v4().to_string(),
                    alias: current_alias.clone(),
                    email: current_email,
                    provider: "builder-id".to_string(), // Should correctly identify provider
                    snapshot_path: snapshot_path.to_string_lossy().to_string(),
                    created_at: Utc::now(),
                    last_used: Some(Utc::now()),
                    machine_id: Some(uuid::Uuid::new_v4().to_string().to_lowercase()),
                };
                
                db::add_account(&conn, &account)?;
                println!("{} Current account saved", ui::green("✓"));
            }
        }
        
        if should_remove {
            println!("{} Removing local login data...", ui::cyan("→"));
            if let Err(e) = fs::remove_file(&kiro_data) {
                #[cfg(target_os = "windows")]
                if e.raw_os_error() == Some(32) {
                    return Err(anyhow!("Cannot remove login data: Kiro is running and has locked the file.\nPlease close Kiro (kiro-cli and kiro-account-manager) and try again."));
                }
                return Err(anyhow!("Failed to remove login data: {}", e));
            }
        }
    }

    // Invoke kiro-cli login command
    println!("{} Logging in...", ui::cyan("→"));
    let status = Command::new("kiro-cli")
        .arg("login")
        .status()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                anyhow!("'kiro-cli' not found in PATH. Please install it first.")
            } else {
                anyhow!("Failed to run 'kiro-cli': {}", e)
            }
        })?;
    if !status.success() {
        return Err(anyhow!("'kiro-cli login' exited with status: {}", status));
    }

    thread::sleep(Duration::from_millis(500));

    if !kiro_data.exists() {
        return Err(anyhow!(
            "Login failed - kiro-cli did not create data at: {}\n\
             Hint: run 'find ~ -name data.sqlite3 2>/dev/null' to locate the actual path,\n\
             then set XDG_DATA_HOME accordingly.",
            kiro_data.display()
        ));
    }

    // Extract account info and save snapshot
    let (email, provider) = extract_account_info(&kiro_data)?;
    let final_alias = alias.unwrap_or_else(|| email.split('@').next().unwrap_or("account").to_string());

    let snapshot_path = fm.account_snapshot_path(&final_alias);
    if let Some(parent) = snapshot_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(&kiro_data, &snapshot_path)?;
    
    #[cfg(unix)]
    fs::set_permissions(&snapshot_path, fs::Permissions::from_mode(0o600))?;

    // Update or insert account record
    if let Some(existing) = db::find_account(&conn, &final_alias)? {
        let updated = Account {
            id: existing.id,
            alias: final_alias.clone(),
            email,
            provider,
            snapshot_path: snapshot_path.to_string_lossy().to_string(),
            created_at: existing.created_at,
            last_used: Some(Utc::now()),
            machine_id: existing.machine_id.or_else(|| Some(uuid::Uuid::new_v4().to_string().to_lowercase())),
        };
        db::remove_account(&conn, &final_alias)?;
        db::add_account(&conn, &updated)?;
        println!("{} Account '{}' updated", ui::green("✓"), ui::cyan(&final_alias));
    } else {
        let account = Account {
            id: uuid::Uuid::new_v4().to_string(),
            alias: final_alias.clone(),
            email,
            provider,
            snapshot_path: snapshot_path.to_string_lossy().to_string(),
            created_at: Utc::now(),
            last_used: Some(Utc::now()),
            machine_id: Some(uuid::Uuid::new_v4().to_string().to_lowercase()),
        };
        db::add_account(&conn, &account)?;
        println!("{} Account '{}' added", ui::green("✓"), ui::cyan(&final_alias));
    }
    
    Ok(())
}

pub fn cmd_list(fm: &FileManager) -> Result<()> {
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

    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_spinner())
    );
    spinner.set_message("Fetching account info...");
    spinner.enable_steady_tick(Duration::from_millis(80));

    let results = fetch_accounts_with_usage(&accounts, &kiro_data, current_email.as_ref());

    spinner.finish_and_clear();

    println!("\n{}", ui::bold("Accounts:"));
    println!("{}", "─".repeat(90));

    for (account, is_current, info_opt) in &results {
        let marker = if *is_current { "●" } else { "○" };
        
        let info_str = if let Some(info) = info_opt {
            let sub_colored = match info.subscription_type.as_str() {
                "Free" => ui::yellow(&info.subscription_type),
                "Pro" => ui::cyan(&info.subscription_type),
                "Pro+" => ui::magenta(&info.subscription_type),
                "Enterprise" => ui::green(&info.subscription_type),
                _ => info.subscription_type.clone(),
            };
            
            let status_str = if info.is_banned {
                ui::red("🚫 BANNED")
            } else {
                ui::green("✓ Active")
            };
            
            let percent = if info.usage_limit > 0.0 {
                (info.current_usage / info.usage_limit * 100.0) as i32
            } else {
                0
            };
            
            format!("{} {} [{}/{} ({}%)]", 
                sub_colored, 
                status_str,
                info.current_usage as i32,
                info.usage_limit as i32,
                percent
            )
        } else {
            let snapshot_path = std::path::Path::new(&account.snapshot_path);
            if !snapshot_path.exists() {
                ui::red("Snapshot not found")
            } else {
                ui::yellow("Token expired or invalid")
            }
        };
        
        // Platform badge
        let provider_badge = match account.provider.as_str() {
            "builder-id" => ui::cyan("[AWS]"),
            "google" => ui::magenta("[Google]"),
            _ => format!("[{}]", account.provider),
        };
        
        println!("{} {} {} ({}) {}", 
            ui::cyan(marker),
            provider_badge,
            ui::bold(&ui::cyan(&account.alias)),
            ui::dimmed(&account.email),
            info_str
        );
    }

    println!("{}", "─".repeat(90));
    println!("{} {} accounts\n", ui::bold("Total:"), accounts.len());
    
    // Calculate total usage statistics
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
    
    if let Ok(token) = extract_token(&kiro_data)
        && let Ok(info) = get_account_info(&token) {
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
    
    if account.is_none() {
        println!("\n{}", ui::yellow("⚠ Account not in registry. Run 'kiro-cli-auth login' to add it."));
    }
    
    println!("{}\n", "─".repeat(50));
    
    Ok(())
}

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

        let spinner = ProgressBar::new_spinner();
        spinner.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan} {msg}")
                .unwrap_or_else(|_| ProgressStyle::default_spinner())
        );
        spinner.set_message("Fetching account info...");
        spinner.enable_steady_tick(Duration::from_millis(80));

        let results = fetch_accounts_with_usage(&accounts, &kiro_data, current_email.as_ref());
        spinner.finish_and_clear();

        // Interactive multi-select
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
        
        db::remove_account(&conn, alias)?;
        println!("{} Account '{}' removed", ui::green("✓"), ui::cyan(alias));
    }
    
    Ok(())
}

pub fn cmd_switch(fm: &FileManager, alias: Option<String>) -> Result<()> {
    use fs2::FileExt;
    
    let conn = fm.get_db_connection()?;
    let accounts = db::list_accounts(&conn)?;
    
    if accounts.is_empty() {
        return Err(anyhow!("No accounts available"));
    }

    // Interactive selection if no alias provided
    let alias = if let Some(a) = alias {
        a
    } else {
        // Get current account
        let kiro_data = fm.kiro_data_path()?;
        let current_email = if kiro_data.exists() {
            extract_account_info(&kiro_data).ok().map(|(e, _)| e)
        } else {
            None
        };
        
        // Show loading spinner
        let spinner = ProgressBar::new_spinner();
        spinner.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan} {msg}")
                .unwrap_or_else(|_| ProgressStyle::default_spinner())
        );
        spinner.set_message("Fetching account info...");
        spinner.enable_steady_tick(Duration::from_millis(80));
        
        // Fetch usage info concurrently
        let results = fetch_accounts_with_usage(&accounts, &kiro_data, current_email.as_ref());
        
        spinner.finish_and_clear();
        
        let items: Vec<String> = results.iter()
            .map(|(account, is_current, info_opt)| {
                format_account_display(account, *is_current, info_opt.as_ref())
            })
            .collect();
        
        let selection = ui::select("Select account", &items)?;
        accounts[selection].alias.clone()
    };

    let account = db::find_account(&conn, &alias)?
        .ok_or_else(|| anyhow!("Account '{}' not found", alias))?;

    let kiro_data = fm.kiro_data_path()?;
    let lock_path = kiro_data.with_extension("lock");
    
    
    // Ensure parent directory exists (kiro-cli may not have run yet on this machine)
    if let Some(parent) = lock_path.parent() {
        fs::create_dir_all(parent)?;
    }
    // Acquire file lock to prevent concurrent switches
    let lock_file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(&lock_path)?;
    
    #[cfg(unix)]
    fs::set_permissions(&lock_path, fs::Permissions::from_mode(0o600))?;

    lock_file.lock_exclusive()
        .map_err(|e| anyhow!("Another switch operation is in progress: {}", e))?;
    
    let backup_path = fm.backup_path();
    
    // Backup current data
    if kiro_data.exists() {
        fs::copy(&kiro_data, &backup_path)?;
        
        #[cfg(unix)]
        fs::set_permissions(&backup_path, fs::Permissions::from_mode(0o600))?;
    }

    let snapshot_path = std::path::Path::new(&account.snapshot_path);
    if !snapshot_path.exists() {
        return Err(anyhow!("Snapshot not found for account '{}'", alias));
    }

    // Copy snapshot to active location
    fs::copy(snapshot_path, &kiro_data)?;
    
    #[cfg(unix)]
    fs::set_permissions(&kiro_data, fs::Permissions::from_mode(0o600))?;

    // Refresh token to avoid 403 errors
    let mut token_refreshed = false;
    // Refresh token - use OIDC for AWS Builder ID, social endpoint for Google
    let mut token_refreshed = false;
    match extract_refresh_token(&kiro_data) {
        Ok(refresh_tok) => {
            // Determine provider from snapshot
            let provider = extract_account_info(&kiro_data).map(|(_, p)| p).unwrap_or_default();
            let refresh_result = if provider == "builder-id" {
                if let Some((client_id, client_secret, region)) = read_aws_sso_credentials() {
                    refresh_token_oidc(&refresh_tok, &client_id, &client_secret, &region)
                } else {
                    // Fallback to social endpoint if SSO cache not found
                    refresh_token(&refresh_tok)
                }
            } else {
                refresh_token(&refresh_tok)
            };
            match refresh_result {
                Ok(response) => {
                    let new_refresh = response.refresh_token.as_deref();
                    if let Err(e) = update_token(&kiro_data, &response.access_token, new_refresh) {
                        println!("{} Token update failed: {}", ui::yellow("⚠"), e);
                        println!("{} You may need to re-login this account", ui::yellow("⚠"));
                    } else {
                        if let Err(e) = fs::copy(&kiro_data, snapshot_path) {
                            return Err(anyhow!("Failed to sync updated token to snapshot: {}", e));
                        }
                        #[cfg(unix)]
                        if let Err(e) = fs::set_permissions(snapshot_path, fs::Permissions::from_mode(0o600)) {
                            println!("{} Failed to set snapshot permissions: {}", ui::yellow("⚠"), e);
                        }
                        token_refreshed = true;
                    }
                }
                Err(e) => {
                    println!("{} Token refresh failed: {}", ui::yellow("⚠"), e);
                    println!("{} Account may have expired, please re-login", ui::yellow("⚠"));
                }
            }
        }
        Err(e) => {
            println!("{} Cannot extract refresh token: {}", ui::yellow("⚠"), e);
        }
    }

    db::update_last_used(&conn, &alias, Utc::now())?;

    // Sync machine ID - each account uses its own random machine ID
    let target_machine_id = if let Some(existing_id) = &account.machine_id {
        // Use existing bound machine ID
        existing_id.clone()
    } else {
        // Generate new random machine ID for new account
        let new_id = uuid::Uuid::new_v4().to_string().to_lowercase();
        // Save to database immediately
        let _ = conn.execute(
            "UPDATE accounts SET machine_id = ?1 WHERE alias = ?2",
            params![new_id.clone(), alias],
        );
        println!("{} Generated new machine ID: {}", ui::cyan("→"), new_id);
        new_id
    };
    
    match crate::core::machine_id::write_machine_id(&target_machine_id) {
        Ok(_) => {
            println!("{} Machine ID synced to system", ui::green("✓"));
        }
        Err(e) => {
            println!("{} Failed to sync machine ID to system: {}", ui::yellow("⚠"), e);
            println!("{} Machine ID saved to database only", ui::yellow("→"));
        }
    }

    // Clean up backup (contains live token; warn if removal fails)
    if let Err(e) = fs::remove_file(&backup_path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            eprintln!("warn: failed to remove token backup {:?}: {}", backup_path, e);
        }
    }

    // Release lock
    lock_file.unlock()?;

    if token_refreshed {
        println!("{} Switched to account '{}' (token refreshed)", 
            ui::green("✓"), 
            ui::cyan(&alias)
        );
    } else {
        println!("{} Switched to account '{}'", ui::green("✓"), ui::cyan(&alias));
    }
    
    Ok(())
}


pub fn cmd_export(fm: &FileManager, alias: Option<String>, output: &str) -> Result<()> {
    let conn = fm.get_db_connection()?;
    let accounts = db::list_accounts(&conn)?;
    
    if accounts.is_empty() {
        return Err(anyhow!("No accounts available"));
    }

    // Security warning
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

        let spinner = ProgressBar::new_spinner();
        spinner.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan} {msg}")
                .unwrap_or_else(|_| ProgressStyle::default_spinner())
        );
        spinner.set_message("Fetching account info...");
        spinner.enable_steady_tick(Duration::from_millis(80));

        let results = fetch_accounts_with_usage(&accounts, &kiro_data, current_email.as_ref());
        spinner.finish_and_clear();

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
    
    // Remove accounts with missing or invalid snapshots
    let mut invalid_snapshots = Vec::new();
    for account in &accounts {
        let snapshot_path = std::path::Path::new(&account.snapshot_path);
        if !snapshot_path.exists() {
            invalid_snapshots.push(account.alias.clone());
        } else {
            // Verify snapshot is readable (contains valid token)
            if extract_token(snapshot_path).is_err() {
                invalid_snapshots.push(account.alias.clone());
            }
        }
    }
    
    for alias in &invalid_snapshots {
        db::remove_account(&conn, alias)?;
        println!("{} Removed invalid account: {}", ui::yellow("⚠"), ui::cyan(alias));
    }
    
    // Deduplicate by email, keeping the most recent
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

pub fn cmd_test() -> Result<()> {
    println!("{}", ui::cyan("Running integration tests..."));
    let mut passed = 0;
    let mut failed = 0;
    
    // Test 1: DB CRUD
    print!("  {} DB CRUD operations... ", ui::cyan("→"));
    match test_db_crud() {
        Ok(_) => { println!("{}", ui::green("✓")); passed += 1; }
        Err(e) => { println!("{} {}", ui::red("✗"), e); failed += 1; }
    }
    
    // Test 2: Migration
    print!("  {} JSON to SQLite migration... ", ui::cyan("→"));
    match test_migration() {
        Ok(_) => { println!("{}", ui::green("✓")); passed += 1; }
        Err(e) => { println!("{} {}", ui::red("✗"), e); failed += 1; }
    }
    
    // Test 3: Account switching
    print!("  {} Account switching... ", ui::cyan("→"));
    match test_switch_accounts() {
        Ok(_) => { println!("{}", ui::green("✓")); passed += 1; }
        Err(e) => { println!("{} {}", ui::red("✗"), e); failed += 1; }
    }
    
    println!();
    if failed == 0 {
        println!("{} All tests passed ({}/{})", ui::green("✓"), passed, passed + failed);
        Ok(())
    } else {
        Err(anyhow!("{} tests failed, {} passed", failed, passed))
    }
}

fn test_db_crud() -> Result<()> {
    use tempfile::tempdir;
    let dir = tempdir()?;
    let db_path = dir.path().join("test.db");
    let conn = db::init_db(&db_path)?;
    
    let account = Account {
        id: "id1".to_string(),
        alias: "work".to_string(),
        email: "work@example.com".to_string(),
        provider: "google".to_string(),
        snapshot_path: "/path/to/work.sqlite3".to_string(),
        created_at: Utc::now(),
        last_used: Some(Utc::now()),
        machine_id: None,
    };
    
    db::add_account(&conn, &account)?;
    let found = db::find_account(&conn, "work")?;
    if found.is_none() || found.as_ref().map(|a| a.email.as_str()) != Some("work@example.com") {
        return Err(anyhow!("Account not found or email mismatch"));
    }
    
    let accounts = db::list_accounts(&conn)?;
    if accounts.len() != 1 {
        return Err(anyhow!("Expected 1 account, found {}", accounts.len()));
    }
    
    db::update_last_used(&conn, "work", Utc::now())?;
    let removed = db::remove_account(&conn, "work")?;
    if !removed {
        return Err(anyhow!("Failed to remove account"));
    }
    
    Ok(())
}

fn test_migration() -> Result<()> {
    use tempfile::tempdir;
    use crate::core::data::migration;
    
    let dir = tempdir()?;
    let json_content = r#"{
        "version": "1.0.0",
        "accounts": [{
            "id": "id1",
            "alias": "legacy",
            "email": "legacy@example.com",
            "provider": "google",
            "snapshot_path": "/path/to/legacy.sqlite3",
            "created_at": "2024-01-01T00:00:00Z",
            "last_used": null
        }]
    }"#;
    
    fs::write(dir.path().join("registry.json"), json_content)?;
    let migrated = migration::migrate_from_json_if_needed(dir.path())?;
    if !migrated {
        return Err(anyhow!("Migration did not run"));
    }
    
    let db_path = dir.path().join("registry.db");
    let conn = db::init_db(&db_path)?;
    let accounts = db::list_accounts(&conn)?;
    
    if accounts.len() != 1 || accounts[0].alias != "legacy" {
        return Err(anyhow!("Migration failed: expected 1 account with alias 'legacy'"));
    }
    
    Ok(())
}

fn test_switch_accounts() -> Result<()> {
    use tempfile::tempdir;
    use rusqlite::Connection;
    
    let dir = tempdir()?;
    let base_dir = dir.path().join(".kiro-cli-auth");
    let accounts_dir = base_dir.join("accounts");
    let kiro_data = dir.path().join("data.sqlite3");
    
    fs::create_dir_all(&accounts_dir)?;
    
    let work_snapshot = accounts_dir.join("work.sqlite3");
    let personal_snapshot = accounts_dir.join("personal.sqlite3");
    
    create_mock_kiro_data(&work_snapshot, "work@example.com")?;
    create_mock_kiro_data(&personal_snapshot, "personal@example.com")?;
    
    let fm = FileManager::new_with_base(base_dir.clone())?;
    let conn = fm.get_db_connection()?;
    
    let work_account = Account {
        id: "id1".to_string(),
        alias: "work".to_string(),
        email: "work@example.com".to_string(),
        provider: "google".to_string(),
        snapshot_path: work_snapshot.to_string_lossy().to_string(),
        created_at: Utc::now(),
        last_used: None,
        machine_id: None,
    };
    
    let personal_account = Account {
        id: "id2".to_string(),
        alias: "personal".to_string(),
        email: "personal@example.com".to_string(),
        provider: "github".to_string(),
        snapshot_path: personal_snapshot.to_string_lossy().to_string(),
        created_at: Utc::now(),
        last_used: None,
        machine_id: None,
    };
    
    db::add_account(&conn, &work_account)?;
    db::add_account(&conn, &personal_account)?;
    
    fs::copy(&work_snapshot, &kiro_data)?;
    let conn_kiro = Connection::open(&kiro_data)?;
    let mut stmt = conn_kiro.prepare("SELECT value FROM auth WHERE key = 'account_info'")?;
    let account_info: String = stmt.query_row([], |row| row.get(0))?;
    if !account_info.contains("work@example.com") {
        return Err(anyhow!("Expected work account, got: {}", account_info));
    }
    
    fs::copy(&personal_snapshot, &kiro_data)?;
    let conn_kiro2 = Connection::open(&kiro_data)?;
    let mut stmt2 = conn_kiro2.prepare("SELECT value FROM auth WHERE key = 'account_info'")?;
    let account_info2: String = stmt2.query_row([], |row| row.get(0))?;
    if !account_info2.contains("personal@example.com") {
        return Err(anyhow!("Expected personal account, got: {}", account_info2));
    }
    
    Ok(())
}

fn create_mock_kiro_data(path: &std::path::PathBuf, email: &str) -> Result<()> {
    use rusqlite::Connection;
    
    let conn = Connection::open(path)?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS auth (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )",
        [],
    )?;
    
    conn.execute(
        "INSERT OR REPLACE INTO auth (key, value) VALUES ('access_token', 'mock_token_123')",
        [],
    )?;
    
    let account_info = format!("{{\"email\":\"{}\",\"provider\":\"google\"}}", email);
    conn.execute(
        "INSERT OR REPLACE INTO auth (key, value) VALUES ('account_info', ?1)",
        [&account_info],
    )?;
    
    Ok(())
}

pub fn cmd_self_update(force: bool) -> Result<()> {
    println!("{}", ui::cyan("→ Checking for updates..."));
    
    let api_url = "https://api.github.com/repos/911218sky/kiro-cli-auth/releases/latest";
    let response = ureq::get(api_url)
        .set("User-Agent", "kiro-cli-auth")
        .call()
        .context("Failed to fetch latest release info")?;
    
    let release: serde_json::Value = response.into_json()
        .context("Failed to parse release info")?;
    
    let latest_version = release["tag_name"].as_str()
        .ok_or_else(|| anyhow!("No tag_name in release"))?;
    
    let current_version = env!("CARGO_PKG_VERSION");
    let latest_version_str = latest_version.trim_start_matches('v');
    
    println!("{} Current version: {}", ui::cyan("→"), current_version);
    println!("{} Latest version: {}", ui::cyan("→"), latest_version_str);
    
    // Simple version comparison: split by '.' and compare numerically
    let current_parts: Vec<u32> = current_version.split('.').filter_map(|s| s.parse().ok()).collect();
    let latest_parts: Vec<u32> = latest_version_str.split('.').filter_map(|s| s.parse().ok()).collect();

    let expected_parts = 3usize;
    if current_parts.len() != expected_parts {
        eprintln!("warn: current version '{}' has unexpected format; update check may be inaccurate", current_version);
    }
    if latest_parts.len() != expected_parts {
        eprintln!("warn: latest version '{}' has unexpected format; update check may be inaccurate", latest_version_str);
    }
    
    if !force {
        let is_newer = latest_parts.iter().zip(current_parts.iter())
            .find(|(l, c)| l != c)
            .map(|(l, c)| l > c)
            .unwrap_or(latest_parts.len() > current_parts.len());
        
        if !is_newer {
            println!("{}", ui::green("✓ Already up to date"));
            return Ok(());
        }
    } else {
        println!("{}", ui::yellow("⚠ Force update enabled, skipping version check"));
    }
    
    // Determine platform-specific asset name
    let asset_name = if cfg!(target_os = "linux") {
        if cfg!(target_arch = "x86_64") {
            "kiro-cli-auth-linux-x86_64"
        } else if cfg!(target_arch = "aarch64") {
            "kiro-cli-auth-linux-aarch64"
        } else {
            return Err(anyhow!("Unsupported Linux architecture"));
        }
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "x86_64") {
            "kiro-cli-auth-macos-x86_64"
        } else if cfg!(target_arch = "aarch64") {
            "kiro-cli-auth-macos-aarch64"
        } else {
            return Err(anyhow!("Unsupported macOS architecture"));
        }
    } else if cfg!(target_os = "windows") {
        "kiro-cli-auth-windows.exe"
    } else {
        return Err(anyhow!("Unsupported platform"));
    };
    
    let assets = release["assets"].as_array()
        .ok_or_else(|| anyhow!("No assets in release"))?;
    
    let download_url = assets.iter()
        .find(|a| a["name"].as_str() == Some(asset_name))
        .and_then(|a| a["browser_download_url"].as_str())
        .ok_or_else(|| anyhow!("Asset {} not found in release", asset_name))?;
    
    println!("{} Downloading {}...", ui::cyan("→"), asset_name);
    
    let response = ureq::get(download_url)
        .call()
        .context("Failed to download binary")?;
    
    let mut temp_file = tempfile::NamedTempFile::new()
        .context("Failed to create temp file")?;
    
    std::io::copy(&mut response.into_reader(), &mut temp_file)
        .context("Failed to write downloaded binary")?;
    
    let temp_path = temp_file.path();
    
    let current_exe = std::env::current_exe()
        .context("Failed to get current executable path")?;
    
    println!("{} Installing to {:?}...", ui::cyan("→"), current_exe);
    
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(temp_path, std::fs::Permissions::from_mode(0o755))
            .context("Failed to set executable permission")?;
    }
    
    let backup_path = current_exe.with_extension("old");

    // Remove stale backup if exists
    if backup_path.exists() {
        if let Err(e) = std::fs::remove_file(&backup_path) {
            eprintln!("warn: could not remove stale backup {:?}: {}", backup_path, e);
        }
    }

    // Backup current executable
    std::fs::rename(&current_exe, &backup_path)
        .context("Failed to backup current executable")?;

    // Install new binary; rollback on failure
    if let Err(copy_err) = std::fs::copy(temp_path, &current_exe) {
        eprintln!("error: failed to install new executable: {}", copy_err);
        if let Err(restore_err) = std::fs::rename(&backup_path, &current_exe) {
            eprintln!("error: rollback also failed — please manually restore {:?} to {:?}", backup_path, current_exe);
            return Err(anyhow!("Install failed and rollback failed: install={}, rollback={}", copy_err, restore_err));
        }
        return Err(anyhow!("Failed to install new executable (rolled back): {}", copy_err));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&current_exe, std::fs::Permissions::from_mode(0o755))
            .context("Failed to set executable permission")?;
    }

    if let Err(e) = std::fs::remove_file(&backup_path) {
        eprintln!("warn: could not remove backup {:?}: {}", backup_path, e);
    }
    
    println!("{} Successfully updated to {}", ui::green("✓"), latest_version);
    println!("{} Please restart kiro-cli-auth to use the new version", ui::yellow("⚠"));
    
    Ok(())
}

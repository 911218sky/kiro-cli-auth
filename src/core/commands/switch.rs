use anyhow::{anyhow, Result};
use chrono::Utc;
use fs2::FileExt;
use rusqlite::params;
use std::fs;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::ui;
use crate::core::auth::api::{refresh_token, refresh_token_oidc};
use crate::core::auth::token::{extract_account_info, extract_refresh_token, update_token, read_aws_sso_credentials};
use crate::core::cache::AccountCache;
use crate::core::config;
use crate::core::data::db;
use crate::core::fs::FileManager;
use super::display::format_account_display;
use super::utils::fetch_accounts_with_usage;

pub fn cmd_switch(fm: &FileManager, alias: Option<String>) -> Result<()> {
    let conn = fm.get_db_connection()?;
    let accounts = db::list_accounts(&conn)?;
    
    if accounts.is_empty() {
        return Err(anyhow!("No accounts available"));
    }

    let alias = if let Some(a) = alias {
        a
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
        
        let selection = ui::select("Select account", &items)?;
        accounts[selection].alias.clone()
    };

    let account = db::find_account(&conn, &alias)?
        .ok_or_else(|| anyhow!("Account '{}' not found", alias))?;

    let kiro_data = fm.kiro_data_path()?;
    let lock_path = kiro_data.with_extension("lock");
    
    if let Some(parent) = lock_path.parent() {
        fs::create_dir_all(parent)?;
    }
    
    let lock_file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(&lock_path)?;
    
    #[cfg(unix)]
    fs::set_permissions(&lock_path, fs::Permissions::from_mode(0o600))?;

    lock_file.lock_exclusive()
        .map_err(|e| anyhow!("Another switch operation is in progress: {}", e))?;
    
    let backup_path = fm.backup_path();
    
    if kiro_data.exists() {
        fs::copy(&kiro_data, &backup_path)?;
        
        #[cfg(unix)]
        fs::set_permissions(&backup_path, fs::Permissions::from_mode(0o600))?;
    }

    let snapshot_path = std::path::Path::new(&account.snapshot_path);
    if !snapshot_path.exists() {
        return Err(anyhow!("Snapshot not found for account '{}'", alias));
    }

    fs::copy(snapshot_path, &kiro_data)?;
    
    #[cfg(unix)]
    fs::set_permissions(&kiro_data, fs::Permissions::from_mode(0o600))?;

    let mut token_refreshed = false;
    match extract_refresh_token(&kiro_data) {
        Ok(refresh_tok) => {
            let provider = extract_account_info(&kiro_data).map(|(_, p)| p).unwrap_or_default();
            let refresh_result = if provider == "builder-id" {
                if let Some((client_id, client_secret, region)) = read_aws_sso_credentials() {
                    refresh_token_oidc(&refresh_tok, &client_id, &client_secret, &region)
                } else {
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

    let target_machine_id = if let Some(existing_id) = &account.machine_id {
        existing_id.clone()
    } else {
        let new_id = uuid::Uuid::new_v4().to_string().to_lowercase();
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

    if let Err(e) = fs::remove_file(&backup_path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            eprintln!("warn: failed to remove token backup {:?}: {}", backup_path, e);
        }
    }

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

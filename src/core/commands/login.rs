use anyhow::{anyhow, Result};
use chrono::Utc;
use std::fs;
use std::process::Command;
use std::thread;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::ui;
use crate::core::auth::token::extract_account_info;
use crate::core::config;
use crate::core::data::db;
use crate::core::fs::FileManager;
use crate::core::models::Account;

pub fn cmd_login(fm: &FileManager, alias: Option<String>) -> Result<()> {
    let kiro_data = fm.kiro_data_path()?;
    let conn = fm.get_db_connection()?;
    
    if kiro_data.exists() {
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
                fs::set_permissions(&snapshot_path, fs::Permissions::from_mode(config::FILE_PERMISSIONS))?;
                
                let account = Account {
                    id: uuid::Uuid::new_v4().to_string(),
                    alias: current_alias.clone(),
                    email: current_email,
                    provider: "builder-id".to_string(),
                    snapshot_path: snapshot_path.to_string_lossy().to_string(),
                    created_at: Utc::now(),
                    last_used: Some(Utc::now()),
                    machine_id: Some(uuid::Uuid::new_v4().to_string().to_lowercase()),
                };
                
                db::add_account(&conn, &account)?;
                println!("{} Current account saved", ui::green("✓"));
            }
        }
        
        println!("{} Removing local login data...", ui::cyan("→"));
        if let Err(e) = fs::remove_file(&kiro_data) {
            #[cfg(target_os = "windows")]
            if e.raw_os_error() == Some(32) {
                return Err(anyhow!("Cannot remove login data: Kiro is running and has locked the file.\nPlease close Kiro (kiro-cli and kiro-account-manager) and try again."));
            }
            return Err(anyhow!("Failed to remove login data: {}", e));
        }
    }

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

    thread::sleep(config::login_delay());

    if !kiro_data.exists() {
        return Err(anyhow!(
            "Login failed - kiro-cli did not create data at: {}\n\
             Hint: run 'find ~ -name data.sqlite3 2>/dev/null' to locate the actual path,\n\
             then set XDG_DATA_HOME accordingly.",
            kiro_data.display()
        ));
    }

    let (email, provider) = extract_account_info(&kiro_data)?;
    let final_alias = alias.unwrap_or_else(|| email.split('@').next().unwrap_or("account").to_string());

    let snapshot_path = fm.account_snapshot_path(&final_alias);
    if let Some(parent) = snapshot_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(&kiro_data, &snapshot_path)?;
    
    #[cfg(unix)]
    fs::set_permissions(&snapshot_path, fs::Permissions::from_mode(0o600))?;

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

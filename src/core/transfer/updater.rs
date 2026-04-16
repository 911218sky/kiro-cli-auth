use anyhow::Result;
use crate::core::auth::api::get_account_info;
use crate::core::auth::token::extract_token;
use crate::core::data::db;
use crate::core::fs::FileManager;
use crate::core::models::Account;
use chrono::Utc;
use std::thread;
use std::time::Duration;

pub struct Updater {
    file_manager: FileManager,
}

#[derive(Debug)]
pub struct UpdateResult {
    pub alias: String,
    pub success: bool,
    pub changes: Vec<String>,
    pub error: Option<String>,
}

impl Updater {
    pub fn new(file_manager: FileManager) -> Self {
        Self { file_manager }
    }

    /// Update a single account by fetching fresh info from API and comparing with stored data.
    pub fn update_account(&self, alias: &str) -> UpdateResult {
        let conn = match self.file_manager.get_db_connection() {
            Ok(c) => c,
            Err(e) => return UpdateResult {
                alias: alias.to_string(),
                success: false,
                changes: vec![],
                error: Some(format!("Database connection failed: {}", e)),
            },
        };

        let account = match db::find_account(&conn, alias) {
            Ok(Some(a)) => a,
            Ok(None) => return UpdateResult {
                alias: alias.to_string(),
                success: false,
                changes: vec![],
                error: Some(format!("Account '{}' not found", alias)),
            },
            Err(e) => return UpdateResult {
                alias: alias.to_string(),
                success: false,
                changes: vec![],
                error: Some(format!("Database error: {}", e)),
            },
        };

        let snapshot_path = self.file_manager.account_snapshot_path(&account.alias);
        if !snapshot_path.exists() {
            return UpdateResult {
                alias: alias.to_string(),
                success: false,
                changes: vec![],
                error: Some("Snapshot file not found".to_string()),
            };
        }

        let token = match extract_token(&snapshot_path) {
            Ok(t) => t,
            Err(e) => return UpdateResult {
                alias: alias.to_string(),
                success: false,
                changes: vec![],
                error: Some(format!("Token extraction failed: {}", e)),
            },
        };

        let info = match get_account_info(&token) {
            Ok(i) => i,
            Err(e) => return UpdateResult {
                alias: alias.to_string(),
                success: false,
                changes: vec![],
                error: Some(format!("API call failed: {}", e)),
            },
        };

        let mut changes = Vec::new();

        // If email changed, remove old record and re-add with new email
        if account.email != info.email {
            changes.push(format!("Email: {} → {}", account.email, info.email));
            let updated = Account {
                email: info.email,
                ..account
            };
            if let Err(e) = db::remove_account(&conn, alias) {
                return UpdateResult {
                    alias: alias.to_string(),
                    success: false,
                    changes: vec![],
                    error: Some(format!("Failed to update: {}", e)),
                };
            }
            if let Err(e) = db::add_account(&conn, &updated) {
                return UpdateResult {
                    alias: alias.to_string(),
                    success: false,
                    changes: vec![],
                    error: Some(format!("Failed to update: {}", e)),
                };
            }
        }

        if let Err(e) = db::update_last_used(&conn, alias, Utc::now()) {
            return UpdateResult {
                alias: alias.to_string(),
                success: false,
                changes,
                error: Some(format!("Failed to update last_used: {}", e)),
            };
        }

        UpdateResult {
            alias: alias.to_string(),
            success: true,
            changes,
            error: None,
        }
    }

    /// Update multiple accounts with a progress bar and rate limiting.
    pub fn update_multiple(&self, aliases: &[String]) -> Result<Vec<UpdateResult>> {
        use indicatif::{ProgressBar, ProgressStyle};
        
        let pb = ProgressBar::new(aliases.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.cyan} [{bar:40.cyan/blue}] {pos}/{len} {msg}")
                .unwrap()
                .progress_chars("#>-")
        );

        let mut results = Vec::new();

        for alias in aliases {
            pb.set_message(format!("Updating {}", alias));
            let result = self.update_account(alias);
            results.push(result);
            pb.inc(1);
            thread::sleep(Duration::from_millis(500));
        }

        pb.finish_with_message("Done");
        Ok(results)
    }
}


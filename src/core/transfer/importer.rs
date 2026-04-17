use anyhow::Result;
use std::fs;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::core::data::db;
use crate::core::fs::FileManager;

pub struct Importer {
    file_manager: FileManager,
}

impl Importer {
    pub fn new(file_manager: FileManager) -> Self {
        Self { file_manager }
    }

    /// Import accounts from an export directory.
    /// If force=true, overwrites existing accounts with the same alias.
    pub fn import(&self, import_dir: &str, force: bool) -> Result<()> {
        let import_dir_path = Path::new(import_dir);
        let canonical_import = fs::canonicalize(import_dir_path)
            .map_err(|_| anyhow::anyhow!("Invalid import directory path"))?;
        
        let import_registry = canonical_import.join("registry.db");
        if !import_registry.exists() {
            anyhow::bail!("registry.db not found in import directory");
        }

        let import_accounts_dir = canonical_import.join("accounts");
        if !import_accounts_dir.exists() {
            anyhow::bail!("accounts directory not found in import directory");
        }

        // Read accounts from the import registry.db
        let import_conn = db::init_db(&import_registry)?;
        let import_accounts = db::list_accounts(&import_conn)?;

        if import_accounts.is_empty() {
            anyhow::bail!("No accounts found in import file");
        }

        let local_conn = self.file_manager.get_db_connection()?;

        for account in import_accounts {
            // Sanitize alias to prevent path traversal: only allow alphanumeric, dash, underscore, dot
            if !account.alias.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.') {
                eprintln!("⚠️  Skipping account with unsafe alias: '{}'", account.alias);
                continue;
            }

            // Skip if account exists and force is not set
            if db::find_account(&local_conn, &account.alias)?.is_some() && !force {
                eprintln!("⚠️  Account '{}' already exists, skipping (use --force to overwrite)", account.alias);
                continue;
            }

            let import_snapshot = import_accounts_dir.join(format!("{}.sqlite3", account.alias));
            if !import_snapshot.exists() {
                eprintln!("⚠️  Snapshot file not found for '{}', skipping", account.alias);
                continue;
            }

            let local_snapshot = self.file_manager.account_snapshot_path(&account.alias);
            if let Some(parent) = local_snapshot.parent() {
                fs::create_dir_all(parent)?;
            }

            fs::copy(&import_snapshot, &local_snapshot)?;
            
            #[cfg(unix)]
            fs::set_permissions(&local_snapshot, fs::Permissions::from_mode(0o600))?;

            // Remove existing account if force mode is enabled
            if force {
                db::remove_account(&local_conn, &account.alias)?;
            }
            
            // Update snapshot_path to use local path instead of imported absolute path
            let mut local_account = account.clone();
            local_account.snapshot_path = local_snapshot.to_string_lossy().to_string();
            
            db::add_account(&local_conn, &local_account)?;
            println!("✅ Imported account: {}", account.alias);
        }

        Ok(())
    }
}


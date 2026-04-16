use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::core::data::db;
use crate::core::fs::FileManager;

pub struct Exporter {
    file_manager: FileManager,
}

impl Exporter {
    pub fn new(file_manager: FileManager) -> Self {
        Self { file_manager }
    }

    /// Export registry.db and account snapshots to a directory.
    /// If aliases is empty, exports all accounts.
    pub fn export(&self, aliases: &[String], output_dir: &str) -> Result<()> {
        let conn = self.file_manager.get_db_connection()?;
        let all_accounts = db::list_accounts(&conn)?;
        
        let accounts = if aliases.is_empty() {
            all_accounts
        } else {
            aliases.iter()
                .map(|a| db::find_account(&conn, a)?
                    .context(format!("Account '{}' not found", a)))
                .collect::<Result<Vec<_>>>()?
        };

        if accounts.is_empty() {
            anyhow::bail!("No accounts to export");
        }

        fs::create_dir_all(output_dir)?;
        let canonical_output = fs::canonicalize(output_dir)
            .map_err(|_| anyhow::anyhow!("Invalid output directory path"))?;
        
        // Copy registry.db with restricted permissions
        let registry_db = self.file_manager.registry_db_path();
        let dest_registry = canonical_output.join("registry.db");
        fs::copy(&registry_db, &dest_registry)?;
        
        #[cfg(unix)]
        fs::set_permissions(&dest_registry, fs::Permissions::from_mode(0o600))?;

        // Copy account snapshots
        let accounts_dir = canonical_output.join("accounts");
        fs::create_dir_all(&accounts_dir)?;
        
        #[cfg(unix)]
        fs::set_permissions(&accounts_dir, fs::Permissions::from_mode(0o700))?;

        for account in &accounts {
            let snapshot_path = Path::new(&account.snapshot_path);
            if !snapshot_path.exists() {
                eprintln!("⚠️  Snapshot not found for '{}', skipping", account.alias);
                continue;
            }

            let dest_snapshot = accounts_dir.join(format!("{}.sqlite3", account.alias));
            fs::copy(snapshot_path, &dest_snapshot)?;
            
            #[cfg(unix)]
            fs::set_permissions(&dest_snapshot, fs::Permissions::from_mode(0o600))?;
        }

        Ok(())
    }
}


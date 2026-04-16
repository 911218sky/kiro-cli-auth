use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use rusqlite::Connection;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::core::data::db;

#[derive(Clone)]
pub struct FileManager {
    base_dir: PathBuf,
}

impl FileManager {
    pub fn new() -> Result<Self> {
        // Allow override via env var, otherwise use .kiro-cli-auth next to executable
        let base_dir = if let Ok(custom_path) = std::env::var("KIRO_CLI_AUTH_DIR") {
            PathBuf::from(custom_path)
        } else {
            let exe_path = std::env::current_exe().context("Cannot get executable path")?;
            let exe_dir = exe_path.parent().context("Cannot get executable directory")?;
            exe_dir.join(".kiro-cli-auth")
        };
        
        if !base_dir.exists() {
            fs::create_dir_all(&base_dir)?;
            fs::create_dir_all(base_dir.join("accounts"))?;
            
            // Restrict permissions to owner-only on Unix systems
            #[cfg(unix)]
            {
                fs::set_permissions(&base_dir, fs::Permissions::from_mode(0o700))?;
                fs::set_permissions(&base_dir.join("accounts"), fs::Permissions::from_mode(0o700))?;
            }
        }
        
        // Migrate legacy registry.json → registry.db; warn if it fails so the
        // user knows their old accounts may not have been imported.
        if let Err(e) = crate::core::data::migration::migrate_from_json_if_needed(&base_dir) {
            eprintln!("warn: failed to migrate legacy registry.json: {}", e);
        }
        
        Ok(Self { base_dir })
    }

    pub fn new_with_base(base_dir: PathBuf) -> Result<Self> {
        if !base_dir.exists() {
            fs::create_dir_all(&base_dir)?;
            fs::create_dir_all(base_dir.join("accounts"))?;

            #[cfg(unix)]
            {
                fs::set_permissions(&base_dir, fs::Permissions::from_mode(0o700))?;
                fs::set_permissions(&base_dir.join("accounts"), fs::Permissions::from_mode(0o700))?;
            }
        }
        Ok(Self { base_dir })
    }

    pub fn registry_db_path(&self) -> PathBuf {
        self.base_dir.join("registry.db")
    }

    pub fn accounts_dir(&self) -> PathBuf {
        self.base_dir.join("accounts")
    }

    pub fn get_db_connection(&self) -> Result<Connection> {
        let db_path = self.registry_db_path();
        db::init_db(&db_path)
    }

    /// Locate the main kiro-cli data.sqlite3 by checking multiple standard paths.
    /// Returns the first existing file, or the XDG default if none exist yet.
    pub fn kiro_data_path(&self) -> Result<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory ($HOME is not set)"))?;

        let xdg_candidate = std::env::var("XDG_DATA_HOME").ok()
            .filter(|x| !x.is_empty())
            .map(|x| PathBuf::from(x).join("kiro-cli/data.sqlite3"));

        let candidates: &[Option<PathBuf>] = &[
            xdg_candidate,
            Some(home.join(".local/share/kiro-cli/data.sqlite3")),
            Some(home.join(".config/kiro-cli/data.sqlite3")),
            Some(home.join(".kiro-cli/data.sqlite3")),
        ];

        for candidate in candidates.iter().flatten() {
            if candidate.exists() {
                return Ok(candidate.clone());
            }
        }

        // Default fallback (file may not exist yet — login will create it)
        Ok(home.join(".local/share/kiro-cli/data.sqlite3"))
    }

    pub fn account_snapshot_path(&self, alias: &str) -> PathBuf {
        self.accounts_dir().join(format!("{}.sqlite3", alias))
    }

    pub fn backup_path(&self) -> PathBuf {
        self.base_dir.join("current_backup.sqlite3")
    }
}

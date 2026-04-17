use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use chrono::{DateTime, Utc};

use crate::core::data::db;
use crate::core::models::Account;

/// Legacy JSON registry format (pre-SQLite)
#[derive(Debug, Serialize, Deserialize)]
struct LegacyRegistry {
    version: String,
    accounts: Vec<LegacyAccount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LegacyAccount {
    id: String,
    alias: String,
    email: String,
    provider: String,
    snapshot_path: String,
    created_at: DateTime<Utc>,
    last_used: Option<DateTime<Utc>>,
}

/// Migrate from JSON registry to SQLite if JSON exists and DB doesn't
/// Returns true if migration was performed
pub fn migrate_from_json_if_needed(base_dir: &Path) -> Result<bool> {
    let json_path = base_dir.join("registry.json");
    let db_path = base_dir.join("registry.db");
    
    // Skip if JSON doesn't exist or DB already exists
    if !json_path.exists() || db_path.exists() {
        return Ok(false);
    }
    
    println!("🔄 Detected legacy registry.json, migrating to SQLite...");
    
    // Read legacy JSON
    let json_content = fs::read_to_string(&json_path)
        .context("Failed to read registry.json")?;
    
    let legacy_registry: LegacyRegistry = serde_json::from_str(&json_content)
        .context("Failed to parse registry.json")?;
    
    // Create new SQLite database
    let conn = db::init_db(&db_path)?;
    
    // Migrate all accounts with updated local paths
    for legacy_account in legacy_registry.accounts {
        // Recalculate snapshot_path to use current device's base_dir
        let local_snapshot_path = base_dir
            .join("accounts")
            .join(format!("{}.sqlite3", legacy_account.alias));
        
        let account = Account {
            id: legacy_account.id,
            alias: legacy_account.alias.clone(),
            email: legacy_account.email,
            provider: legacy_account.provider,
            snapshot_path: local_snapshot_path.to_string_lossy().to_string(),
            created_at: legacy_account.created_at,
            last_used: legacy_account.last_used,
            machine_id: None,
        };
        
        db::add_account(&conn, &account)?;
        println!("  ✓ Migrated account: {}", account.alias);
    }
    
    // Backup old JSON
    let backup_path = base_dir.join("registry.json.bak");
    fs::rename(&json_path, &backup_path)?;
    
    println!("✅ Migration complete! Old file backed up to registry.json.bak");
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_migration_success() {
        let dir = tempdir().unwrap();
        
        // Create old JSON file
        let json_content = r#"{
            "version": "1.0.0",
            "accounts": [
                {
                    "id": "id1",
                    "alias": "work",
                    "email": "work@example.com",
                    "provider": "google",
                    "snapshot_path": "/path/to/work.sqlite3",
                    "created_at": "2024-01-01T00:00:00Z",
                    "last_used": null
                },
                {
                    "id": "id2",
                    "alias": "personal",
                    "email": "personal@example.com",
                    "provider": "github",
                    "snapshot_path": "/path/to/personal.sqlite3",
                    "created_at": "2024-01-02T00:00:00Z",
                    "last_used": "2024-01-03T00:00:00Z"
                }
            ]
        }"#;
        
        let json_path = dir.path().join("registry.json");
        fs::write(&json_path, json_content).unwrap();
        
        // Execute migration
        let migrated = migrate_from_json_if_needed(dir.path()).unwrap();
        assert!(migrated);
        
        // Verify DB is created
        let db_path = dir.path().join("registry.db");
        assert!(db_path.exists());
        
        // Verify data is migrated
        let conn = db::init_db(&db_path).unwrap();
        let accounts = db::list_accounts(&conn).unwrap();
        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[0].alias, "personal"); // Has last_used, should be sorted by last_used DESC
        assert_eq!(accounts[1].alias, "work");
        
        // Verify JSON is backed up
        assert!(!json_path.exists());
        assert!(dir.path().join("registry.json.bak").exists());
    }

    #[test]
    fn test_no_migration_if_json_not_exists() {
        let dir = tempdir().unwrap();
        
        let migrated = migrate_from_json_if_needed(dir.path()).unwrap();
        assert!(!migrated);
    }

    #[test]
    fn test_no_migration_if_db_exists() {
        let dir = tempdir().unwrap();
        
        // Create JSON and DB
        let json_path = dir.path().join("registry.json");
        fs::write(&json_path, r#"{"version":"1.0.0","accounts":[]}"#).unwrap();
        
        let db_path = dir.path().join("registry.db");
        db::init_db(&db_path).unwrap();
        
        // Should not migrate
        let migrated = migrate_from_json_if_needed(dir.path()).unwrap();
        assert!(!migrated);
        
        // JSON should still exist
        assert!(json_path.exists());
    }

    #[test]
    fn test_migration_invalid_json() {
        let dir = tempdir().unwrap();
        
        let json_path = dir.path().join("registry.json");
        fs::write(&json_path, "invalid json").unwrap();
        
        let result = migrate_from_json_if_needed(dir.path());
        assert!(result.is_err());
    }
}

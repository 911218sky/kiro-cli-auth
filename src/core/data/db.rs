use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use chrono::{DateTime, Utc};
use std::path::Path;

use crate::core::models::Account;

/// Initialize SQLite database with accounts table and indexes
pub fn init_db(db_path: &Path) -> Result<Connection> {
    let conn = Connection::open(db_path)
        .context("Failed to open database")?;
    
    conn.execute(
        "CREATE TABLE IF NOT EXISTS accounts (
            id TEXT PRIMARY KEY,
            alias TEXT UNIQUE NOT NULL,
            email TEXT NOT NULL,
            provider TEXT NOT NULL,
            snapshot_path TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            last_used INTEGER,
            machine_id TEXT
        )",
        [],
    )
    .context("Failed to create accounts table")?;
    
    // Add machine_id column for legacy databases (silently fails if exists)
    let _ = conn.execute("ALTER TABLE accounts ADD COLUMN machine_id TEXT", []);
    
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_alias ON accounts(alias)",
        [],
    )
    .context("Failed to create index on alias")?;
    
    Ok(conn)
}

pub fn add_account(conn: &Connection, account: &Account) -> Result<()> {
    conn.execute(
        "INSERT INTO accounts (id, alias, email, provider, snapshot_path, created_at, last_used, machine_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            account.id,
            account.alias,
            account.email,
            account.provider,
            account.snapshot_path,
            account.created_at.timestamp(),
            account.last_used.map(|t| t.timestamp()),
            account.machine_id,
        ],
    )
    .context("Failed to insert account")?;
    Ok(())
}

pub fn find_account(conn: &Connection, alias: &str) -> Result<Option<Account>> {
    let mut stmt = conn.prepare(
        "SELECT id, alias, email, provider, snapshot_path, created_at, last_used, machine_id
         FROM accounts WHERE alias = ?1"
    )
    .context("Failed to prepare find account query")?;
    
    let mut rows = stmt.query(params![alias])
        .context("Failed to execute find account query")?;
    
    if let Some(row) = rows.next()? {
        let created_ts: i64 = row.get(5)?;
        let last_used_ts: Option<i64> = row.get(6)?;
        
        Ok(Some(Account {
            id: row.get(0)?,
            alias: row.get(1)?,
            email: row.get(2)?,
            provider: row.get(3)?,
            snapshot_path: row.get(4)?,
            created_at: DateTime::from_timestamp(created_ts, 0)
                .ok_or_else(|| anyhow::anyhow!("Invalid timestamp"))?,
            last_used: last_used_ts.and_then(|t| DateTime::from_timestamp(t, 0)),
            machine_id: row.get(7)?,
        }))
    } else {
        Ok(None)
    }
}

/// List all accounts, sorted by last_used DESC then created_at DESC
pub fn list_accounts(conn: &Connection) -> Result<Vec<Account>> {
    let mut stmt = conn.prepare(
        "SELECT id, alias, email, provider, snapshot_path, created_at, last_used, machine_id
         FROM accounts ORDER BY last_used DESC, created_at DESC"
    )
    .context("Failed to prepare list accounts query")?;
    
    let accounts = stmt.query_map([], |row| {
        let created_ts: i64 = row.get(5)?;
        let last_used_ts: Option<i64> = row.get(6)?;
        
        Ok(Account {
            id: row.get(0)?,
            alias: row.get(1)?,
            email: row.get(2)?,
            provider: row.get(3)?,
            snapshot_path: row.get(4)?,
            created_at: DateTime::from_timestamp(created_ts, 0)
                .ok_or_else(|| rusqlite::Error::InvalidQuery)?,
            last_used: last_used_ts.and_then(|t| DateTime::from_timestamp(t, 0)),
            machine_id: row.get(7)?,
        })
    })
    .context("Failed to execute list accounts query")?
    .collect::<Result<Vec<_>, _>>()
    .context("Failed to collect account results")?;
    
    Ok(accounts)
}

pub fn remove_account(conn: &Connection, alias: &str) -> Result<bool> {
    let rows = conn.execute("DELETE FROM accounts WHERE alias = ?1", params![alias])
        .context("Failed to delete account")?;
    Ok(rows > 0)
}

pub fn update_last_used(conn: &Connection, alias: &str, timestamp: DateTime<Utc>) -> Result<()> {
    conn.execute(
        "UPDATE accounts SET last_used = ?1 WHERE alias = ?2",
        params![timestamp.timestamp(), alias],
    )
    .context("Failed to update last_used timestamp")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_init_db() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let conn = init_db(&db_path).unwrap();
        
        // Verify table exists
        let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='accounts'").unwrap();
        let exists = stmt.exists([]).unwrap();
        assert!(exists);
    }

    #[test]
    fn test_add_and_find_account() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let conn = init_db(&db_path).unwrap();

        let account = Account {
            id: "test_id".to_string(),
            alias: "test".to_string(),
            email: "test@example.com".to_string(),
            provider: "google".to_string(),
            snapshot_path: "/path/to/snapshot".to_string(),
            created_at: Utc::now(),
            last_used: None,
            machine_id: None,
        };

        add_account(&conn, &account).unwrap();
        
        let found = find_account(&conn, "test").unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.email, "test@example.com");
        assert_eq!(found.provider, "google");
    }

    #[test]
    fn test_list_accounts() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let conn = init_db(&db_path).unwrap();

        let account1 = Account {
            id: "id1".to_string(),
            alias: "work".to_string(),
            email: "work@example.com".to_string(),
            provider: "google".to_string(),
            snapshot_path: "/path/1".to_string(),
            created_at: Utc::now(),
            last_used: Some(Utc::now()),
            machine_id: None,
        };

        let account2 = Account {
            id: "id2".to_string(),
            alias: "personal".to_string(),
            email: "personal@example.com".to_string(),
            provider: "github".to_string(),
            snapshot_path: "/path/2".to_string(),
            created_at: Utc::now(),
            last_used: None,
            machine_id: None,
        };

        add_account(&conn, &account1).unwrap();
        add_account(&conn, &account2).unwrap();

        let accounts = list_accounts(&conn).unwrap();
        assert_eq!(accounts.len(), 2);
        // Should be sorted by last_used DESC
        assert_eq!(accounts[0].alias, "work");
    }

    #[test]
    fn test_remove_account() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let conn = init_db(&db_path).unwrap();

        let account = Account {
            id: "test_id".to_string(),
            alias: "test".to_string(),
            email: "test@example.com".to_string(),
            provider: "google".to_string(),
            snapshot_path: "/path/to/snapshot".to_string(),
            created_at: Utc::now(),
            last_used: None,
            machine_id: None,
        };

        add_account(&conn, &account).unwrap();
        
        let removed = remove_account(&conn, "test").unwrap();
        assert!(removed);

        let not_found = find_account(&conn, "test").unwrap();
        assert!(not_found.is_none());
        
        // Removing again should return false
        let removed_again = remove_account(&conn, "test").unwrap();
        assert!(!removed_again);
    }

    #[test]
    fn test_update_last_used() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let conn = init_db(&db_path).unwrap();

        let account = Account {
            id: "test_id".to_string(),
            alias: "test".to_string(),
            email: "test@example.com".to_string(),
            provider: "google".to_string(),
            snapshot_path: "/path/to/snapshot".to_string(),
            created_at: Utc::now(),
            last_used: None,
            machine_id: None,
        };

        add_account(&conn, &account).unwrap();
        
        let new_time = Utc::now();
        update_last_used(&conn, "test", new_time).unwrap();

        let found = find_account(&conn, "test").unwrap().unwrap();
        assert!(found.last_used.is_some());
    }

    #[test]
    fn test_duplicate_alias() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let conn = init_db(&db_path).unwrap();

        let account1 = Account {
            id: "id1".to_string(),
            alias: "test".to_string(),
            email: "test1@example.com".to_string(),
            provider: "google".to_string(),
            snapshot_path: "/path/1".to_string(),
            created_at: Utc::now(),
            last_used: None,
            machine_id: None,
        };

        let account2 = Account {
            id: "id2".to_string(),
            alias: "test".to_string(),
            email: "test2@example.com".to_string(),
            provider: "github".to_string(),
            snapshot_path: "/path/2".to_string(),
            created_at: Utc::now(),
            last_used: None,
            machine_id: None,
        };

        add_account(&conn, &account1).unwrap();
        let result = add_account(&conn, &account2);
        assert!(result.is_err()); // Should fail because alias is UNIQUE
    }
}

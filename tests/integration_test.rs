use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;
use rusqlite::Connection;
use chrono::Utc;

use kiro_cli_auth::core::data::{db, migration};
use kiro_cli_auth::core::fs::FileManager;
use kiro_cli_auth::core::models::Account;

// Mock Kiro CLI data file
fn create_mock_kiro_data(path: &PathBuf, email: &str) {
    let conn = Connection::open(path).unwrap();
    conn.execute(
        "CREATE TABLE IF NOT EXISTS auth (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )",
        [],
    ).unwrap();
    
    conn.execute(
        "INSERT OR REPLACE INTO auth (key, value) VALUES ('access_token', 'mock_token_123')",
        [],
    ).unwrap();
    
    let account_info = format!("{{\"email\":\"{}\",\"provider\":\"google\"}}", email);
    conn.execute(
        "INSERT OR REPLACE INTO auth (key, value) VALUES ('account_info', ?1)",
        [&account_info],
    ).unwrap();
}

#[test]
fn test_db_crud() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let conn = db::init_db(&db_path).unwrap();
    
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
    
    db::add_account(&conn, &account).unwrap();
    
    let found = db::find_account(&conn, "work").unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().email, "work@example.com");
    
    let accounts = db::list_accounts(&conn).unwrap();
    assert_eq!(accounts.len(), 1);
    
    db::update_last_used(&conn, "work", Utc::now()).unwrap();
    
    let removed = db::remove_account(&conn, "work").unwrap();
    assert!(removed);
}

#[test]
fn test_migration() {
    let dir = tempdir().unwrap();
    
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
    
    fs::write(dir.path().join("registry.json"), json_content).unwrap();
    
    let migrated = migration::migrate_from_json_if_needed(dir.path()).unwrap();
    assert!(migrated);
    
    let db_path = dir.path().join("registry.db");
    let conn = db::init_db(&db_path).unwrap();
    let accounts = db::list_accounts(&conn).unwrap();
    
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].alias, "legacy");
}

#[test]
fn test_switch_accounts() {
    let dir = tempdir().unwrap();
    let base_dir = dir.path().join(".kiro-cli-auth");
    let accounts_dir = base_dir.join("accounts");
    let kiro_data = dir.path().join("data.sqlite3");
    
    fs::create_dir_all(&accounts_dir).unwrap();
    
    // Create two account snapshots
    let work_snapshot = accounts_dir.join("work.sqlite3");
    let personal_snapshot = accounts_dir.join("personal.sqlite3");
    
    create_mock_kiro_data(&work_snapshot, "work@example.com");
    create_mock_kiro_data(&personal_snapshot, "personal@example.com");
    
    // Initialize database and add accounts
    let fm = FileManager::new_with_base(base_dir.clone()).unwrap();
    let conn = fm.get_db_connection().unwrap();
    
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
    
    db::add_account(&conn, &work_account).unwrap();
    db::add_account(&conn, &personal_account).unwrap();
    
    // Test switching to work
    fs::copy(&work_snapshot, &kiro_data).unwrap();
    
    // Verify current account is work
    let conn_kiro = Connection::open(&kiro_data).unwrap();
    let mut stmt = conn_kiro.prepare("SELECT value FROM auth WHERE key = 'account_info'").unwrap();
    let account_info: String = stmt.query_row([], |row| row.get(0)).unwrap();
    assert!(account_info.contains("work@example.com"));
    
    // Switch to personal
    fs::copy(&personal_snapshot, &kiro_data).unwrap();
    
    // Verify current account is personal
    let conn_kiro2 = Connection::open(&kiro_data).unwrap();
    let mut stmt2 = conn_kiro2.prepare("SELECT value FROM auth WHERE key = 'account_info'").unwrap();
    let account_info2: String = stmt2.query_row([], |row| row.get(0)).unwrap();
    assert!(account_info2.contains("personal@example.com"));
    
    println!("✓ Switch accounts test passed");
}

use anyhow::Result;
use kiro_cli_auth::core::data::db;
use kiro_cli_auth::core::fs::FileManager;
use kiro_cli_auth::core::models::Account;
use kiro_cli_auth::core::transfer::{Exporter, Importer};
use std::fs;
use tempfile::tempdir;
use chrono::Utc;

#[test]
fn test_export_and_import_basic() -> Result<()> {
    let source_dir = tempdir()?;
    let export_dir = tempdir()?;
    let target_dir = tempdir()?;

    // Setup source account
    let source_fm = FileManager::new_with_base(source_dir.path().to_path_buf())?;
    let conn = source_fm.get_db_connection()?;
    
    let snapshot_path = source_fm.account_snapshot_path("work");
    
    // Ensure accounts directory exists
    if let Some(parent) = snapshot_path.parent() {
        fs::create_dir_all(parent)?;
    }
    
    let account = Account {
        id: "test-id".to_string(),
        alias: "work".to_string(),
        email: "work@example.com".to_string(),
        provider: "google".to_string(),
        snapshot_path: snapshot_path.to_string_lossy().to_string(),
        created_at: Utc::now(),
        last_used: None,
        machine_id: Some("machine-123".to_string()),
    };
    
    // Create snapshot file
    fs::write(&account.snapshot_path, "test data")?;
    db::add_account(&conn, &account)?;

    // Export
    let exporter = Exporter::new(source_fm.clone());
    exporter.export(&[], export_dir.path().to_str().unwrap())?;

    // Import to different location
    let target_fm = FileManager::new_with_base(target_dir.path().to_path_buf())?;
    let importer = Importer::new(target_fm.clone());
    importer.import(export_dir.path().to_str().unwrap(), false)?;

    // Verify: path should be updated to target location
    let target_conn = target_fm.get_db_connection()?;
    let imported = db::find_account(&target_conn, "work")?.unwrap();
    
    let expected_path = target_fm.account_snapshot_path("work");
    assert_eq!(imported.snapshot_path, expected_path.to_string_lossy().to_string());
    assert_eq!(imported.email, "work@example.com");
    assert!(expected_path.exists());

    Ok(())
}

#[test]
fn test_import_force_overwrite() -> Result<()> {
    let source_dir = tempdir()?;
    let export_dir = tempdir()?;
    let target_dir = tempdir()?;

    // Setup source
    let source_fm = FileManager::new_with_base(source_dir.path().to_path_buf())?;
    let conn = source_fm.get_db_connection()?;
    
    let snapshot_path = source_fm.account_snapshot_path("work");
    if let Some(parent) = snapshot_path.parent() {
        fs::create_dir_all(parent)?;
    }
    
    let account = Account {
        id: "new-id".to_string(),
        alias: "work".to_string(),
        email: "new@example.com".to_string(),
        provider: "google".to_string(),
        snapshot_path: snapshot_path.to_string_lossy().to_string(),
        created_at: Utc::now(),
        last_used: None,
        machine_id: None,
    };
    
    fs::write(&account.snapshot_path, "new data")?;
    db::add_account(&conn, &account)?;

    // Export
    let exporter = Exporter::new(source_fm);
    exporter.export(&[], export_dir.path().to_str().unwrap())?;

    // Create existing account in target
    let target_fm = FileManager::new_with_base(target_dir.path().to_path_buf())?;
    let target_conn = target_fm.get_db_connection()?;
    
    let old_snapshot_path = target_fm.account_snapshot_path("work");
    if let Some(parent) = old_snapshot_path.parent() {
        fs::create_dir_all(parent)?;
    }
    
    let old_account = Account {
        id: "old-id".to_string(),
        alias: "work".to_string(),
        email: "old@example.com".to_string(),
        provider: "google".to_string(),
        snapshot_path: old_snapshot_path.to_string_lossy().to_string(),
        created_at: Utc::now(),
        last_used: None,
        machine_id: None,
    };
    
    fs::write(&old_account.snapshot_path, "old data")?;
    db::add_account(&target_conn, &old_account)?;

    // Import with force
    let importer = Importer::new(target_fm.clone());
    importer.import(export_dir.path().to_str().unwrap(), true)?;

    // Verify overwrite
    let imported = db::find_account(&target_conn, "work")?.unwrap();
    assert_eq!(imported.email, "new@example.com");
    assert_eq!(imported.id, "new-id");
    
    let content = fs::read_to_string(&imported.snapshot_path)?;
    assert_eq!(content, "new data");

    Ok(())
}

#[test]
fn test_import_without_force_skips_existing() -> Result<()> {
    let source_dir = tempdir()?;
    let export_dir = tempdir()?;
    let target_dir = tempdir()?;

    // Setup and export
    let source_fm = FileManager::new_with_base(source_dir.path().to_path_buf())?;
    let conn = source_fm.get_db_connection()?;
    
    let snapshot_path = source_fm.account_snapshot_path("work");
    if let Some(parent) = snapshot_path.parent() {
        fs::create_dir_all(parent)?;
    }
    
    let account = Account {
        id: "new-id".to_string(),
        alias: "work".to_string(),
        email: "new@example.com".to_string(),
        provider: "google".to_string(),
        snapshot_path: snapshot_path.to_string_lossy().to_string(),
        created_at: Utc::now(),
        last_used: None,
        machine_id: None,
    };
    
    fs::write(&account.snapshot_path, "new data")?;
    db::add_account(&conn, &account)?;

    let exporter = Exporter::new(source_fm);
    exporter.export(&[], export_dir.path().to_str().unwrap())?;

    // Create existing account in target
    let target_fm = FileManager::new_with_base(target_dir.path().to_path_buf())?;
    let target_conn = target_fm.get_db_connection()?;
    
    let old_snapshot_path = target_fm.account_snapshot_path("work");
    if let Some(parent) = old_snapshot_path.parent() {
        fs::create_dir_all(parent)?;
    }
    
    let old_account = Account {
        id: "old-id".to_string(),
        alias: "work".to_string(),
        email: "old@example.com".to_string(),
        provider: "google".to_string(),
        snapshot_path: old_snapshot_path.to_string_lossy().to_string(),
        created_at: Utc::now(),
        last_used: None,
        machine_id: None,
    };
    
    fs::write(&old_account.snapshot_path, "old data")?;
    db::add_account(&target_conn, &old_account)?;

    // Import without force - should skip
    let importer = Importer::new(target_fm.clone());
    importer.import(export_dir.path().to_str().unwrap(), false)?;

    // Verify old account remains
    let result = db::find_account(&target_conn, "work")?.unwrap();
    assert_eq!(result.email, "old@example.com");
    assert_eq!(result.id, "old-id");

    Ok(())
}

#[test]
fn test_export_missing_snapshot_warning() -> Result<()> {
    let source_dir = tempdir()?;
    let export_dir = tempdir()?;

    let source_fm = FileManager::new_with_base(source_dir.path().to_path_buf())?;
    let conn = source_fm.get_db_connection()?;
    
    let account = Account {
        id: "test-id".to_string(),
        alias: "missing".to_string(),
        email: "test@example.com".to_string(),
        provider: "google".to_string(),
        snapshot_path: source_fm.account_snapshot_path("missing").to_string_lossy().to_string(),
        created_at: Utc::now(),
        last_used: None,
        machine_id: None,
    };
    
    // Don't create snapshot file
    db::add_account(&conn, &account)?;

    // Export should succeed but warn
    let exporter = Exporter::new(source_fm);
    let result = exporter.export(&[], export_dir.path().to_str().unwrap());
    assert!(result.is_ok());

    Ok(())
}

#[test]
fn test_import_path_sanitization() -> Result<()> {
    let source_dir = tempdir()?;
    let export_dir = tempdir()?;
    let target_dir = tempdir()?;

    // Create malicious account with path traversal alias
    let source_fm = FileManager::new_with_base(source_dir.path().to_path_buf())?;
    let conn = source_fm.get_db_connection()?;
    
    let snapshot_path = source_fm.account_snapshot_path("evil");
    if let Some(parent) = snapshot_path.parent() {
        fs::create_dir_all(parent)?;
    }
    
    let malicious_account = Account {
        id: "evil-id".to_string(),
        alias: "../../../etc/passwd".to_string(),
        email: "evil@example.com".to_string(),
        provider: "google".to_string(),
        snapshot_path: snapshot_path.to_string_lossy().to_string(),
        created_at: Utc::now(),
        last_used: None,
        machine_id: None,
    };
    
    fs::write(&malicious_account.snapshot_path, "evil data")?;
    db::add_account(&conn, &malicious_account)?;

    // Export
    let exporter = Exporter::new(source_fm);
    exporter.export(&[], export_dir.path().to_str().unwrap())?;

    // Import should skip malicious alias
    let target_fm = FileManager::new_with_base(target_dir.path().to_path_buf())?;
    let importer = Importer::new(target_fm.clone());
    importer.import(export_dir.path().to_str().unwrap(), false)?;

    // Verify malicious account was not imported
    let target_conn = target_fm.get_db_connection()?;
    let result = db::find_account(&target_conn, "../../../etc/passwd")?;
    assert!(result.is_none());

    Ok(())
}

#[test]
fn test_import_cross_platform_windows_to_linux() -> Result<()> {
    use rusqlite::Connection;
    
    let export_dir = tempdir()?;
    let target_dir = tempdir()?;

    // Manually create a Windows-style export
    let export_registry = export_dir.path().join("registry.db");
    let conn = Connection::open(&export_registry)?;
    
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
    )?;
    
    // Insert account with Windows path
    conn.execute(
        "INSERT INTO accounts (id, alias, email, provider, snapshot_path, created_at, last_used, machine_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            "win-id",
            "work",
            "work@example.com",
            "google",
            "C:\\Users\\user\\.kiro-cli-auth\\accounts\\work.sqlite3",  // Windows path
            chrono::Utc::now().timestamp(),
            None::<i64>,
            Some("win-machine-123"),
        ],
    )?;
    drop(conn);

    // Create snapshot file
    let accounts_dir = export_dir.path().join("accounts");
    fs::create_dir_all(&accounts_dir)?;
    fs::write(accounts_dir.join("work.sqlite3"), "windows data")?;

    // Import to Linux-style target
    let target_fm = FileManager::new_with_base(target_dir.path().to_path_buf())?;
    let importer = Importer::new(target_fm.clone());
    importer.import(export_dir.path().to_str().unwrap(), false)?;

    // Verify path was converted to Linux style
    let target_conn = target_fm.get_db_connection()?;
    let imported = db::find_account(&target_conn, "work")?.unwrap();
    
    // Path should be updated to local path, not the original Windows absolute path
    let expected_path = target_fm.account_snapshot_path("work");
    assert_eq!(imported.snapshot_path, expected_path.to_string_lossy().to_string());
    assert!(!imported.snapshot_path.contains("C:\\Users\\user\\.kiro-cli-auth"));
    assert!(expected_path.exists());

    
    let content = fs::read_to_string(&imported.snapshot_path)?;
    assert_eq!(content, "windows data");

    Ok(())
}

#[test]
fn test_import_cross_platform_linux_to_windows_style() -> Result<()> {
    use rusqlite::Connection;
    
    let export_dir = tempdir()?;
    let target_dir = tempdir()?;

    // Manually create a Linux-style export
    let export_registry = export_dir.path().join("registry.db");
    let conn = Connection::open(&export_registry)?;
    
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
    )?;
    
    // Insert account with Linux path
    conn.execute(
        "INSERT INTO accounts (id, alias, email, provider, snapshot_path, created_at, last_used, machine_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            "linux-id",
            "personal",
            "personal@example.com",
            "google",
            "/home/user/.kiro-cli-auth/accounts/personal.sqlite3",  // Linux path
            chrono::Utc::now().timestamp(),
            None::<i64>,
            Some("linux-machine-456"),
        ],
    )?;
    drop(conn);

    // Create snapshot file
    let accounts_dir = export_dir.path().join("accounts");
    fs::create_dir_all(&accounts_dir)?;
    fs::write(accounts_dir.join("personal.sqlite3"), "linux data")?;

    // Import to target (will use local path style)
    let target_fm = FileManager::new_with_base(target_dir.path().to_path_buf())?;
    let importer = Importer::new(target_fm.clone());
    importer.import(export_dir.path().to_str().unwrap(), false)?;

    // Verify path was converted to local style
    let target_conn = target_fm.get_db_connection()?;
    let imported = db::find_account(&target_conn, "personal")?.unwrap();
    
    // Path should be updated to local path, not Linux path
    let expected_path = target_fm.account_snapshot_path("personal");
    assert_eq!(imported.snapshot_path, expected_path.to_string_lossy().to_string());
    assert!(!imported.snapshot_path.starts_with("/home/"));
    assert!(expected_path.exists());
    
    let content = fs::read_to_string(&imported.snapshot_path)?;
    assert_eq!(content, "linux data");

    Ok(())
}

#[test]
fn test_import_cross_platform_macos_paths() -> Result<()> {
    use rusqlite::Connection;
    
    let export_dir = tempdir()?;
    let target_dir = tempdir()?;

    // Manually create a macOS-style export
    let export_registry = export_dir.path().join("registry.db");
    let conn = Connection::open(&export_registry)?;
    
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
    )?;
    
    // Insert account with macOS path
    conn.execute(
        "INSERT INTO accounts (id, alias, email, provider, snapshot_path, created_at, last_used, machine_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            "mac-id",
            "dev",
            "dev@example.com",
            "google",
            "/Users/user/.kiro-cli-auth/accounts/dev.sqlite3",  // macOS path
            chrono::Utc::now().timestamp(),
            None::<i64>,
            Some("mac-machine-789"),
        ],
    )?;
    drop(conn);

    // Create snapshot file
    let accounts_dir = export_dir.path().join("accounts");
    fs::create_dir_all(&accounts_dir)?;
    fs::write(accounts_dir.join("dev.sqlite3"), "macos data")?;

    // Import to target
    let target_fm = FileManager::new_with_base(target_dir.path().to_path_buf())?;
    let importer = Importer::new(target_fm.clone());
    importer.import(export_dir.path().to_str().unwrap(), false)?;

    // Verify path was converted to local style
    let target_conn = target_fm.get_db_connection()?;
    let imported = db::find_account(&target_conn, "dev")?.unwrap();
    
    let expected_path = target_fm.account_snapshot_path("dev");
    assert_eq!(imported.snapshot_path, expected_path.to_string_lossy().to_string());
    assert!(!imported.snapshot_path.starts_with("/Users/"));
    assert!(expected_path.exists());
    
    let content = fs::read_to_string(&imported.snapshot_path)?;
    assert_eq!(content, "macos data");

    Ok(())
}

#[test]
fn test_migration_cross_platform_paths() -> Result<()> {
    let dir = tempdir()?;
    
    // Create legacy JSON with Windows paths
    let json_content = r#"{
        "version": "1.0.0",
        "accounts": [
            {
                "id": "id1",
                "alias": "work",
                "email": "work@example.com",
                "provider": "google",
                "snapshot_path": "C:\\Users\\olduser\\.kiro-cli-auth\\accounts\\work.sqlite3",
                "created_at": "2024-01-01T00:00:00Z",
                "last_used": null
            }
        ]
    }"#;
    
    let json_path = dir.path().join("registry.json");
    fs::write(&json_path, json_content)?;
    
    // Create the snapshot file in the expected location
    let accounts_dir = dir.path().join("accounts");
    fs::create_dir_all(&accounts_dir)?;
    fs::write(accounts_dir.join("work.sqlite3"), "migrated data")?;
    
    // Execute migration
    use kiro_cli_auth::core::data::migration::migrate_from_json_if_needed;
    let migrated = migrate_from_json_if_needed(dir.path())?;
    assert!(migrated);
    
    // Verify path was updated to local style
    let db_path = dir.path().join("registry.db");
    let conn = db::init_db(&db_path)?;
    let accounts = db::list_accounts(&conn)?;
    
    assert_eq!(accounts.len(), 1);
    let account = &accounts[0];
    
    // Path should be updated to local path, not Windows path
    let expected_path = accounts_dir.join("work.sqlite3");
    assert_eq!(account.snapshot_path, expected_path.to_string_lossy().to_string());
    assert!(!account.snapshot_path.contains("C:\\Users\\olduser\\.kiro-cli-auth"));
    
    Ok(())
}

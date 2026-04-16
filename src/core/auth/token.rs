use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use std::path::Path;

// Extract access token from Kiro SQLite database
// Tries AWS Builder ID (ODIC) first, falls back to Google (Social)
pub fn extract_token(db_path: &Path) -> Result<String> {
    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("Failed to open SQLite database: {:?}", db_path))?;
    
    // Try AWS Builder ID (ODIC)
    let mut stmt = conn.prepare("SELECT value FROM auth_kv WHERE key = 'kirocli:odic:token'")
        .context("Failed to prepare SQL statement")?;
    let value: String = if let Ok(v) = stmt.query_row([], |row| row.get(0)) {
        v
    } else {
        // Fallback to Google login (Social)
        drop(stmt);
        let mut stmt = conn.prepare("SELECT value FROM auth_kv WHERE key = 'kirocli:social:token'")
            .context("Failed to prepare SQL statement")?;
        stmt.query_row([], |row| row.get(0))
            .context("Token not found in database")?
    };
    
    let json: Value = serde_json::from_str(&value)
        .context("Failed to parse token JSON")?;
    json["access_token"]
        .as_str()
        .map(|s| s.to_string())
        .context("access_token field not found in token data")
}

// Extract refresh token from Kiro SQLite database
pub fn extract_refresh_token(db_path: &Path) -> Result<String> {
    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("Failed to open SQLite database: {:?}", db_path))?;
    
    // Try AWS Builder ID (ODIC)
    let mut stmt = conn.prepare("SELECT value FROM auth_kv WHERE key = 'kirocli:odic:token'")
        .context("Failed to prepare SQL statement")?;
    let value: String = if let Ok(v) = stmt.query_row([], |row| row.get(0)) {
        v
    } else {
        // Fallback to Google login (Social)
        drop(stmt);
        let mut stmt = conn.prepare("SELECT value FROM auth_kv WHERE key = 'kirocli:social:token'")
            .context("Failed to prepare SQL statement")?;
        stmt.query_row([], |row| row.get(0))
            .context("Token not found in database")?
    };
    
    let json: Value = serde_json::from_str(&value)
        .context("Failed to parse token JSON")?;
    json["refresh_token"]
        .as_str()
        .map(|s| s.to_string())
        .context("refresh_token field not found in token data")
}

// Update tokens in SQLite database (preserves other fields in JSON)
pub fn update_token(db_path: &Path, access_token: &str, refresh_token: Option<&str>) -> Result<()> {
    let conn = Connection::open(db_path)
        .with_context(|| format!("Failed to open SQLite database: {:?}", db_path))?;
    
    // Detect which token key is in use
    let token_key = {
        let mut stmt = conn.prepare("SELECT key FROM auth_kv WHERE key IN ('kirocli:odic:token', 'kirocli:social:token')")
            .context("Failed to prepare SQL statement")?;
        let key: String = stmt.query_row([], |row| row.get(0))
            .context("Token not found in database")?;
        key
    };
    
    let value: String = conn.prepare("SELECT value FROM auth_kv WHERE key = ?1")?
        .query_row([&token_key], |row| row.get(0))
        .context("Token not found in database")?;
    
    let mut json: Value = serde_json::from_str(&value)
        .context("Failed to parse token JSON")?;
    
    json["access_token"] = Value::String(access_token.to_string());
    if let Some(rt) = refresh_token {
        json["refresh_token"] = Value::String(rt.to_string());
    }
    
    let updated_value = serde_json::to_string(&json)
        .context("Failed to serialize updated token")?;
    
    conn.execute(
        "UPDATE auth_kv SET value = ?1 WHERE key = ?2",
        [updated_value, token_key],
    ).context("Failed to update token in database")?;
    
    Ok(())
}

// Extract account email and provider from database
pub fn extract_account_info(db_path: &Path) -> Result<(String, String)> {
    let token = extract_token(db_path)?;
    let account_info = crate::core::auth::api::get_account_info(&token)?;
    
    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("Failed to open SQLite database: {:?}", db_path))?;
    
    // Detect login provider by checking which token key exists
    let mut stmt = conn.prepare("SELECT key FROM auth_kv WHERE key IN ('kirocli:odic:token', 'kirocli:social:token') LIMIT 1")
        .context("Failed to prepare SQL statement")?;
    
    let provider = match stmt.query_row([], |row| row.get::<_, String>(0)) {
        Ok(token_key) => {
            if token_key == "kirocli:odic:token" {
                "builder-id"
            } else {
                "google"
            }
        }
        Err(_) => "unknown"
    };
    
    Ok((account_info.email, provider.to_string()))
}

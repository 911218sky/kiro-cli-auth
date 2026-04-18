use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};
use std::path::Path;

fn open_ro(db_path: &Path) -> Result<Connection> {
    Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("Failed to open SQLite database: {:?}", db_path))
}

fn get_auth_value(conn: &Connection, key: &str) -> Result<String> {
    conn.prepare("SELECT value FROM auth WHERE key = ?1")
        .context("Failed to prepare SQL statement")?
        .query_row([key], |row| row.get(0))
        .with_context(|| format!("Key '{}' not found in auth table", key))
}

pub fn extract_token(db_path: &Path) -> Result<String> {
    let conn = open_ro(db_path)?;
    get_auth_value(&conn, "access_token")
}

pub fn extract_refresh_token(db_path: &Path) -> Result<String> {
    let conn = open_ro(db_path)?;
    get_auth_value(&conn, "refresh_token")
}

pub fn extract_account_info(db_path: &Path) -> Result<(String, String)> {
    let conn = open_ro(db_path)?;
    let email = get_auth_value(&conn, "email")?;
    let provider = get_auth_value(&conn, "provider").unwrap_or_else(|_| "google".to_string());
    // Normalize provider name
    let provider = match provider.as_str() {
        "aws_builder_id" | "BuilderId" | "IdC" => "builder-id",
        "google" | "Google" | "Github" | "social" => "google",
        other => other,
    }.to_string();
    Ok((email, provider))
}

pub fn update_token(db_path: &Path, access_token: &str, refresh_token: Option<&str>) -> Result<()> {
    let conn = Connection::open(db_path)
        .with_context(|| format!("Failed to open SQLite database: {:?}", db_path))?;

    conn.execute(
        "INSERT OR REPLACE INTO auth (key, value) VALUES ('access_token', ?1)",
        [access_token],
    ).context("Failed to update access_token")?;

    if let Some(rt) = refresh_token {
        conn.execute(
            "INSERT OR REPLACE INTO auth (key, value) VALUES ('refresh_token', ?1)",
            [rt],
        ).context("Failed to update refresh_token")?;
    }

    Ok(())
}

/// Read AWS SSO clientId/clientSecret from ~/.aws/sso/cache/
/// Returns (clientId, clientSecret, region) for Builder ID accounts
pub fn read_aws_sso_credentials() -> Option<(String, String, String)> {
    let home = dirs::home_dir()?;
    let sso_cache = home.join(".aws").join("sso").join("cache");

    // Read kiro-auth-token.json to get clientIdHash and region
    let token_path = sso_cache.join("kiro-auth-token.json");
    let token_content = std::fs::read_to_string(&token_path).ok()?;
    let token_data: serde_json::Value = serde_json::from_str(&token_content).ok()?;

    let region = token_data["region"].as_str().unwrap_or("us-east-1").to_string();
    let client_id_hash = token_data["clientIdHash"].as_str()?;

    // Read {clientIdHash}.json to get clientId/clientSecret
    let client_path = sso_cache.join(format!("{}.json", client_id_hash));
    let client_content = std::fs::read_to_string(&client_path).ok().or_else(|| {
        // Fallback: scan all .json files for one with clientId/clientSecret
        std::fs::read_dir(&sso_cache).ok()?.filter_map(|e| e.ok()).find_map(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.ends_with(".json") && name != "kiro-auth-token.json" {
                let content = std::fs::read_to_string(entry.path()).ok()?;
                let data: serde_json::Value = serde_json::from_str(&content).ok()?;
                if data["clientId"].is_string() && data["clientSecret"].is_string() {
                    Some(content)
                } else {
                    None
                }
            } else {
                None
            }
        })
    })?;

    let client_data: serde_json::Value = serde_json::from_str(&client_content).ok()?;
    let client_id = client_data["clientId"].as_str()?.to_string();
    let client_secret = client_data["clientSecret"].as_str()?.to_string();

    Some((client_id, client_secret, region))
}

// Clear tokens from database (used on Windows when file cannot be deleted)
pub fn clear_token(db_path: &Path) -> Result<()> {
    let conn = Connection::open(db_path)
        .with_context(|| format!("Failed to open SQLite database: {:?}", db_path))?;
    conn.execute("DELETE FROM auth WHERE key IN ('access_token', 'refresh_token')", [])
        .context("Failed to clear tokens from database")?;
    Ok(())
}

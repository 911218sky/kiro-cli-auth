use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct TokenBlob {
    access_token: Option<String>,
    refresh_token: Option<String>,
    provider: Option<String>,
}

/// Known auth_kv key prefixes kiro-cli uses, in priority order
const AUTH_KEY_PREFIXES: &[&str] = &[
    "kirocli:social:token",
    "kirocli:builderid:token",
    "kirocli:idc:token",
];

fn open_ro(db_path: &Path) -> Result<Connection> {
    Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("Failed to open SQLite database: {:?}", db_path))
}

/// Read the first matching token blob from auth_kv
fn read_token_blob(conn: &Connection) -> Result<(String, TokenBlob)> {
    // Try known keys first
    for key in AUTH_KEY_PREFIXES {
        if let Ok(value) = conn
            .prepare("SELECT value FROM auth_kv WHERE key = ?1")?
            .query_row([key], |row| row.get::<_, String>(0))
        {
            let blob: TokenBlob = serde_json::from_str(&value)
                .with_context(|| format!("Failed to parse token blob for key '{}'", key))?;
            return Ok((key.to_string(), blob));
        }
    }

    // Fallback: scan all auth_kv rows for any JSON with access_token
    let mut stmt = conn.prepare("SELECT key, value FROM auth_kv")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let key: String = row.get(0)?;
        let value: String = row.get(1)?;
        if let Ok(blob) = serde_json::from_str::<TokenBlob>(&value) {
            if blob.access_token.is_some() {
                return Ok((key, blob));
            }
        }
    }

    Err(anyhow::anyhow!(
        "No auth token found in auth_kv. Please run 'kiro-cli login' first."
    ))
}

pub fn extract_token(db_path: &Path) -> Result<String> {
    let conn = open_ro(db_path)?;
    let (_, blob) = read_token_blob(&conn)?;
    blob.access_token
        .ok_or_else(|| anyhow::anyhow!("access_token missing from token blob"))
}

pub fn extract_refresh_token(db_path: &Path) -> Result<String> {
    let conn = open_ro(db_path)?;
    let (_, blob) = read_token_blob(&conn)?;
    blob.refresh_token
        .ok_or_else(|| anyhow::anyhow!("refresh_token missing from token blob"))
}

/// Returns (email, provider). Email is fetched from the API since it's not stored locally.
pub fn extract_account_info(db_path: &Path) -> Result<(String, String)> {
    let conn = open_ro(db_path)?;
    let (key, blob) = read_token_blob(&conn)?;

    let raw_provider = blob.provider.unwrap_or_else(|| {
        // Infer provider from key name
        if key.contains("builderid") || key.contains("idc") {
            "aws_builder_id".to_string()
        } else {
            "google".to_string()
        }
    });

    let provider = normalize_provider(&raw_provider);

    let access_token = blob.access_token
        .ok_or_else(|| anyhow::anyhow!("access_token missing from token blob"))?;

    // Fetch email from API
    let email = crate::core::auth::api::get_account_info(&access_token)
        .map(|info| info.email)
        .unwrap_or_else(|_| "unknown@unknown".to_string());

    Ok((email, provider))
}

fn normalize_provider(provider: &str) -> String {
    match provider {
        "aws_builder_id" | "BuilderId" | "IdC" | "builderid" | "idc" => "builder-id",
        "google" | "Google" | "Github" | "social" => "google",
        other => other,
    }
    .to_string()
}

pub fn update_token(db_path: &Path, access_token: &str, refresh_token: Option<&str>) -> Result<()> {
    let conn = Connection::open(db_path)
        .with_context(|| format!("Failed to open SQLite database: {:?}", db_path))?;

    // Find the existing key to update in-place
    let key = find_auth_key(&conn).unwrap_or_else(|| "kirocli:social:token".to_string());

    // Read existing blob, patch tokens, write back
    let existing_json: String = conn
        .prepare("SELECT value FROM auth_kv WHERE key = ?1")?
        .query_row([&key], |row| row.get(0))
        .unwrap_or_else(|_| "{}".to_string());

    let mut blob: serde_json::Value = serde_json::from_str(&existing_json).unwrap_or_default();
    blob["access_token"] = serde_json::Value::String(access_token.to_string());
    if let Some(rt) = refresh_token {
        blob["refresh_token"] = serde_json::Value::String(rt.to_string());
    }

    conn.execute(
        "INSERT OR REPLACE INTO auth_kv (key, value) VALUES (?1, ?2)",
        rusqlite::params![key, blob.to_string()],
    )
    .context("Failed to update token in auth_kv")?;

    Ok(())
}

#[allow(dead_code)]
pub fn clear_token(db_path: &Path) -> Result<()> {
    let conn = Connection::open(db_path)
        .with_context(|| format!("Failed to open SQLite database: {:?}", db_path))?;

    // Null out tokens inside the JSON blob rather than deleting the row
    if let Some(key) = find_auth_key(&conn) {
        if let Ok(existing_json) = conn
            .prepare("SELECT value FROM auth_kv WHERE key = ?1")?
            .query_row([&key], |row| row.get::<_, String>(0))
        {
            if let Ok(mut blob) = serde_json::from_str::<serde_json::Value>(&existing_json) {
                blob["access_token"] = serde_json::Value::Null;
                blob["refresh_token"] = serde_json::Value::Null;
                conn.execute(
                    "UPDATE auth_kv SET value = ?1 WHERE key = ?2",
                    rusqlite::params![blob.to_string(), key],
                )
                .context("Failed to clear tokens in auth_kv")?;
                return Ok(());
            }
        }
    }

    Ok(())
}

fn find_auth_key(conn: &Connection) -> Option<String> {
    for key in AUTH_KEY_PREFIXES {
        if conn
            .prepare("SELECT 1 FROM auth_kv WHERE key = ?1")
            .ok()?
            .exists([key])
            .unwrap_or(false)
        {
            return Some(key.to_string());
        }
    }
    // Fallback: first row with a JSON access_token
    let mut stmt = conn.prepare("SELECT key, value FROM auth_kv").ok()?;
    let mut rows = stmt.query([]).ok()?;
    while let Some(row) = rows.next().ok()? {
        let key: String = row.get(0).ok()?;
        let value: String = row.get(1).ok()?;
        if let Ok(blob) = serde_json::from_str::<TokenBlob>(&value) {
            if blob.access_token.is_some() {
                return Some(key);
            }
        }
    }
    None
}

/// Read AWS SSO clientId/clientSecret from ~/.aws/sso/cache/
pub fn read_aws_sso_credentials() -> Option<(String, String, String)> {
    let home = dirs::home_dir()?;
    let sso_cache = home.join(".aws").join("sso").join("cache");

    let token_path = sso_cache.join("kiro-auth-token.json");
    let token_content = std::fs::read_to_string(&token_path).ok()?;
    let token_data: serde_json::Value = serde_json::from_str(&token_content).ok()?;

    let region = token_data["region"].as_str().unwrap_or("us-east-1").to_string();
    let client_id_hash = token_data["clientIdHash"].as_str()?;

    let client_path = sso_cache.join(format!("{}.json", client_id_hash));
    let client_content = std::fs::read_to_string(&client_path).ok().or_else(|| {
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

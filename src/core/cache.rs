use rusqlite::{Connection, params};
use anyhow::Result;
use crate::core::auth::api::AccountInfo;

pub struct AccountCache {
    conn: Connection,
}

impl AccountCache {
    pub fn new(db_path: &std::path::Path) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS account_cache (
                email TEXT PRIMARY KEY,
                token TEXT NOT NULL,
                subscription_type TEXT NOT NULL,
                status TEXT NOT NULL,
                current_usage REAL NOT NULL,
                usage_limit REAL NOT NULL,
                is_banned INTEGER NOT NULL,
                trial_expiry TEXT,
                next_reset TEXT,
                cached_at INTEGER NOT NULL
            )",
            [],
        )?;
        Ok(Self { conn })
    }

    #[allow(dead_code)]
    pub fn get(&self, email: &str) -> Option<(AccountInfo, String)> {
        let mut stmt = self.conn.prepare(
            "SELECT token, subscription_type, status, current_usage, usage_limit, is_banned, trial_expiry, next_reset 
             FROM account_cache WHERE email = ?1"
        ).ok()?;
        
        stmt.query_row(params![email], |row| {
            Ok((
                AccountInfo {
                    email: email.to_string(),
                    subscription_type: row.get(1)?,
                    status: row.get(2)?,
                    current_usage: row.get(3)?,
                    usage_limit: row.get(4)?,
                    is_banned: row.get::<_, i64>(5)? != 0,
                    trial_expiry: row.get(6).ok(),
                    next_reset: row.get(7).ok(),
                },
                row.get::<_, String>(0)?,
            ))
        }).ok()
    }

    pub fn get_with_time(&self, email: &str) -> Option<(AccountInfo, i64)> {
        let mut stmt = self.conn.prepare(
            "SELECT token, subscription_type, status, current_usage, usage_limit, is_banned, trial_expiry, next_reset, cached_at
             FROM account_cache WHERE email = ?1"
        ).ok()?;
        
        stmt.query_row(params![email], |row| {
            Ok((
                AccountInfo {
                    email: email.to_string(),
                    subscription_type: row.get(1)?,
                    status: row.get(2)?,
                    current_usage: row.get(3)?,
                    usage_limit: row.get(4)?,
                    is_banned: row.get::<_, i64>(5)? != 0,
                    trial_expiry: row.get(6).ok(),
                    next_reset: row.get(7).ok(),
                },
                row.get::<_, i64>(8)?,
            ))
        }).ok()
    }

    pub fn set(&self, email: String, info: AccountInfo, token: String) -> Result<()> {
        let cached_at = chrono::Utc::now().timestamp();
        self.conn.execute(
            "INSERT OR REPLACE INTO account_cache 
             (email, token, subscription_type, status, current_usage, usage_limit, is_banned, trial_expiry, next_reset, cached_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                email,
                token,
                info.subscription_type,
                info.status,
                info.current_usage,
                info.usage_limit,
                if info.is_banned { 1 } else { 0 },
                info.trial_expiry,
                info.next_reset,
                cached_at,
            ],
        )?;
        Ok(())
    }

    pub fn remove(&self, email: &str) -> Result<()> {
        self.conn.execute("DELETE FROM account_cache WHERE email = ?1", params![email])?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn clear(&self) -> Result<()> {
        self.conn.execute("DELETE FROM account_cache", [])?;
        Ok(())
    }
}

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

// Represents a Kiro account with authentication and usage tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub alias: String,
    pub email: String,
    pub provider: String,           // "builder-id" or "google"
    pub snapshot_path: String,      // Path to SQLite database snapshot
    pub created_at: DateTime<Utc>,
    pub last_used: Option<DateTime<Utc>>,
    pub machine_id: Option<String>, // Device identifier for multi-device tracking
}


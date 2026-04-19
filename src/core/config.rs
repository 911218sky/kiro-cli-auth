use std::path::PathBuf;
use std::time::Duration;

// ============================================================================
// Cache Configuration
// ============================================================================

/// Cache TTL in seconds (5 minutes)
/// Can be overridden by KIRO_CACHE_TTL environment variable
pub fn cache_ttl_seconds() -> i64 {
    std::env::var("KIRO_CACHE_TTL")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300)
}

/// Get cache database path
/// Can be overridden by KIRO_CACHE_PATH environment variable
pub fn cache_db_path() -> PathBuf {
    std::env::var("KIRO_CACHE_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("kiro-cli-auth-cache.db"))
}

// ============================================================================
// API Configuration
// ============================================================================

/// Kiro API base URL for usage limits
pub const API_USAGE_LIMITS_URL: &str = 
    "https://q.us-east-1.amazonaws.com/getUsageLimits?origin=AI_EDITOR&resourceType=AGENTIC_REQUEST&isEmailRequired=true";

/// Kiro API base URL for token refresh
pub const API_REFRESH_TOKEN_URL: &str = 
    "https://prod.us-east-1.auth.desktop.kiro.dev/refreshToken";

/// AWS OIDC token URL format (requires region)
pub fn aws_oidc_token_url(region: &str) -> String {
    format!("https://oidc.{}.amazonaws.com/token", region)
}

/// API request timeout in seconds
/// Can be overridden by KIRO_API_TIMEOUT environment variable
pub fn api_timeout_seconds() -> u64 {
    std::env::var("KIRO_API_TIMEOUT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(15)
}

/// API request timeout as Duration
pub fn api_timeout() -> Duration {
    Duration::from_secs(api_timeout_seconds())
}

/// Short API timeout for quick operations (10 seconds)
pub fn api_short_timeout() -> Duration {
    Duration::from_secs(10)
}

/// API retry delay in milliseconds
pub const API_RETRY_DELAY_MS: u64 = 500;

/// API retry delay as Duration
pub fn api_retry_delay() -> Duration {
    Duration::from_millis(API_RETRY_DELAY_MS)
}

// ============================================================================
// UI Configuration
// ============================================================================

/// Spinner tick rate in milliseconds
pub const SPINNER_TICK_MS: u64 = 80;

/// Spinner tick rate as Duration
pub fn spinner_tick_rate() -> Duration {
    Duration::from_millis(SPINNER_TICK_MS)
}

/// Login operation delay in milliseconds (wait for kiro-cli to write data)
pub const LOGIN_DELAY_MS: u64 = 500;

/// Login operation delay as Duration
pub fn login_delay() -> Duration {
    Duration::from_millis(LOGIN_DELAY_MS)
}

// ============================================================================
// Directory Configuration
// ============================================================================

/// Base directory name for Unix systems
pub const UNIX_DIR_NAME: &str = ".kiro-cli-auth";

/// Base directory name for Windows systems
pub const WINDOWS_DIR_NAME: &str = "kiro-cli-auth";

/// Accounts subdirectory name
pub const ACCOUNTS_DIR_NAME: &str = "accounts";

/// Registry database filename
pub const REGISTRY_DB_NAME: &str = "registry.db";

/// Legacy registry JSON filename (for migration)
pub const LEGACY_REGISTRY_NAME: &str = "registry.json";

/// Backup file extension
pub const BACKUP_EXTENSION: &str = "backup";

// ============================================================================
// Environment Variables
// ============================================================================

/// Environment variable for custom base directory
pub const ENV_BASE_DIR: &str = "KIRO_CLI_AUTH_DIR";

/// Environment variable for cache TTL (seconds)
pub const ENV_CACHE_TTL: &str = "KIRO_CACHE_TTL";

/// Environment variable for cache path
pub const ENV_CACHE_PATH: &str = "KIRO_CACHE_PATH";

/// Environment variable for API timeout (seconds)
pub const ENV_API_TIMEOUT: &str = "KIRO_API_TIMEOUT";

/// Environment variable for XDG data home (Unix)
pub const ENV_XDG_DATA_HOME: &str = "XDG_DATA_HOME";

/// Environment variable for APPDATA (Windows)
pub const ENV_APPDATA: &str = "APPDATA";

/// Environment variable for LOCALAPPDATA (Windows)
pub const ENV_LOCALAPPDATA: &str = "LOCALAPPDATA";

/// Environment variable to disable cache
pub const ENV_NO_CACHE: &str = "KIRO_NO_CACHE";

// ============================================================================
// File Permissions (Unix)
// ============================================================================

/// Directory permissions (owner only: rwx------)
#[cfg(unix)]
pub const DIR_PERMISSIONS: u32 = 0o700;

/// File permissions (owner only: rw-------)
#[cfg(unix)]
pub const FILE_PERMISSIONS: u32 = 0o600;

// ============================================================================
// GitHub Release Configuration
// ============================================================================

/// GitHub repository for releases
pub const GITHUB_REPO: &str = "911218sky/kiro-cli-auth";

/// GitHub API URL for latest release
pub fn github_latest_release_url() -> String {
    format!("https://api.github.com/repos/{}/releases/latest", GITHUB_REPO)
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Get all environment variable names used by the application
pub fn env_vars() -> Vec<&'static str> {
    vec![
        ENV_BASE_DIR,
        ENV_CACHE_TTL,
        ENV_CACHE_PATH,
        ENV_API_TIMEOUT,
        ENV_XDG_DATA_HOME,
        ENV_APPDATA,
        ENV_LOCALAPPDATA,
    ]
}

/// Check if running in debug mode
pub fn is_debug() -> bool {
    cfg!(debug_assertions)
}

/// Check if cache should be used
/// Returns false if KIRO_NO_CACHE is set to "1", "true", or "yes"
pub fn should_use_cache() -> bool {
    match std::env::var(ENV_NO_CACHE) {
        Ok(val) => {
            let val_lower = val.to_lowercase();
            val_lower != "1" && val_lower != "true" && val_lower != "yes"
        }
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_cache_ttl() {
        let ttl = cache_ttl_seconds();
        assert!(ttl > 0);
        assert!(ttl <= 3600); // Should be reasonable (≤ 1 hour)
    }

    #[test]
    fn test_api_timeout() {
        let timeout = api_timeout_seconds();
        assert!(timeout >= 5);
        assert!(timeout <= 60);
    }

    #[test]
    fn test_github_url() {
        let url = github_latest_release_url();
        assert!(url.contains("github.com"));
        assert!(url.contains(GITHUB_REPO));
    }
}

use std::thread;
use indicatif::{ProgressBar, ProgressStyle};
use crate::core::auth::api::{get_account_info, refresh_token, refresh_token_oidc, AccountInfo};
use crate::core::auth::token::{extract_account_info, extract_token, extract_refresh_token, update_token, read_aws_sso_credentials};
use crate::core::cache::AccountCache;
use crate::core::config;
use crate::core::models::Account;

/// Create a spinner with consistent style
pub fn create_spinner(message: &str) -> ProgressBar {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_spinner())
    );
    spinner.set_message(message.to_string());
    spinner.enable_steady_tick(config::spinner_tick_rate());
    spinner
}

/// Sort accounts by trial expiry days (ascending: fewer days first)
/// Accounts with trial info come before accounts without trial info
pub fn sort_by_trial_days(results: &mut Vec<(Account, bool, Option<AccountInfo>)>) {
    results.sort_by(|a, b| {
        let days_a = a.2.as_ref().and_then(|info| {
            info.trial_expiry.as_ref().and_then(|expiry| {
                chrono::DateTime::parse_from_rfc3339(expiry).ok().map(|expiry_time| {
                    let now = chrono::Utc::now();
                    expiry_time.signed_duration_since(now).num_days()
                })
            })
        });
        let days_b = b.2.as_ref().and_then(|info| {
            info.trial_expiry.as_ref().and_then(|expiry| {
                chrono::DateTime::parse_from_rfc3339(expiry).ok().map(|expiry_time| {
                    let now = chrono::Utc::now();
                    expiry_time.signed_duration_since(now).num_days()
                })
            })
        });
        
        match (days_a, days_b) {
            (Some(da), Some(db)) => da.cmp(&db),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
    });
}

/// Fetch account usage info concurrently for multiple accounts
/// Returns (Account, is_current, usage_info) tuples
pub fn fetch_accounts_with_usage(
    accounts: &[Account],
    kiro_data: &std::path::PathBuf,
    current_email: Option<&String>,
    _cache: &AccountCache,
    no_cache: bool,
) -> Vec<(Account, bool, Option<AccountInfo>)> {
    let cache_path = config::cache_db_path();
    
    // Check if cache should be used (CLI flag or env var)
    let use_cache = !no_cache && config::should_use_cache();
    
    // Spawn concurrent threads for each account to fetch info in parallel
    let handles: Vec<_> = accounts.iter().map(|account| {
        let account = account.clone();
        let is_current = current_email == Some(&account.email);
        let kiro_data = kiro_data.clone();
        let cache_path = cache_path.clone();
        
        thread::spawn(move || {
            // Step 1: Check cache first (5-minute TTL) if enabled
            let cached_info = if use_cache {
                if let Ok(cache) = AccountCache::new(&cache_path) {
                    if let Some((info, cached_at)) = cache.get_with_time(&account.email) {
                        let now = chrono::Utc::now().timestamp();
                        let cache_age = now - cached_at;
                        // Return cached data if still valid (within TTL)
                        if cache_age < config::cache_ttl_seconds() {
                            return (account, is_current, Some(info));
                        }
                        // Keep expired cache as fallback
                        Some(info)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            // Step 2: Determine which database to use (live or snapshot)
            let snapshot_path = std::path::Path::new(&account.snapshot_path);
            let db_path: Option<std::path::PathBuf> = if is_current && kiro_data.exists() {
                Some(kiro_data)  // Use live data for current account
            } else if snapshot_path.exists() {
                Some(snapshot_path.to_path_buf())  // Use snapshot for other accounts
            } else {
                None
            };

            // Step 3: Try to fetch fresh data from API
            let fresh_info = if let Some(path) = db_path {
                let token_result = extract_token(&path);
                let mut needs_refresh = false;
                
                // Try using existing token
                let info = if let Ok(token) = &token_result {
                    match get_account_info(token) {
                        Ok(info) => Some(info),
                        Err(_) => {
                            needs_refresh = true;  // Token expired or invalid
                            None
                        }
                    }
                } else {
                    needs_refresh = true;
                    None
                };

                // Step 4: Auto-refresh token if needed
                if needs_refresh {
                    if let Ok(refresh_tok) = extract_refresh_token(&path) {
                        let provider = extract_account_info(&path).ok().map(|(_, p)| p).unwrap_or_default();
                        
                        // Use OIDC endpoint for AWS Builder ID, social endpoint for Google
                        let refresh_result = if provider == "builder-id" {
                            if let Some((client_id, client_secret, region)) = read_aws_sso_credentials() {
                                refresh_token_oidc(&refresh_tok, &client_id, &client_secret, &region)
                            } else {
                                refresh_token(&refresh_tok)
                            }
                        } else {
                            refresh_token(&refresh_tok)
                        };

                        if let Ok(refresh_resp) = refresh_result {
                            let new_refresh = refresh_resp.refresh_token.as_deref();
                            // Update snapshot with new token
                            if let Err(e) = update_token(&path, &refresh_resp.access_token, new_refresh) {
                                eprintln!("warn: failed to update snapshot for {}: {}", account.email, e);
                            }
                            
                            // Fetch account info with new token
                            if let Ok(info) = get_account_info(&refresh_resp.access_token) {
                                // Update cache with fresh data
                                if let Ok(cache) = AccountCache::new(&cache_path) {
                                    let _ = cache.set(account.email.clone(), info.clone(), refresh_resp.access_token);
                                }
                                Some(info)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else if let Some(info) = info {
                    // Token is valid, update cache
                    if let Ok(token) = token_result {
                        if let Ok(cache) = AccountCache::new(&cache_path) {
                            let _ = cache.set(account.email.clone(), info.clone(), token);
                        }
                    }
                    Some(info)
                } else {
                    None
                }
            } else {
                None
            };
            
            // Return fresh data if available, otherwise use cached data as fallback
            (account, is_current, fresh_info.or(cached_info))
        })
    }).collect();

    // Collect results from all threads, filtering out panicked threads
    handles.into_iter().filter_map(|h| match h.join() {
        Ok(result) => Some(result),
        Err(payload) => {
            let msg = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("<unknown panic>");
            eprintln!("warn: a background account-info thread panicked and was skipped: {}", msg);
            None
        }
    }).collect()
}

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::core::config;

// Account information returned from Kiro API
#[derive(Debug, Clone, Deserialize)]
pub struct AccountInfo {
    pub email: String,
    pub subscription_type: String,
    pub status: String,
    pub current_usage: f64,
    pub usage_limit: f64,
    pub is_banned: bool,
    pub trial_expiry: Option<String>,
    pub next_reset: Option<String>,
}

// API response structure for usage limits endpoint
#[derive(Debug, Deserialize)]
struct UsageData {
    #[serde(rename = "usageBreakdownList")]
    usage_breakdown_list: Option<Vec<UsageBreakdown>>,
    #[serde(rename = "subscriptionInfo")]
    subscription_info: Option<SubscriptionInfo>,
    #[serde(rename = "userInfo")]
    user_info: Option<UserInfo>,
    #[serde(rename = "nextDateReset")]
    next_date_reset: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct UsageBreakdown {
    #[serde(rename = "currentUsage")]
    current_usage: Option<f64>,
    #[serde(rename = "usageLimit")]
    usage_limit: Option<f64>,
    #[serde(rename = "freeTrialInfo")]
    free_trial_info: Option<FreeTrialInfo>,
}

#[derive(Debug, Deserialize)]
struct FreeTrialInfo {
    #[serde(rename = "freeTrialStatus")]
    status: Option<String>,
    #[serde(rename = "usageLimit")]
    usage_limit: Option<f64>,
    #[serde(rename = "currentUsage")]
    current_usage: Option<f64>,
    #[serde(rename = "freeTrialExpiry")]
    expiry: Option<f64>,  // Unix timestamp
}

#[derive(Debug, Deserialize)]
struct SubscriptionInfo {
    #[serde(rename = "type")]
    subscription_type: Option<String>,
    #[serde(rename = "subscriptionTitle")]
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UserInfo {
    email: Option<String>,
}

pub fn get_account_info(token: &str) -> Result<AccountInfo> {
    let url = config::API_USAGE_LIMITS_URL;
    
    // Retry up to 2 times with delay
    let mut last_error = None;
    for attempt in 0..2 {
        if attempt > 0 {
            std::thread::sleep(config::api_retry_delay());
        }
        
        match ureq::get(url)
            .timeout(config::api_timeout())
            .set("Accept", "application/json")
            .set("Authorization", &format!("Bearer {}", token))
            .call()
        {
            Ok(response) => {
                let response_text = response.into_string()
                    .context("Failed to read API response")?;
                
                let usage_data: UsageData = serde_json::from_str(&response_text)
                    .context("Failed to parse API response as JSON")?;

                let email = usage_data.user_info
                    .and_then(|u| u.email)
                    .ok_or_else(|| anyhow::anyhow!("API response missing userInfo.email"))?;

                let subscription_type = usage_data.subscription_info
                    .as_ref()
                    .and_then(|s| s.title.clone().or_else(|| s.subscription_type.clone()))
                    .filter(|s| !s.is_empty() && s != "Unknown")
                    .unwrap_or_else(|| "Free".to_string());

                let status = "Active".to_string();
                let is_banned = false;

                let mut current_usage = 0.0;
                let mut usage_limit = 0.0;
                let mut trial_expiry = None;
    
                // Combine base subscription usage with active free trial usage
                if let Some(breakdown_list) = usage_data.usage_breakdown_list {
                    if let Some(breakdown) = breakdown_list.into_iter().next() {
                        let base_limit = breakdown.usage_limit.unwrap_or(0.0);
                        let base_current = breakdown.current_usage.unwrap_or(0.0);
                        
                        let (trial_limit, trial_current) = if let Some(trial_info) = &breakdown.free_trial_info {
                            if let Some(expiry_ts) = trial_info.expiry {
                                trial_expiry = chrono::DateTime::from_timestamp(expiry_ts as i64, 0)
                                    .map(|dt| dt.to_rfc3339());
                            }
                            
                            if let Some(trial_status) = &trial_info.status {
                                if trial_status.to_uppercase() == "ACTIVE" {
                                    (
                                        trial_info.usage_limit.unwrap_or(0.0),
                                        trial_info.current_usage.unwrap_or(0.0)
                                    )
                                } else {
                                    (0.0, 0.0)
                                }
                            } else {
                                (0.0, 0.0)
                            }
                        } else {
                            (0.0, 0.0)
                        };
                        
                        current_usage = base_current + trial_current;
                        usage_limit = base_limit + trial_limit;
                    }
                }

                let next_reset = usage_data.next_date_reset.map(|ts| {
                    chrono::DateTime::from_timestamp(ts as i64, 0)
                        .map(|dt| dt.format("%Y-%m-%d").to_string())
                        .unwrap_or_default()
                });

                return Ok(AccountInfo {
                    email,
                    subscription_type,
                    status,
                    current_usage,
                    usage_limit,
                    is_banned,
                    trial_expiry,
                    next_reset,
                });
            }
            Err(e) => {
                last_error = Some(anyhow::anyhow!("API call failed: {}", e));
            }
        }
    }
    
    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("API call failed after retries")))
}

#[derive(Debug, Serialize)]
struct RefreshTokenRequest {
    #[serde(rename = "refreshToken")]
    refresh_token: String,
}

#[derive(Debug, Deserialize)]
pub struct RefreshTokenResponse {
    #[serde(rename = "accessToken")]
    pub access_token: String,
    #[serde(rename = "refreshToken")]
    pub refresh_token: Option<String>,
    #[serde(rename = "expiresIn")]
    #[allow(dead_code)]
    pub expires_in: Option<u64>,
}

// Exchange refresh token for new access token
pub fn refresh_token(refresh_token: &str) -> Result<RefreshTokenResponse> {
    let url = "https://prod.us-east-1.auth.desktop.kiro.dev/refreshToken";
    
    let request_body = RefreshTokenRequest {
        refresh_token: refresh_token.to_string(),
    };
    
    let response = ureq::post(url)
        .timeout(std::time::Duration::from_secs(10))
        .set("Content-Type", "application/json")
        .set("User-Agent", "aws-sdk-js/1.0.18 ua/2.1 os/linux lang/js md/nodejs#20.16.0 api/codewhispererstreaming#1.0.18 m/E KiroIDE-0.6.18")
        .send_json(&request_body)
        .map_err(|e| anyhow::anyhow!("Token refresh failed: {}", e))?;

    let refresh_response: RefreshTokenResponse = response.into_json()
        .context("Failed to parse refresh token response")?;

    Ok(refresh_response)
}

#[derive(Debug, serde::Serialize)]
struct OidcRefreshRequest {
    #[serde(rename = "clientId")]
    client_id: String,
    #[serde(rename = "clientSecret")]
    client_secret: String,
    #[serde(rename = "refreshToken")]
    refresh_token: String,
    #[serde(rename = "grantType")]
    grant_type: String,
}

// Exchange refresh token for new access token (AWS Builder ID / OIDC)
pub fn refresh_token_oidc(refresh_token: &str, client_id: &str, client_secret: &str, region: &str) -> Result<RefreshTokenResponse> {
    let url = format!("https://oidc.{}.amazonaws.com/token", region);

    let request_body = OidcRefreshRequest {
        client_id: client_id.to_string(),
        client_secret: client_secret.to_string(),
        refresh_token: refresh_token.to_string(),
        grant_type: "refresh_token".to_string(),
    };

    let response = ureq::post(&url)
        .timeout(std::time::Duration::from_secs(10))
        .set("Content-Type", "application/json")
        .send_json(&request_body)
        .map_err(|e| anyhow::anyhow!("Token refresh failed: {}", e))?;

    let data: serde_json::Value = response.into_json()
        .context("Failed to parse OIDC refresh response")?;

    Ok(RefreshTokenResponse {
        access_token: data["accessToken"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing accessToken in OIDC response"))?.to_string(),
        refresh_token: data["refreshToken"].as_str().map(|s| s.to_string()),
        expires_in: data["expiresIn"].as_u64(),
    })
}

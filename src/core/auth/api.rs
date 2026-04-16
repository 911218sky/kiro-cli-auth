use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

// Account information returned from Kiro API
#[derive(Debug, Deserialize)]
pub struct AccountInfo {
    pub email: String,
    pub subscription_type: String,
    pub status: String,
    pub current_usage: f64,
    pub usage_limit: f64,
    pub is_banned: bool,
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
    #[allow(dead_code)]
    #[serde(rename = "currentUsage")]
    current_usage: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct SubscriptionInfo {
    #[serde(rename = "subscriptionType")]
    subscription_type: Option<String>,
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UserInfo {
    email: Option<String>,
}

pub fn get_account_info(token: &str) -> Result<AccountInfo> {
    let url = "https://q.us-east-1.amazonaws.com/getUsageLimits?origin=AI_EDITOR&resourceType=AGENTIC_REQUEST&isEmailRequired=true";
    
    // Retry up to 2 times with 500ms delay
    let mut last_error = None;
    for attempt in 0..2 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        
        match ureq::get(url)
            .timeout(std::time::Duration::from_secs(15))
            .set("Accept", "application/json")
            .set("Authorization", &format!("Bearer {}", token))
            .call()
        {
            Ok(response) => {
                let usage_data: UsageData = response.into_json()
                    .context("Failed to parse API response as JSON")?;

                let email = usage_data.user_info
                    .and_then(|u| u.email)
                    .ok_or_else(|| anyhow::anyhow!("API response missing userInfo.email"))?;

                let subscription_type = usage_data.subscription_info
                    .as_ref()
                    .and_then(|s| s.subscription_type.clone())
                    .filter(|s| !s.is_empty() && s != "Unknown")
                    .unwrap_or_else(|| "Free".to_string());

                let status = usage_data.subscription_info
                    .as_ref()
                    .and_then(|s| s.status.clone())
                    .unwrap_or_else(|| "Unknown".to_string());

                let is_banned = status.to_uppercase() == "BANNED";

                let mut current_usage = 0.0;
                let mut usage_limit = 0.0;
    
                // Combine base subscription usage with active free trial usage
                if let Some(breakdown_list) = usage_data.usage_breakdown_list
                    && let Some(breakdown) = breakdown_list.into_iter().next() {
                        let base_limit = breakdown.usage_limit.unwrap_or(0.0);
                        let base_current = breakdown.current_usage.unwrap_or(0.0);
                        
                        let (trial_limit, trial_current) = if let Some(trial_info) = &breakdown.free_trial_info {
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

                return Ok(AccountInfo {
                    email,
                    subscription_type,
                    status,
                    current_usage,
                    usage_limit,
                    is_banned,
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

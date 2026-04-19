use crate::ui;
use crate::core::auth::api::AccountInfo;
use crate::core::models::Account;

/// Format account display string consistently across all commands
/// Returns a colored, formatted string showing account info
pub fn format_account_display(account: &Account, is_current: bool, info_opt: Option<&AccountInfo>) -> String {
    // Show filled circle for current account, hollow for others
    let prefix = if is_current { 
        ui::cyan("● ") 
    } else { 
        ui::cyan("○ ") 
    };
    
    // Format provider badge with color
    let provider = match account.provider.as_str() {
        "google" => ui::magenta("[Google]"),
        "builder-id" => ui::cyan("[AWS]"),
        _ => return format!("{}{} {} ({})", prefix, account.provider, account.alias, account.email),
    };
    
    if let Some(info) = info_opt {
        // Calculate usage percentage
        let percentage = if info.usage_limit > 0.0 {
            (info.current_usage / info.usage_limit * 100.0) as u32
        } else {
            0
        };
        
        // Status icon: ✓ for active, 🚫 for banned
        let status = if info.is_banned { "🚫" } else { "✓" };
        
        // Calculate trial expiry days remaining
        let expiry_str = if let Some(trial_expiry) = &info.trial_expiry {
            if let Ok(expiry_time) = chrono::DateTime::parse_from_rfc3339(trial_expiry) {
                let now = chrono::Utc::now();
                let diff = expiry_time.signed_duration_since(now);
                let days = diff.num_days();
                if days > 0 {
                    format!(" | 試用{}天", days)
                } else if days == 0 {
                    " | 試用今天到期".to_string()
                } else {
                    " | 試用已過期".to_string()
                }
            } else {
                String::new()
            }
        } else if let Some(next_reset) = &info.next_reset {
            format!(" | 重置{}", next_reset)
        } else {
            String::new()
        };
        
        // Color-code percentage: red (>90%), yellow (70-90%), green (<70%)
        let percentage_colored = if percentage >= 90 {
            ui::red(&format!("{}%", percentage))
        } else if percentage >= 70 {
            ui::yellow(&format!("{}%", percentage))
        } else {
            ui::green(&format!("{}%", percentage))
        };
        
        // Format: ● [Provider] alias | email | usage/limit (%) | trial days | status
        format!("{}{} {} | {} | {}/{} ({}){}| {}", 
            prefix, 
            provider,
            ui::bold(&ui::cyan(&account.alias)),
            ui::dimmed(&account.email),
            info.current_usage as i32,
            info.usage_limit as i32,
            percentage_colored,
            expiry_str,
            status)
    } else {
        // No account info available
        format!("{}{} {} | {} | {}", prefix, provider, ui::bold(&ui::cyan(&account.alias)), ui::dimmed(&account.email), ui::yellow("無資料"))
    }
}

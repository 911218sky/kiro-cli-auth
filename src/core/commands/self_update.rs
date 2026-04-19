use anyhow::{anyhow, Context, Result};
use crate::core::config;
use crate::ui;

/// Self-update command: download and install the latest release from GitHub
pub fn cmd_self_update(force: bool) -> Result<()> {
    println!("{}", ui::cyan("→ Checking for updates..."));
    
    // Fetch latest release info from GitHub API
    let api_url = config::github_latest_release_url();
    let response = ureq::get(&api_url)
        .set("User-Agent", "kiro-cli-auth")
        .timeout(config::api_timeout())
        .call()
        .context("Failed to fetch latest release info")?;
    
    let release: serde_json::Value = response.into_json()
        .context("Failed to parse release info")?;
    
    let latest_version = release["tag_name"].as_str()
        .ok_or_else(|| anyhow!("No tag_name in release"))?;
    
    let current_version = env!("CARGO_PKG_VERSION");
    let latest_version_str = latest_version.trim_start_matches('v');
    
    println!("{} Current version: {}", ui::cyan("→"), current_version);
    println!("{} Latest version: {}", ui::cyan("→"), latest_version_str);
    
    // Check if update is needed
    if !force && !is_newer_version(current_version, latest_version_str) {
        println!("{}", ui::green("✓ Already up to date"));
        return Ok(());
    }
    
    if force {
        println!("{}", ui::yellow("⚠ Force update enabled, skipping version check"));
    }
    
    // Get platform-specific asset name
    let asset_name = get_asset_name()?;
    
    // Find download URL
    let assets = release["assets"].as_array()
        .ok_or_else(|| anyhow!("No assets in release"))?;
    
    let download_url = assets.iter()
        .find(|a| a["name"].as_str() == Some(asset_name))
        .and_then(|a| a["browser_download_url"].as_str())
        .ok_or_else(|| anyhow!("Asset {} not found in release", asset_name))?;
    
    println!("{} Downloading {}...", ui::cyan("→"), asset_name);
    
    // Download binary
    let response = ureq::get(download_url)
        .timeout(config::api_timeout())
        .call()
        .context("Failed to download binary")?;
    
    let mut temp_file = tempfile::NamedTempFile::new()
        .context("Failed to create temp file")?;
    
    std::io::copy(&mut response.into_reader(), &mut temp_file)
        .context("Failed to write downloaded binary")?;
    
    let temp_path = temp_file.path();
    
    // Set executable permission on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(temp_path, std::fs::Permissions::from_mode(0o755))
            .context("Failed to set executable permission")?;
    }
    
    // Install the new binary
    install_binary(temp_path)?;
    
    println!("{} Successfully updated to {}", ui::green("✓"), latest_version);
    println!("{} Please restart kiro-cli-auth to use the new version", ui::yellow("⚠"));
    
    Ok(())
}

/// Compare two version strings (e.g., "1.2.3" vs "1.2.4")
/// Returns true if latest is newer than current
fn is_newer_version(current: &str, latest: &str) -> bool {
    let current_parts: Vec<u32> = current.split('.').filter_map(|s| s.parse().ok()).collect();
    let latest_parts: Vec<u32> = latest.split('.').filter_map(|s| s.parse().ok()).collect();
    
    // Warn if version format is unexpected
    if current_parts.len() != 3 {
        eprintln!("warn: current version '{}' has unexpected format", current);
    }
    if latest_parts.len() != 3 {
        eprintln!("warn: latest version '{}' has unexpected format", latest);
    }
    
    // Compare version parts
    for (l, c) in latest_parts.iter().zip(current_parts.iter()) {
        if l > c {
            return true;
        } else if l < c {
            return false;
        }
    }
    
    // If all parts are equal, check if latest has more parts
    latest_parts.len() > current_parts.len()
}

/// Get platform-specific asset name
fn get_asset_name() -> Result<&'static str> {
    if cfg!(target_os = "linux") {
        if cfg!(target_arch = "x86_64") {
            Ok("kiro-cli-auth-linux-x86_64")
        } else if cfg!(target_arch = "aarch64") {
            Ok("kiro-cli-auth-linux-aarch64")
        } else {
            Err(anyhow!("Unsupported Linux architecture"))
        }
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "x86_64") {
            Ok("kiro-cli-auth-macos-x86_64")
        } else if cfg!(target_arch = "aarch64") {
            Ok("kiro-cli-auth-macos-aarch64")
        } else {
            Err(anyhow!("Unsupported macOS architecture"))
        }
    } else if cfg!(target_os = "windows") {
        Ok("kiro-cli-auth-windows.exe")
    } else {
        Err(anyhow!("Unsupported platform"))
    }
}

/// Install the new binary, with backup and rollback on failure
fn install_binary(temp_path: &std::path::Path) -> Result<()> {
    let current_exe = std::env::current_exe()
        .context("Failed to get current executable path")?;
    
    println!("{} Installing to {:?}...", ui::cyan("→"), current_exe);
    
    let backup_path = current_exe.with_extension("old");
    
    // Remove stale backup if exists
    if backup_path.exists() {
        if let Err(e) = std::fs::remove_file(&backup_path) {
            eprintln!("warn: could not remove stale backup {:?}: {}", backup_path, e);
        }
    }
    
    // Backup current executable
    std::fs::rename(&current_exe, &backup_path)
        .context("Failed to backup current executable")?;
    
    // Install new binary; rollback on failure
    if let Err(copy_err) = std::fs::copy(temp_path, &current_exe) {
        eprintln!("error: failed to install new executable: {}", copy_err);
        if let Err(restore_err) = std::fs::rename(&backup_path, &current_exe) {
            eprintln!("error: rollback also failed — please manually restore {:?} to {:?}", 
                     backup_path, current_exe);
            return Err(anyhow!("Install failed and rollback failed: install={}, rollback={}", 
                              copy_err, restore_err));
        }
        return Err(anyhow!("Failed to install new executable (rolled back): {}", copy_err));
    }
    
    // Set executable permission on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&current_exe, std::fs::Permissions::from_mode(0o755))
            .context("Failed to set executable permission")?;
    }
    
    // Clean up backup on non-Windows platforms
    #[cfg(not(target_os = "windows"))]
    {
        if let Err(e) = std::fs::remove_file(&backup_path) {
            eprintln!("warn: could not remove backup {:?}: {}", backup_path, e);
        }
    }
    
    // On Windows, keep backup (will be cleaned up on next update)
    #[cfg(target_os = "windows")]
    println!("{} Backup saved to {:?} (will be cleaned up on next update)", 
             ui::cyan("→"), backup_path);
    
    Ok(())
}

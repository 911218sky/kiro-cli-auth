use anyhow::Result;
#[cfg(not(target_os = "linux"))]
use anyhow::Context;
use std::fs;
use std::path::PathBuf;
#[cfg(not(target_os = "linux"))]
use std::process::Command;

// macOS: Store in user's Application Support directory
#[cfg(target_os = "macos")]
#[allow(dead_code)]
pub fn get_machine_id_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("Library/Application Support/Kiro/machineid")
}

// Linux: Use system-wide machine-id
#[cfg(target_os = "linux")]
#[allow(dead_code)]
pub fn get_machine_id_path() -> PathBuf {
    PathBuf::from("/etc/machine-id")
}

// Windows: Registry path (marker only, actual access via reg command)
#[cfg(target_os = "windows")]
#[allow(dead_code)]
pub fn get_machine_id_path() -> PathBuf {
    PathBuf::from("HKLM\\SOFTWARE\\Microsoft\\Cryptography\\MachineGuid")
}

#[allow(dead_code)]
pub fn read_machine_id() -> Result<String> {
    #[cfg(target_os = "macos")]
    {
        let path = get_machine_id_path();
        if path.exists() {
            return fs::read_to_string(&path)
                .context("Failed to read macOS machine ID")
                .map(|s| s.trim().to_string());
        }
        // Fallback: extract hardware UUID via ioreg
        let output = Command::new("ioreg")
            .args(["-rd1", "-c", "IOPlatformExpertDevice"])
            .output()
            .context("Failed to execute ioreg")?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.contains("IOPlatformUUID") {
                // Parse: "IOPlatformUUID" = "XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX"
                if let Some(uuid) = line.split('"').nth(3) {
                    return Ok(uuid.trim().to_lowercase());
                }
            }
        }
        Err(anyhow::anyhow!("Failed to get macOS machine ID"))
    }

    #[cfg(target_os = "linux")]
    {
        // Try both standard locations
        let paths = ["/etc/machine-id", "/var/lib/dbus/machine-id"];
        for path in &paths {
            if let Ok(content) = fs::read_to_string(path) {
                let id = content.trim();
                if !id.is_empty() {
                    return Ok(format_as_uuid(id));
                }
            }
        }
        Err(anyhow::anyhow!("Failed to read Linux machine ID"))
    }

    #[cfg(target_os = "windows")]
    {
        // Query registry for MachineGuid
        let output = Command::new("reg")
            .args([
                "query",
                "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Cryptography",
                "/v",
                "MachineGuid",
            ])
            .output()
            .context("Failed to query Windows registry")?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.contains("MachineGuid") {
                if let Some(guid) = line.split_whitespace().last() {
                    return Ok(guid.trim().to_lowercase());
                }
            }
        }
        Err(anyhow::anyhow!("Failed to read Windows machine ID"))
    }
}

pub fn write_machine_id(machine_id: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let path = get_machine_id_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, machine_id.trim())
            .context("Failed to write macOS machine ID")?;
        Ok(())
    }

    #[cfg(target_os = "linux")]
    {
        // Linux machine-id is stored as raw hex (no dashes)
        let raw_id = machine_id.replace('-', "").to_lowercase();
        let paths = ["/etc/machine-id", "/var/lib/dbus/machine-id"];
        let mut success = false;
        for path in &paths {
            if let Ok(_) = fs::write(path, format!("{}\n", raw_id)) {
                success = true;
            }
        }
        if success {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Failed to write Linux machine ID (requires root)"))
        }
    }

    #[cfg(target_os = "windows")]
    {
        // Update registry via reg command (requires admin)
        let output = Command::new("reg")
            .args([
                "add",
                "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Cryptography",
                "/v",
                "MachineGuid",
                "/t",
                "REG_SZ",
                "/d",
                machine_id,
                "/f",
            ])
            .output()
            .context("Failed to execute reg command")?;
        
        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("Access is denied") || stderr.contains("拒絕存取") {
                Err(anyhow::anyhow!(
                    "Failed to write Windows machine ID: Administrator privileges required.\n\
                     Please run PowerShell as Administrator."
                ))
            } else {
                Err(anyhow::anyhow!(
                    "Failed to write Windows machine ID: {}", 
                    stderr.trim()
                ))
            }
        }
    }
}

// Convert 32-char hex string to UUID format: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
#[cfg(target_os = "linux")]
#[allow(dead_code)]
fn format_as_uuid(hex: &str) -> String {
    let clean = hex.replace('-', "").to_lowercase();
    if clean.len() != 32 {
        return clean;
    }
    format!(
        "{}-{}-{}-{}-{}",
        &clean[0..8],
        &clean[8..12],
        &clean[12..16],
        &clean[16..20],
        &clean[20..]
    )
}

#[cfg(not(target_os = "linux"))]
fn format_as_uuid(s: &str) -> String {
    s.to_string()
}

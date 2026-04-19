# kiro-cli-auth Windows Installer

$ErrorActionPreference = "Stop"

# Check if kiro-cli is installed
if (-not (Get-Command kiro-cli -ErrorAction SilentlyContinue)) {
    Write-Host "ERROR: kiro-cli is not installed" -ForegroundColor Red
    Write-Host "Please install kiro-cli first from: https://github.com/aws/kiro-cli" -ForegroundColor Yellow
    Read-Host "Press Enter to exit"
    exit 1
}

# Auto-elevate to Administrator (only when running as a file, not piped)
if ($PSCommandPath -and -not ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Start-Process powershell.exe "-NoProfile -ExecutionPolicy Bypass -File `"$PSCommandPath`"" -Verb RunAs
    exit
}

# Check admin when piped
if (-not ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Host "ERROR: Administrator privileges required." -ForegroundColor Red
    Write-Host "Please run PowerShell as Administrator and try again." -ForegroundColor Yellow
    Write-Host ""
    Write-Host "Or download and run the script:" -ForegroundColor Cyan
    Write-Host "  Invoke-WebRequest -Uri 'https://raw.githubusercontent.com/911218sky/kiro-cli-auth/main/install.ps1' -OutFile 'install.ps1'" -ForegroundColor Gray
    Write-Host "  .\install.ps1" -ForegroundColor Gray
    Read-Host "Press Enter to exit"
    exit 1
}

Write-Host "Installing kiro-cli-auth for Windows..." -ForegroundColor Cyan
Write-Host ""

# Get latest release info
Write-Host "Fetching latest release..." -ForegroundColor Cyan
$apiUrl = "https://api.github.com/repos/911218sky/kiro-cli-auth/releases/latest"
try {
    $release = Invoke-RestMethod -Uri $apiUrl -Headers @{ "User-Agent" = "kiro-cli-auth-installer" }
} catch {
    Write-Host "ERROR: Failed to fetch release info: $_" -ForegroundColor Red
    Read-Host "Press Enter to exit"
    exit 1
}

$version = $release.tag_name
$asset = $release.assets | Where-Object { $_.name -eq "kiro-cli-auth-windows.exe" }

if (-not $asset) {
    Write-Host "ERROR: Windows binary not found in release $version" -ForegroundColor Red
    Read-Host "Press Enter to exit"
    exit 1
}

# Find checksum asset
$checksumAsset = $release.assets | Where-Object { $_.name -eq "kiro-cli-auth-windows.exe.sha256" }

Write-Host "Latest version: $version" -ForegroundColor Green
Write-Host ""

# Download binary
$downloadUrl = $asset.browser_download_url
$tempFile = "$env:TEMP\kiro-cli-auth.exe"

Write-Host "Downloading from: $downloadUrl" -ForegroundColor Cyan
try {
    Invoke-WebRequest -Uri $downloadUrl -OutFile $tempFile -UseBasicParsing
} catch {
    Write-Host "ERROR: Failed to download: $_" -ForegroundColor Red
    Read-Host "Press Enter to exit"
    exit 1
}

# Verify checksum if available
if ($checksumAsset) {
    Write-Host "Verifying checksum..." -ForegroundColor Cyan
    try {
        $checksumContent = (Invoke-WebRequest -Uri $checksumAsset.browser_download_url -UseBasicParsing).Content.Trim()
        $expectedHash = ($checksumContent -split '\s+')[0].ToUpper()
        $actualHash = (Get-FileHash -Path $tempFile -Algorithm SHA256).Hash.ToUpper()
        if ($actualHash -ne $expectedHash) {
            Remove-Item -Path $tempFile -Force -ErrorAction SilentlyContinue
            Write-Host "ERROR: Checksum mismatch! File may be corrupted or tampered." -ForegroundColor Red
            Write-Host "  Expected: $expectedHash" -ForegroundColor Yellow
            Write-Host "  Got:      $actualHash" -ForegroundColor Yellow
            Read-Host "Press Enter to exit"
            exit 1
        }
        Write-Host "Checksum verified." -ForegroundColor Green
    } catch {
        Remove-Item -Path $tempFile -Force -ErrorAction SilentlyContinue
        Write-Host "ERROR: Failed to verify checksum: $_" -ForegroundColor Red
        Read-Host "Press Enter to exit"
        exit 1
    }
} else {
    Write-Host "WARNING: No checksum file found, skipping verification." -ForegroundColor Yellow
}

# Install to Program Files
$installDir = "$env:ProgramFiles\kiro-cli-auth"
Write-Host "Installing to: $installDir" -ForegroundColor Cyan

if (-not (Test-Path $installDir)) {
    New-Item -ItemType Directory -Path $installDir -Force | Out-Null
}

$installPath = "$installDir\kiro-cli-auth.exe"
try {
    Move-Item -Path $tempFile -Destination $installPath -Force
} catch {
    Remove-Item -Path $tempFile -Force -ErrorAction SilentlyContinue
    Write-Host "ERROR: Failed to install binary: $_" -ForegroundColor Red
    Read-Host "Press Enter to exit"
    exit 1
}

# Add to PATH if not already present
$currentPath = [Environment]::GetEnvironmentVariable("Path", "Machine")
if ($currentPath -notlike "*$installDir*") {
    Write-Host "Adding to system PATH..." -ForegroundColor Cyan
    [Environment]::SetEnvironmentVariable("Path", "$currentPath;$installDir", "Machine")
    Write-Host "PATH updated (restart terminal to take effect)" -ForegroundColor Yellow
} else {
    Write-Host "Already in PATH" -ForegroundColor Green
}

Write-Host ""
Write-Host "Installation successful!" -ForegroundColor Green
Write-Host "Location: $installPath" -ForegroundColor Cyan
Write-Host ""
Write-Host "Please restart your terminal, then run: kiro-cli-auth --version" -ForegroundColor Yellow
Write-Host ""
Read-Host "Press Enter to exit"

# kiro-cli-auth Windows Installer

$ErrorActionPreference = "Stop"

# Auto-elevate to Administrator
if (-not ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Start-Process powershell.exe "-NoProfile -ExecutionPolicy Bypass -File `"$PSCommandPath`"" -Verb RunAs
    exit
}

Write-Host "Installing kiro-cli-auth for Windows..." -ForegroundColor Cyan
Write-Host ""# Get latest release info
Write-Host "Fetching latest release..." -ForegroundColor Cyan
$apiUrl = "https://api.github.com/repos/911218sky/kiro-cli-auth/releases/latest"
try {
    $release = Invoke-RestMethod -Uri $apiUrl -Headers @{ "User-Agent" = "kiro-cli-auth-installer" }
} catch {
    Write-Host "ERROR: Failed to fetch release info: $_" -ForegroundColor Red
    exit 1
}

$version = $release.tag_name
$asset = $release.assets | Where-Object { $_.name -eq "kiro-cli-auth-windows.exe" }

if (-not $asset) {
    Write-Host "ERROR: Windows binary not found in release $version" -ForegroundColor Red
    exit 1
}

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
    exit 1
}

# Install to Program Files
$installDir = "$env:ProgramFiles\Kiro"
Write-Host "Installing to: $installDir" -ForegroundColor Cyan

if (-not (Test-Path $installDir)) {
    New-Item -ItemType Directory -Path $installDir -Force | Out-Null
}

$installPath = "$installDir\kiro-cli-auth.exe"
try {
    Move-Item -Path $tempFile -Destination $installPath -Force
} catch {
    Write-Host "ERROR: Failed to install binary: $_" -ForegroundColor Red
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

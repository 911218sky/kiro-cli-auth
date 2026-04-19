# kiro-cli-auth

Multi-account auth manager with AWS Builder ID and Google login. Each account gets its own machine ID.

## Install

### Linux

```bash
curl -fsSL https://raw.githubusercontent.com/911218sky/kiro-cli-auth/main/install.sh | sudo bash
```

Or manually:
```bash
# For x86_64:
wget https://github.com/911218sky/kiro-cli-auth/releases/latest/download/kiro-cli-auth-linux-x86_64
chmod +x kiro-cli-auth-linux-x86_64
sudo mv kiro-cli-auth-linux-x86_64 /usr/local/bin/kiro-cli-auth

# For ARM64:
wget https://github.com/911218sky/kiro-cli-auth/releases/latest/download/kiro-cli-auth-linux-aarch64
chmod +x kiro-cli-auth-linux-aarch64
sudo mv kiro-cli-auth-linux-aarch64 /usr/local/bin/kiro-cli-auth
```

### macOS

```bash
curl -fsSL https://raw.githubusercontent.com/911218sky/kiro-cli-auth/main/install.sh | sudo bash
```

Or manually:
```bash
# For Intel (x86_64):
curl -LO https://github.com/911218sky/kiro-cli-auth/releases/latest/download/kiro-cli-auth-macos-x86_64
chmod +x kiro-cli-auth-macos-x86_64
sudo mv kiro-cli-auth-macos-x86_64 /usr/local/bin/kiro-cli-auth

# For Apple Silicon (ARM64):
curl -LO https://github.com/911218sky/kiro-cli-auth/releases/latest/download/kiro-cli-auth-macos-aarch64
chmod +x kiro-cli-auth-macos-aarch64
sudo mv kiro-cli-auth-macos-aarch64 /usr/local/bin/kiro-cli-auth
```

### Windows

**Run in PowerShell as Administrator:**
```powershell
irm https://raw.githubusercontent.com/911218sky/kiro-cli-auth/main/install.ps1 | iex
```

Or manually download from [releases](https://github.com/911218sky/kiro-cli-auth/releases/latest) and place in `C:\Program Files\kiro-cli-auth\`

## Update

```bash
kiro-cli-auth self-update
```

Or reinstall using the install script (same as installation).

## Uninstall

### Linux/macOS

```bash
sudo rm /usr/local/bin/kiro-cli-auth
rm -rf ~/.kiro-cli-auth
```

### Windows

Run in PowerShell as Administrator:

```powershell
Remove-Item "C:\Program Files\kiro-cli-auth\kiro-cli-auth.exe" -Force
Remove-Item "$env:APPDATA\kiro-cli-auth" -Recurse -Force
```

## Usage

```bash
# Login
kiro-cli-auth login myaccount

# List accounts
kiro-cli-auth list

# Switch account (syncs machine ID)
kiro-cli-auth switch myaccount

# Current account
kiro-cli-auth current

# Remove account
kiro-cli-auth remove myaccount
```

## Machine ID

Each account gets a unique UUID machine ID on first switch. Stored in database and optionally synced to system:

- **Linux**: `/etc/machine-id` (requires root)
- **macOS**: `~/Library/Application Support/Kiro/machineid`
- **Windows**: Registry `HKLM\SOFTWARE\Microsoft\Cryptography\MachineGuid` (requires admin)

Without elevated permissions, machine ID is saved to database only — this works fine.

## Data Location

**Linux/macOS**: `~/.kiro-cli-auth/`  
**Windows**: `%APPDATA%\kiro-cli-auth\`

## Configuration

### Environment Variables

**`KIRO_CLI_AUTH_DIR`** - Change kiro-cli-auth data storage location

Default location:
- Linux/macOS: `~/.kiro-cli-auth/`
- Windows: `%APPDATA%\kiro-cli-auth\`

Example:
```bash
export KIRO_CLI_AUTH_DIR=/custom/path
# Account data will now be stored in /custom/path/ instead of ~/.kiro-cli-auth/
```

**`KIRO_NO_CACHE`** - Disable cache globally

Set to `1`, `true`, or `yes` to disable cache for all commands.

Example:
```bash
export KIRO_NO_CACHE=1
# All commands will fetch fresh data from API instead of using cache
```

**`KIRO_CACHE_TTL`** - Cache time-to-live in seconds

Default: `300` (5 minutes)

Example:
```bash
export KIRO_CACHE_TTL=600
# Cache will be valid for 10 minutes
```

**`KIRO_CACHE_PATH`** - Custom cache database location

Default: `/tmp/kiro-cli-auth-cache.db`

Example:
```bash
export KIRO_CACHE_PATH=/var/cache/kiro-cli-auth.db
# Cache will be stored at custom location
```

**`KIRO_API_TIMEOUT`** - API request timeout in seconds

Default: `15` seconds

Example:
```bash
export KIRO_API_TIMEOUT=30
# API requests will timeout after 30 seconds
```

**`XDG_DATA_HOME`** - Change kiro-cli main program data location

kiro-cli-auth needs to read kiro-cli's main database to sync machine ID. This variable tells it where to find kiro-cli's database.

Default search order:
1. `$XDG_DATA_HOME/kiro-cli/data.sqlite3` (if XDG_DATA_HOME is set)
2. `~/.local/share/kiro-cli/data.sqlite3`
3. `~/.config/kiro-cli/data.sqlite3`
4. `~/.kiro-cli/data.sqlite3`

Example:
```bash
export XDG_DATA_HOME=/my/data
# kiro-cli-auth will look for kiro-cli database at /my/data/kiro-cli/data.sqlite3
```

**In short:**
- `KIRO_CLI_AUTH_DIR` = where kiro-cli-auth stores its own data
- `KIRO_NO_CACHE` = disable cache globally
- `KIRO_CACHE_TTL` = how long cache is valid
- `KIRO_CACHE_PATH` = where cache is stored
- `KIRO_API_TIMEOUT` = API request timeout
- `XDG_DATA_HOME` = where kiro-cli main program stores its data

## Commands

```bash
kiro-cli-auth login [alias]      # Login with optional alias
kiro-cli-auth list               # List all accounts
kiro-cli-auth list --no-cache    # List accounts without using cache
kiro-cli-auth switch [alias]     # Switch account (interactive if no alias)
kiro-cli-auth current            # Show current account
kiro-cli-auth remove <alias>     # Remove account
kiro-cli-auth logout             # Logout current account
kiro-cli-auth clean              # Clean invalid accounts
kiro-cli-auth export <file>      # Export accounts
kiro-cli-auth import <file>      # Import accounts
kiro-cli-auth self-update        # Update to latest version
```

### Cache Control

The `list` command uses a 5-minute cache by default to reduce API requests. You can bypass the cache in two ways:

**Option 1: Command-line flag**
```bash
kiro-cli-auth list --no-cache
```

**Option 2: Environment variable**
```bash
# Disable cache for all commands
export KIRO_NO_CACHE=1
kiro-cli-auth list

# Or for a single command
KIRO_NO_CACHE=1 kiro-cli-auth list
```
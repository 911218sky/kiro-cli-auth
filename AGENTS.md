# AI Agent Instructions for kiro-cli-auth

## Quick Reference
- **Architecture**: [Directory structure](#architecture) | [Data flows](#data-flow)
- **Add command**: [Steps](#adding-a-new-command) → [Security checklist](#security-checklist)
- **Error handling**: Use `thiserror::Error`, never `unwrap()` in production
- **Key files**: `registry.db` (SQLite metadata), `~/.kiro-cli-auth/accounts/` (backups)

## Project Overview
Rust CLI tool managing multiple Kiro CLI accounts with instant switching.

## Architecture

| Path | Purpose | Created When |
|------|---------|--------------|
| `~/.kiro/` | Active account data | Kiro CLI first run |
| `~/.kiro-cli-auth/registry.db` | Account metadata (SQLite) | First `login` |
| `~/.kiro-cli-auth/accounts/{alias}.sqlite3` | Account backups | `login` or `switch` |

### Data Flow
**Login**: `~/.kiro/` → backup to `accounts/{alias}.sqlite3` → update registry.db  
**Switch**: validate alias → atomic backup current → restore target → update registry.db

## Code Style
- Use `thiserror` for structured errors, `anyhow` only in `main.rs`
- **Never** use `unwrap()` or `expect()` in production code
- Implement atomic operations with rollback on failure

## Common Tasks

### Adding a new command
1. Add variant to `Commands` enum
2. Implement handler with proper error types
3. Update registry if needed
4. **⚠️ Run [Security Checklist](#security-checklist)**

### Error handling pattern
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("Invalid alias: {0}")]
    InvalidAlias(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

fn backup_account(alias: &str) -> Result<(), AuthError> {
    validate_alias(alias)?;
    let src = dirs::home_dir().ok_or(AuthError::HomeNotFound)?.join(".kiro");
    // Atomic operation: backup current before replacing
    Ok(())
}
```

## Security Checklist
- [ ] No credentials in logs (use `[REDACTED]`)
- [ ] File permissions 0600 (`std::os::unix::fs::PermissionsExt`)
- [ ] Alias validated (reject `../`, `/`, `\`)
- [ ] Sensitive data cleared (`zeroize` crate for tokens)

```rust
fn validate_alias(alias: &str) -> Result<(), AuthError> {
    if alias.contains("..") || alias.contains('/') || alias.contains('\\') {
        return Err(AuthError::InvalidAlias(alias.to_string()));
    }
    Ok(())
}
```

## Dependencies
- `clap`, `anyhow`, `dirs`, `dialoguer`, `inquire`
- `rusqlite` (SQLite database), `serde`, `chrono`
- `thiserror` (structured errors), `zeroize` (credential cleanup)

## Questions to Ask
- Is this operation atomic? (Can it be rolled back on failure?)
- Does registry schema change? (Migration needed?)
- Cross-platform compatible? (Windows/macOS/Linux)
- Error message actionable for users?

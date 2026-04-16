use anyhow::Result;

/// Sentinel error returned when the user cancels an interactive prompt (Ctrl-C / Esc).
/// Callers can downcast to distinguish cancellation from real failures.
#[derive(Debug)]
pub struct UserCancelled;

impl std::fmt::Display for UserCancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cancelled")
    }
}

impl std::error::Error for UserCancelled {}

pub fn cyan(s: &str) -> String {
    format!("\x1b[36m{}\x1b[0m", s)
}

pub fn green(s: &str) -> String {
    format!("\x1b[32m{}\x1b[0m", s)
}

pub fn yellow(s: &str) -> String {
    format!("\x1b[33m{}\x1b[0m", s)
}

pub fn red(s: &str) -> String {
    format!("\x1b[31m{}\x1b[0m", s)
}

pub fn magenta(s: &str) -> String {
    format!("\x1b[35m{}\x1b[0m", s)
}

pub fn dimmed(s: &str) -> String {
    format!("\x1b[2m{}\x1b[0m", s)
}

pub fn bold(s: &str) -> String {
    format!("\x1b[1m{}\x1b[0m", s)
}

/// Single-selection prompt. Returns the index of the selected item.
/// Converts Ctrl-C/Esc into UserCancelled error.
pub fn select(prompt: &str, items: &[String]) -> Result<usize> {
    let selected = inquire::Select::new(prompt, items.to_vec())
        .prompt()
        .map_err(|e| match e {
            inquire::InquireError::OperationCanceled
            | inquire::InquireError::OperationInterrupted => {
                anyhow::Error::new(UserCancelled)
            }
            other => anyhow::Error::new(other).context("Selection failed"),
        })?;

    items
        .iter()
        .position(|item| item == &selected)
        .ok_or_else(|| anyhow::anyhow!("Selection not found"))
}

/// Multi-selection prompt. Returns indices of all selected items.
/// Converts Ctrl-C/Esc into UserCancelled error.
pub fn multi_select(prompt: &str, items: &[String]) -> Result<Vec<usize>> {
    let selected = inquire::MultiSelect::new(prompt, items.to_vec())
        .with_help_message("[↑↓ to move, space to select one, → to all, ← to none, type to filter]")
        .prompt()
        .map_err(|e| match e {
            inquire::InquireError::OperationCanceled
            | inquire::InquireError::OperationInterrupted => {
                anyhow::Error::new(UserCancelled)
            }
            other => anyhow::Error::new(other).context("Multi-selection failed"),
        })?;

    let indices: Vec<usize> = selected
        .iter()
        .filter_map(|item| items.iter().position(|i| i == item))
        .collect();

    Ok(indices)
}

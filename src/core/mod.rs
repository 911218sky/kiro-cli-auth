// Core modules for Kiro CLI authentication and data management
pub mod models;
pub mod data;
pub mod auth;
pub mod fs;
pub mod transfer;
pub mod commands;
pub mod machine_id;

use anyhow::{Context, Result};
use base64::Engine;

#[allow(dead_code)]
pub fn encode_base64(data: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(data)
}

#[allow(dead_code)]
pub fn decode_base64(s: &str) -> Result<Vec<u8>> {
    base64::engine::general_purpose::STANDARD.decode(s)
        .context("Invalid base64 encoding")
}

//! Mapping domain errors into anyhow with context.

use anyhow::{Context, Result};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("not found: {0}")]
    NotFound(String),
}

fn load_item(key: &str) -> std::result::Result<String, StoreError> {
    Err(StoreError::NotFound(key.to_string()))
}

pub fn error_composition_demo() -> String {
    let result: Result<String> = load_item("missing-key")
        .context("loading item failed")
        .map(|v| format!("loaded {v}"));

    format!("{result:?}")
}


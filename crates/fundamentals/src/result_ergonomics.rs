//! Using `?` and mapping errors across layers.

use anyhow::{Context, Result};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("empty input")]
    Empty,
    #[error("not a number")]
    NotNumber,
}

fn parse_positive(input: &str) -> std::result::Result<u32, ParseError> {
    if input.trim().is_empty() {
        return Err(ParseError::Empty);
    }
    let value: u32 = input.trim().parse().map_err(|_| ParseError::NotNumber)?;
    if value == 0 {
        return Err(ParseError::NotNumber);
    }
    Ok(value)
}

fn compute(input: &str) -> Result<u32> {
    let n = parse_positive(input).context("parse failed")?;
    Ok(n * 2)
}

pub fn result_flow_demo() -> String {
    let ok = compute("7");
    let bad = compute("oops");

    format!("ok: {:?}\nbad: {:?}", ok, bad)
}


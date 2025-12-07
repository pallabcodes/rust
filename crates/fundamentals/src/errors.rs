//! Shows Result and custom errors.

use anyhow::Result;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    // thiserror derives Display + std::error::Error.
    #[error("input was empty")]
    Empty,
    #[error("not a positive integer: {0}")]
    NotPositive(i64),
}

fn parse_positive(input: &str) -> Result<i64, ParseError> {
    if input.trim().is_empty() {
        return Err(ParseError::Empty);
    }
    let value: i64 = input.trim().parse().map_err(|_| ParseError::NotPositive(0))?;
    if value <= 0 {
        return Err(ParseError::NotPositive(value));
    }
    Ok(value)
}

pub fn errors_demo() -> String {
    let ok = parse_positive("7");
    let bad = parse_positive("-2");
    let empty = parse_positive("");

    let lines = vec![
        format!("ok result: {:?}", ok),
        format!("bad result: {:?}", bad),
        format!("empty result: {:?}", empty),
    ];

    lines.join("\n")
}


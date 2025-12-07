//! Minimal JSON file write/read round trip.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Config {
    feature: String,
    enabled: bool,
}

pub fn file_io_demo() -> String {
    let cfg = Config {
        feature: "demo".to_string(),
        enabled: true,
    };

    match round_trip(&cfg) {
        Ok(ok) => ok,
        Err(err) => format!("file round trip failed: {err:?}"),
    }
}

fn round_trip(cfg: &Config) -> Result<String> {
    let dir = std::env::temp_dir();
    let path: PathBuf = dir.join("fundamentals_config.json");

    let json = serde_json::to_string_pretty(cfg)?;
    fs::write(&path, json)?;

    let read_back = fs::read_to_string(&path)?;
    let decoded: Config = serde_json::from_str(&read_back)?;

    Ok(format!(
        "path: {}, decoded == original: {}",
        path.display(),
        decoded == *cfg
    ))
}


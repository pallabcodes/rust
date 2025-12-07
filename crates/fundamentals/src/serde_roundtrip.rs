//! Serde serialize/deserialize round trip.

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Profile {
    name: String,
    level: u8,
}

pub fn serde_demo() -> String {
    let profile = Profile {
        name: "Ava".to_string(),
        level: 3,
    };

    let json = serde_json::to_string(&profile).unwrap();
    let decoded: Profile = serde_json::from_str(&json).unwrap();

    format!("json: {json}, round-trip equal: {}", profile == decoded)
}


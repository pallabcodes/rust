//! Cargo workspace and feature basics.

pub fn cargo_demo() -> String {
    let points = [
        "Workspace root sets shared edition, rust-version, deps.",
        "Member crates inherit but can override package metadata.",
        "Use features to gate optional code; prefer additive flags.",
        "Profiles: dev for fast rebuild, release for benchmarks.",
        "Pin direct deps; keep lockfiles per app when deploying.",
    ];

    points.join("\n")
}


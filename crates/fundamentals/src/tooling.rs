//! Notes on fmt and clippy for consistency.

pub fn tooling_demo() -> String {
    let steps = [
        "cargo fmt to enforce style",
        "cargo clippy -- -D warnings to keep lints clean",
        "run tests with cargo test",
    ];

    steps.join("\n")
}


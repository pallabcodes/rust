//! Shows immutable bindings, mutability, and shadowing.

pub fn variables_demo() -> String {
    let immutable = 5;

    let mut counter = 0;
    counter += 1;

    let shadowed = "hello";
    let shadowed = format!("{shadowed} world");

    let lines = vec![
        format!("immutable: {immutable}"),
        format!("counter (mut): {counter}"),
        format!("shadowed: {shadowed}"),
    ];

    lines.join("\n")
}


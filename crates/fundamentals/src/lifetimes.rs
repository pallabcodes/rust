//! Shows explicit lifetimes in signatures and structs.

// 'a is a lifetime label. It names how long references are valid (compile time only).
// Here we say: both inputs and the output share the same borrow, so the return
// can never outlive the shorter of the two inputs.
fn longest<'a>(left: &'a str, right: &'a str) -> &'a str {
    if left.len() >= right.len() {
        left
    } else {
        right
    }
}

// Struct holding a borrowed reference needs the same lifetime annotation.
struct Holder<'a> {
    value: &'a str,
}

pub fn lifetimes_demo() -> String {
    let first = String::from("alpha");
    let second = "beta";
    let longer = longest(first.as_str(), second);

    let holder = Holder { value: longer };

    let lines = vec![
        format!("longer str: {longer}"),
        format!("holder keeps: {}", holder.value),
    ];

    lines.join("\n")
}


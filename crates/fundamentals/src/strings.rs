//! String versus str slices and safe slicing.

pub fn strings_demo() -> String {
    let owned = String::from("hello rust");
    let borrowed: &str = &owned;

    let slice = borrowed.get(0..5).unwrap_or("");
    let utf8_safe = "héllo";
    let first_two = utf8_safe.chars().take(2).collect::<String>();

    let parts: Vec<&str> = borrowed.split_whitespace().collect();

    let lines = vec![
        format!("owned String: {owned}"),
        format!("borrowed &str: {borrowed}"),
        format!("slice[0..5]: {slice}"),
        format!("utf8 safe chars take 2: {first_two}"),
        format!("split words: {:?}", parts),
    ];

    lines.join("\n")
}


//! Moves versus borrows inside collections.

pub fn collections_ownership_demo() -> String {
    let mut names = vec!["amy".to_string(), "bob".to_string()];

    let borrowed: Vec<&str> = names.iter().map(|s| s.as_str()).collect();

    let moved = names.pop().unwrap();
    names.push("cory".to_string());

    let cloned = names[0].clone();

    let lines = vec![
        format!("borrowed view: {:?}", borrowed),
        format!("moved out: {moved}, remaining: {:?}", names),
        format!("cloned first: {cloned} (original still {})", names[0]),
    ];

    lines.join("\n")
}


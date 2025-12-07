//! Introduces Vec and HashMap iteration.

use std::collections::HashMap;

pub fn collections_demo() -> String {
    let nums = vec![1, 2, 3, 4];
    let doubled: Vec<_> = nums.iter().map(|n| n * 2).collect();

    let mut scores = HashMap::new();
    scores.insert("alice", 10);
    scores.insert("bob", 8);

    let average = scores.values().copied().sum::<i32>() as f32 / scores.len() as f32;

    let lines = vec![
        format!("original: {:?}, doubled: {:?}", nums, doubled),
        format!("scores: {:?}", scores),
        format!("average score: {:.1}", average),
    ];

    lines.join("\n")
}


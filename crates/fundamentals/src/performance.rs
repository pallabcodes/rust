//! Allocation and timing basics without external benches.

use std::time::Instant;

fn sum_owned(nums: Vec<i32>) -> i32 {
    nums.iter().sum()
}

fn sum_borrowed(nums: &[i32]) -> i32 {
    nums.iter().sum()
}

pub fn performance_demo() -> String {
    let data: Vec<_> = (0..1_000).collect();

    let owned_start = Instant::now();
    let owned = sum_owned(data.clone());
    let owned_elapsed = owned_start.elapsed();

    let borrowed_start = Instant::now();
    let borrowed = sum_borrowed(&data);
    let borrowed_elapsed = borrowed_start.elapsed();

    let lines = vec![
        format!("owned sum: {owned} in {:?}", owned_elapsed),
        format!("borrowed sum: {borrowed} in {:?}", borrowed_elapsed),
        String::from("Use borrowed slices to avoid extra allocations when possible"),
    ];

    lines.join("\n")
}


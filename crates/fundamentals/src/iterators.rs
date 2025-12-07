//! Illustrates iterator adapters and closures.

pub fn iterators_demo() -> String {
    let nums = vec![1, 2, 3, 4, 5];

    // iter() borrows; copied() turns &i32 into i32 to own the values.
    let evens: Vec<_> = nums.iter().copied().filter(|n| n % 2 == 0).collect();
    // fold accumulates with a closure; map produces squared values lazily.
    let squares_sum: i32 = nums.iter().map(|n| n * n).fold(0, |acc, v| acc + v);

    let offset = 10;
    let offset_vals: Vec<_> = nums.iter().map(|n| n + offset).collect();

    let lines = vec![
        format!("evens: {:?}", evens),
        format!("sum of squares: {}", squares_sum),
        format!("offset by {offset}: {:?}", offset_vals),
    ];

    lines.join("\n")
}


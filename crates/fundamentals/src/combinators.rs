//! Option/Result combinators for succinct flow.

fn first_even(nums: &[i32]) -> Option<i32> {
    nums.iter().copied().find(|n| n % 2 == 0)
}

fn parse_positive(input: &str) -> Result<u32, String> {
    input
        .trim()
        .parse::<u32>()
        .map_err(|_| "not a number".to_string())
        .and_then(|n| if n > 0 { Ok(n) } else { Err("must be > 0".to_string()) })
}

pub fn combinators_demo() -> String {
    let nums = [1, 3, 4, 7];
    let found = first_even(&nums).map(|n| n * 10).unwrap_or(0);

    let ok = parse_positive("12").map(|n| n + 1);
    let bad = parse_positive("-3").unwrap_or_else(|e| {
        format!("error: {e}").parse().unwrap_or(0)
    });

    let lines = vec![
        format!("first even x10: {found}"),
        format!("parsed positive +1: {:?}", ok),
        format!("negative path yields: {bad}"),
    ];

    lines.join("\n")
}


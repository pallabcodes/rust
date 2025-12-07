//! Shows a tested helper and table-style verification.

fn add(left: i32, right: i32) -> i32 {
    left + right
}

pub fn testing_demo() -> String {
    let cases = [(1, 2), (5, 7), (-1, 3)];
    let results: Vec<_> = cases
        .iter()
        .map(|(l, r)| format!("{l} + {r} = {}", add(*l, *r)))
        .collect();

    results.join("\n")
}

#[cfg(test)]
mod tests {
    use super::add;

    #[test]
    fn adds_positive() {
        assert_eq!(add(2, 3), 5);
    }

    #[test]
    fn adds_negative() {
        assert_eq!(add(-2, 3), 1);
    }
}


//! Demonstrates closure capture modes and Fn/FnMut/FnOnce.

// Fn means no mutation or move from the captured environment.
fn apply_fn<F: Fn(i32) -> i32>(f: F, v: i32) -> i32 {
    f(v)
}

// FnMut lets the closure mutate captured variables.
fn apply_fn_mut<F: FnMut(i32) -> i32>(mut f: F, v: i32) -> i32 {
    f(v)
}

// FnOnce consumes captured values (takes ownership).
fn apply_fn_once<F: FnOnce(String) -> usize>(f: F, v: String) -> usize {
    f(v)
}

pub fn closures_demo() -> String {
    let mut total = 0;
    let mut add = |v| {
        total += v;
        total
    };
    let first = add(2);
    let second = add(3);

    let square = |v| v * v;
    let squared = apply_fn(square, 4);

    let mut scaler = 10;
    let scaled = apply_fn_mut(|v| {
        scaler += 1;
        v * scaler
    }, 2);

    let owned = String::from("owned move");
    let consumed = apply_fn_once(|s: String| s.len(), owned);

    // move forces capture by value, even though we only read later.
    let captured = vec![1, 2, 3];
    let moved_sum = {
        let closure = move || captured.iter().sum::<i32>();
        closure()
    };

    let lines = vec![
        format!("add runs: {first} then {second}"),
        format!("square via Fn: {squared}"),
        format!("FnMut scaled: {scaled} with scaler {}", scaler),
        format!("FnOnce consumed len: {consumed}"),
        format!("moved Vec sum: {moved_sum}"),
    ];

    lines.join("\n")
}


//! Demonstrates ownership moves versus Copy types.

// 1.Each value has an owner
// 2.There can only be one owner at a time
// 3.When the owner goes out of scope, the value is dropped
// At any given time, either exactly one mutable reference or any number of immutable references
// Takes ownership (moves) because String is not Copy.
fn takes_ownership(input: String) -> usize {
    input.len()
}

// Borrows immutably; caller retains ownership.
fn borrows(input: &str) -> usize {
    input.len()
}

pub fn ownership_demo() -> String {
    let text = String::from("rust");
    let moved_len = takes_ownership(text);

    let copy_num: i32 = 42;
    let copied_num = copy_num;

    let borrowed_len = borrows("borrowed");

    let lines = vec![
        format!("moved string length: {moved_len}"),
        format!("copy still usable: {copy_num} copied as {copied_num}"),
        format!("borrowed length: {borrowed_len}"),
    ];

    lines.join("\n")
}
//! Shows shared versus mutable borrowing.

// Shared borrow: many readers allowed.
fn count_words(text: &str) -> usize {
    text.split_whitespace().count()
}

// Mutable borrow: exclusive access required.
fn append_exclaim(text: &mut String) {
    text.push('!');
}

pub fn borrowing_demo() -> String {
    let sentence = String::from("borrowing keeps data alive");
    let word_count = count_words(&sentence);

    let mut loud = sentence.clone();
    append_exclaim(&mut loud);

    let lines = vec![
        format!("shared borrow word count: {word_count}"),
        format!("mutable borrow after mutation: {loud}"),
        format!("original still usable: {sentence}"),
    ];

    lines.join("\n")
}


//! Thread spawning patterns with naming and scoped borrowing.

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

pub fn threads_demo() -> String {
    // Builder lets us name threads or tune stack size.
    let builder = thread::Builder::new().name("named-worker".into());
    let handle = builder
        .spawn(|| {
            thread::sleep(Duration::from_millis(5));
            "done".to_string()
        })
        .expect("spawn");

    let named_result = handle.join().expect("join");

    // scoped spawns can borrow stack data safely without 'static bounds.
    let (tx, rx) = mpsc::channel();
    thread::scope(|scope| {
        let values = [1, 2, 3];
        scope.spawn(|| {
            for v in values {
                tx.send(v * 2).unwrap();
            }
        });
    });
    // Close the channel so iterator ends.
    drop(tx);
    let collected: Vec<_> = rx.into_iter().collect();

    let lines = vec![
        format!("named thread result: {named_result}"),
        format!("scoped doubles: {:?}", collected),
    ];

    lines.join("\n")
}


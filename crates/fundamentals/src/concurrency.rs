//! Shows threads, channels, and a minimal async task.

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

pub fn concurrency_demo() -> String {
    let (tx, rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        thread::sleep(Duration::from_millis(5));
        tx.send(String::from("thread finished")).unwrap();
    });

    let thread_msg = rx.recv().unwrap();
    worker.join().unwrap();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("runtime");
    let async_msg = rt.block_on(async {
        tokio::time::sleep(Duration::from_millis(5)).await;
        "async task finished".to_string()
    });

    let lines = vec![
        thread_msg,
        async_msg,
    ];

    lines.join("\n")
}


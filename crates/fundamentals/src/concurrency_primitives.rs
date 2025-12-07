//! RwLock, OnceLock, Barrier, and timeouts.

use std::sync::{Arc, Barrier, OnceLock, RwLock};
use std::thread;
use std::time::Duration;

pub fn concurrency_primitives_demo() -> String {
    let shared = Arc::new(RwLock::new(0));
    {
        let mut guard = shared.write().unwrap();
        *guard = 7;
    }
    let read_val = *shared.read().unwrap();

    static CONFIG: OnceLock<String> = OnceLock::new();
    let init = CONFIG.get_or_init(|| "configured".to_string()).clone();

    let barrier = Arc::new(Barrier::new(2));
    let b2 = barrier.clone();
    let worker = thread::spawn(move || {
        thread::sleep(Duration::from_millis(2));
        b2.wait();
        "worker passed barrier"
    });
    barrier.wait();
    let barrier_msg = worker.join().unwrap();

    let timed = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("runtime");
    let timeout_msg = timed.block_on(async {
        let slow = tokio::time::sleep(Duration::from_millis(5));
        match tokio::time::timeout(Duration::from_millis(1), slow).await {
            Ok(_) => "completed inside timeout".to_string(),
            Err(_) => "timed out task".to_string(),
        }
    });

    let lines = vec![
        format!("RwLock read: {read_val}"),
        format!("OnceLock: {init}"),
        barrier_msg.to_string(),
        format!("timeout result: {timeout_msg}"),
    ];

    lines.join("\n")
}


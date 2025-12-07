//! Basic async/await with cancellation via abort.

use std::time::Duration;

async fn fetch(id: u32, delay_ms: u64) -> String {
    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    format!("fetched {id}")
}

pub fn async_primer_demo() -> String {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("runtime");

    rt.block_on(async {
        // One task via spawn (JoinHandle), one inline future.
        let mut handle = tokio::spawn(fetch(1, 20));
        let fast = fetch(2, 5);

        let first = tokio::select! {
            v = &mut handle => format!("first via handle: {:?}", v),
            v = fast => format!("first via fast: {v}"),
        };

        // Abort cancels the spawned task if it is still running.
        handle.abort();

        format!("{first}\nsecond task aborted")
    })
}


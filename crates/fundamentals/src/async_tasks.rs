//! Spawning multiple async tasks and joining results.

use std::time::Duration;

async fn work(id: u32, delay_ms: u64) -> String {
    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    format!("task {id} done")
}

pub fn async_tasks_demo() -> String {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("runtime");

    let outputs = rt.block_on(async {
        // spawn returns JoinHandle; tasks run concurrently on the runtime.
        let mut t1 = tokio::spawn(work(1, 5));
        let mut t2 = tokio::spawn(work(2, 2));
        let mut t3 = tokio::spawn(work(3, 1));
        let mut t4 = tokio::spawn(work(4, 10));

        // select! awaits whichever future completes first.
        let first_done = tokio::select! {
            res = &mut t3 => format!("select winner: {:?}", res.unwrap()),
            res = &mut t4 => format!("slow winner: {:?}", res.unwrap()),
        };

        let res1 = t1.await.expect("task 1 join");
        let res2 = t2.await.expect("task 2 join");

        // Ensure the losing select handle is awaited to avoid cancellation noise.
        let _ = t3.await;
        let _ = t4.await;

        vec![res1, res2, first_done]
    });

    outputs.join("\n")
}


//! Send and Sync traits gate cross-thread usage.

fn assert_send_sync<T: Send + Sync>() {}

pub fn send_sync_demo() -> String {
    assert_send_sync::<i32>();
    assert_send_sync::<String>();

    let rc_note = "Rc<T> is !Send and !Sync; prefer Arc for threads.";
    let cell_note = "RefCell<T> is !Sync; use Mutex/Arc<Mutex> across threads.";
    let auto_note = "Most std types are Send/Sync if their contents are.";

    [rc_note, cell_note, auto_note].join("\n")
}


//! Smart pointers and interior mutability basics.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Debug)]
struct Node {
    value: i32,
    next: Option<Rc<Node>>,
}

#[derive(Debug)]
struct Counter {
    inner: RefCell<i32>,
}

impl Counter {
    fn increment(&self) {
        *self.inner.borrow_mut() += 1;
    }

    fn get(&self) -> i32 {
        *self.inner.borrow()
    }
}

#[derive(Debug)]
struct DropLogger(&'static str);

impl Drop for DropLogger {
    fn drop(&mut self) {
        println!("dropping {}", self.0);
    }
}

pub fn smart_pointers_demo() -> String {
    // Box moves data to the heap while keeping ownership simple.
    let boxed = Box::new(DropLogger("boxed value"));
    let boxed_note = format!("boxed ptr holds: {:?}", boxed);

    // Rc models shared ownership on a single thread.
    let third = Rc::new(Node { value: 3, next: None });
    let second = Rc::new(Node {
        value: 2,
        next: Some(third.clone()),
    });
    let first = Rc::new(Node {
        value: 1,
        next: Some(second.clone()),
    });

    let lengths = format!(
        "Rc strong counts: first {}, second {}, third {}",
        Rc::strong_count(&first),
        Rc::strong_count(&second),
        Rc::strong_count(&third)
    );

    // RefCell enables interior mutability while staying single-threaded.
    let counter = Counter {
        inner: RefCell::new(0),
    };
    counter.increment();
    counter.increment();

    // Arc<Mutex> shares data across threads safely.
    let shared = Arc::new(Mutex::new(0));
    let mut threads = Vec::new();
    for _ in 0..2 {
        let cloned = shared.clone();
        threads.push(thread::spawn(move || {
            let mut guard = cloned.lock().unwrap();
            *guard += 1;
        }));
    }
    for handle in threads {
        handle.join().unwrap();
    }
    let arc_count = Arc::strong_count(&shared);
    let arc_value = *shared.lock().unwrap();

    let lines = vec![
        boxed_note,
        lengths,
        format!("counter via RefCell: {}", counter.get()),
        format!("Arc<Mutex> value: {}, refs: {}", arc_value, arc_count),
        String::from("DropLogger prints when leaving scope"),
    ];

    lines.join("\n")
}
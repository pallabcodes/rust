//! Pin and self-referential safety basics.

use std::marker::PhantomPinned;
use std::pin::Pin;

struct SelfRef {
    data: String,
    ptr: *const String,
    _pin: PhantomPinned,
}

impl SelfRef {
    fn new(text: &str) -> Pin<Box<SelfRef>> {
        let data = text.to_string();
        let s = SelfRef {
            data,
            ptr: std::ptr::null(),
            _pin: PhantomPinned,
        };
        let mut boxed = Box::pin(s);
        let ptr = &boxed.data as *const String;
        unsafe {
            let mut_ref = Pin::as_mut(&mut boxed);
            Pin::get_unchecked_mut(mut_ref).ptr = ptr;
        }
        boxed
    }

    fn check_ptr(self: Pin<&Self>) -> bool {
        std::ptr::eq(self.data.as_ptr(), unsafe { (*self.ptr).as_ptr() })
    }
}

pub fn pinning_demo() -> String {
    let pinned = SelfRef::new("pinned");
    let ptr_ok = pinned.as_ref().check_ptr();

    let movable = Box::new("move ok".to_string());
    let movable_note = format!("Box<String> is Unpin so it can move: {}", movable);

    let lines = vec![
        format!("pinned self-ref valid: {ptr_ok}"),
        movable_note,
        String::from("Pin prevents moves that would break self-references"),
    ];

    lines.join("\n")
}


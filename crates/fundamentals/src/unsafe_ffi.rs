//! Unsafe pointer basics; avoid in most code.

pub fn unsafe_demo() -> String {
    let value = 7u32;
    let ptr = &value as *const u32;

    let read_back = unsafe { *ptr };

    let lines = vec![
        format!("value: {value}, read via raw pointer: {read_back}"),
        String::from("Use unsafe only with clear invariants; wrap in safe APIs."),
    ];

    lines.join("\n")
}


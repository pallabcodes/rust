//! Notes on common borrow checker pitfalls.

pub fn borrow_checker_notes() -> String {
    let notes = [
        "You cannot mutate while an immutable borrow is active.",
        "Use shorter scopes or create new bindings to release borrows.",
        "Move vs copy: types without Copy move by default.",
        "Use interior mutability (RefCell/Mutex) when shared mutation is required.",
        "Split borrows by operating on disjoint fields or slices.",
    ];

    notes.join("\n")
}


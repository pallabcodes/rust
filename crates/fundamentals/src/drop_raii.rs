//! Drop order and RAII.

#[derive(Debug)]
struct Trace(&'static str);

impl Drop for Trace {
    fn drop(&mut self) {
        println!("dropping {}", self.0);
    }
}

fn scoped() -> String {
    let a = Trace("a");
    let b = Trace("b");
    let _c = Trace("c");
    format!("{:?} {:?}", a, b)
}

pub fn drop_demo() -> String {
    let order_note = "Drops run in reverse declaration order within a scope";
    let panic_note = "RAII runs on panic; use Drop for cleanup like locks or files";
    let _ = scoped();
    [order_note, panic_note].join("\n")
}


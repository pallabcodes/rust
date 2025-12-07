//! Simple macro_rules example versus derive.

#[derive(Debug, Clone)]
struct Item {
    id: u32,
    name: String,
}

macro_rules! make_items {
    ($($id:expr => $name:expr),* $(,)?) => {{
        let mut v = Vec::new();
        $( v.push(Item { id: $id, name: $name.to_string() }); )*
        v
    }};
}

pub fn macros_demo() -> String {
    let items = make_items!(1 => "alpha", 2 => "beta");
    format!("macro built: {:?}", items)
}
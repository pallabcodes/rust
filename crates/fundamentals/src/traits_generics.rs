//! Demonstrates traits, generics, and trait bounds.

use std::fmt::Display;

trait Describable {
    fn describe(&self) -> String;
}

// T is a type parameter: caller picks the concrete type (i32, f64, etc).
// Copy/Clone derive only works if T itself is Copy.
#[derive(Copy, Clone)]
struct Point<T> {
    x: T,
    y: T,
}

// Trait impl with bounds ensures T can be printed and copied.
// Bounds live with the impl; they state the contract for this specialization.
impl<T: Display + Copy> Describable for Point<T> {
    fn describe(&self) -> String {
        format!("Point({}, {})", self.x, self.y)
    }
}

// Generic function: Ord for comparison, Copy to return by value.
// copied() turns &T into T by Copy.
fn max_in_slice<T: Ord + Copy>(items: &[T]) -> Option<T> {
    items.iter().copied().max()
}

// Trait object: dynamic dispatch at runtime via vtable.
fn describe_dyn(item: &dyn Describable) -> String {
    item.describe()
}

// Generic parameter: monomorphized to a concrete function per T (static dispatch).
fn describe_impl<T: Describable>(item: &T) -> String {
    item.describe()
}

pub fn traits_demo() -> String {
    let p = Point { x: 2, y: 5 };
    let desc = p.describe();
    let dyn_desc = describe_dyn(&p);
    let impl_desc = describe_impl(&p);

    let vals = [3, 1, 4, 1, 5];
    let max_val = max_in_slice(&vals);

    let lines = vec![
        desc,
        dyn_desc,
        impl_desc,
        format!("max in slice {:?} = {:?}", vals, max_val),
    ];

    lines.join("\n")
}


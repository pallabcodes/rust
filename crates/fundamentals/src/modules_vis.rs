//! Demonstrates modules and visibility.

mod domain {
    pub mod inner {
        pub fn exposed() -> String {
            format!("{} from inner", helper())
        }

        fn helper() -> &'static str {
            "hello"
        }
    }

    pub use inner::exposed;
}

pub fn modules_demo() -> String {
    let msg = domain::exposed();
    let lines = vec![
        msg,
        String::from("helper stays private inside inner"),
    ];

    lines.join("\n")
}


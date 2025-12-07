//! Shows tracing spans and events.

use tracing::{info, span, Level};

pub fn tracing_spans_demo() -> String {
    let span = span!(Level::INFO, "work_span", task = "demo");
    let _enter = span.enter();
    info!("inside span doing work");
    drop(_enter);
    "span emitted; configure RUST_LOG=info to see it".to_string()
}


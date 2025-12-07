# Rust Monorepo Starter

This repo is a teaching oriented Rust monorepo for learning fundamentals and keeping scale concerns in mind.

## Layout
* Cargo workspace manifest in `Cargo.toml` with shared dependencies and policy
* `apps/cli`: minimal CLI that exercises shared logic
* `crates/corelib`: reusable domain and utility code
* `crates/fundamentals`: hands-on lessons for Rust basics
* `services/gateway`: HTTP boundary that reuses the shared library

## Standards
* Max 150 lines per file
* Edition 2021 and rust-version 1.80 across all crates
* Tracing is wired with env based filters for debuggability

## Learning path
1. Run lesson demos in the CLI: `cargo run -p cli -- learn variables`
   * Options: variables, ownership, borrowing, patterns, collections, collections-ownership, errors, lifetimes, traits, iterators, concurrency, modules, testing, smart-pointers, async-tasks, error-composition, macros, io, serde, tracing-spans, combinators, borrow-checker, file-io, strings, enums, result-flow, async-primer, tooling, cargo, send-sync, unsafe, pinning, drop-raii, concurrency-primitives, performance, api-design, closures, threads
2. Explore shared utilities in `crates/corelib`
3. See HTTP wiring in `services/gateway`

## Getting started
1. Install Rust via rustup and ensure `cargo` is on PATH
2. Build everything: `cargo build --workspace`
3. Run tests: `cargo test --workspace`
4. Run CLI: `cargo run -p cli -- --help`
5. Run gateway service: `cargo run -p gateway`

## Extending
* Add new libraries under `crates/` and expose narrow APIs
* Add services or binaries under `services/` or `apps/`
* Keep binaries thin and push business logic into libraries for reuse


use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use corelib::{greeting, math};
use fundamentals::*;
use tracing::info;

#[derive(Parser)]
#[command(name = "cli", version, about = "Rust monorepo CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Greet { #[arg(short, long, default_value = "world")] name: String },
    Sum { #[arg()] a: i64, #[arg()] b: i64 },
    Learn { #[arg(value_enum)] lesson: Lesson },
}

#[derive(Clone, ValueEnum)]
enum Lesson {
    Variables,
    Ownership,
    Borrowing,
    Patterns,
    Collections,
    Errors,
    Lifetimes,
    Traits,
    Iterators,
    Concurrency,
    Modules,
    Testing,
    SmartPointers,
    AsyncTasks,
    ErrorComposition,
    Macros,
    Io,
    Serde,
    TracingSpans,
    Combinators,
    BorrowChecker,
    FileIo,
    Strings,
    Enums,
    ResultFlow,
    AsyncPrimer,
    Tooling,
    Cargo,
    SendSync,
    CollectionsOwnership,
    Unsafe,
    Pinning,
    DropRaii,
    ConcurrencyPrimitives,
    Performance,
    ApiDesign,
    Closures,
    Threads,
}

fn main() -> Result<()> {
    init_tracing();

    let cli = Cli::parse();
    match cli.command {
        Command::Greet { name } => {
            let text = greeting::greet(&name);
            info!(%name, "generated greeting");
            println!("{}", text);
        }
        Command::Sum { a, b } => {
            match math::checked_add(a, b) {
                Ok(total) => {
                    info!(a, b, total, "sum succeeded");
                    println!("{} + {} = {}", a, b, total);
                }
                Err(err) => {
                    info!(a, b, "sum failed");
                    println!("could not add: {err:?}");
                }
            }
        }
        Command::Learn { lesson } => {
            let output = run_lesson(lesson);
            println!("{output}");
        }
    }

    Ok(())
}

fn run_lesson(lesson: Lesson) -> String {
    match lesson {
        Lesson::Variables => variables_demo(),
        Lesson::Ownership => ownership_demo(),
        Lesson::Borrowing => borrowing_demo(),
        Lesson::Patterns => patterns_demo(),
        Lesson::Collections => collections_demo(),
        Lesson::Errors => errors_demo(),
        Lesson::Lifetimes => lifetimes_demo(),
        Lesson::Traits => traits_demo(),
        Lesson::Iterators => iterators_demo(),
        Lesson::Concurrency => concurrency_demo(),
        Lesson::Modules => modules_demo(),
        Lesson::Testing => testing_demo(),
        Lesson::SmartPointers => smart_pointers_demo(),
        Lesson::AsyncTasks => async_tasks_demo(),
        Lesson::ErrorComposition => error_composition_demo(),
        Lesson::Macros => macros_demo(),
        Lesson::Io => io_demo(),
        Lesson::Serde => serde_demo(),
        Lesson::TracingSpans => tracing_spans_demo(),
        Lesson::Combinators => combinators_demo(),
        Lesson::BorrowChecker => borrow_checker_notes(),
        Lesson::FileIo => file_io_demo(),
        Lesson::Strings => strings_demo(),
        Lesson::Enums => enums_demo(),
        Lesson::ResultFlow => result_flow_demo(),
        Lesson::AsyncPrimer => async_primer_demo(),
        Lesson::Tooling => tooling_demo(),
        Lesson::Cargo => cargo_demo(),
        Lesson::SendSync => send_sync_demo(),
        Lesson::CollectionsOwnership => collections_ownership_demo(),
        Lesson::Unsafe => unsafe_demo(),
        Lesson::Pinning => pinning_demo(),
        Lesson::DropRaii => drop_demo(),
        Lesson::ConcurrencyPrimitives => concurrency_primitives_demo(),
        Lesson::Performance => performance_demo(),
        Lesson::ApiDesign => api_design_demo(),
        Lesson::Closures => closures_demo(),
        Lesson::Threads => threads_demo(),
    }
}

fn init_tracing() {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}


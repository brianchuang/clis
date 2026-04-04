pub mod shell;

/// Shared error type for CLI tools. Boxed dynamic error for ergonomic `?` usage.
pub type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Standard CLI entry point: run the given closure and print errors to stderr.
/// Exits with code 1 on error.
pub fn run_main(f: impl FnOnce() -> Result) {
    if let Err(e) = f() {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

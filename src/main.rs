//! Command-line entry point of Aporia.
//!
//! Exit status: 0 on success, 1 for domain or I/O errors (one line on
//! stderr), 2 for usage errors reported by clap.

mod cli;

use std::process::ExitCode;

use clap::Parser;

fn main() -> ExitCode {
    let args = cli::Cli::parse();
    match cli::execute(args, &mut std::io::stdout().lock()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

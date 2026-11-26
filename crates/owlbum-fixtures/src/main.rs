//! `cargo run -p owlbum-fixtures -- trip <folder>`,
//! `cargo run -p owlbum-fixtures -- large <count> <folder> [--seed <n>]` or
//! `cargo run -p owlbum-fixtures -- extras <folder>`.
//! See docs/08-desktop-and-release.md, "Fixtures".

use std::process::ExitCode;

use owlbum_fixtures::cli;

fn main() -> ExitCode {
    let command = match cli::parse(std::env::args().skip(1)) {
        Ok(c) => c,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    match cli::run(command) {
        Ok(line) => {
            println!("{line}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("owlbum-fixtures: {e}");
            ExitCode::FAILURE
        }
    }
}

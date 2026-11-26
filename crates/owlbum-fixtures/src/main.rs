//! `cargo run -p owlbum-fixtures -- trip <folder>` or
//! `cargo run -p owlbum-fixtures -- large <count> <folder>`.

use std::process::ExitCode;

fn main() -> ExitCode {
    eprintln!(
        "usage: owlbum-fixtures trip <folder>\n       owlbum-fixtures large <count> <folder>\n\nThe generator is not built yet (roadmap issue 2)."
    );
    ExitCode::from(2)
}

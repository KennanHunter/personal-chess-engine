//! Convert a PGN file into the comma-separated UCI move histories the bot
//! consumes (one game per line).
//!
//! Usage:
//!     cargo run --bin convert_pgn -- games.pgn > games.txt
//!     cat games.pgn | cargo run --bin convert_pgn > games.txt   # read stdin

// Native-only tooling: the `pgn` module (and its `pgn-reader` dep) is excluded
// from WASM builds, so this binary is a no-op when targeting wasm32.
#[cfg(target_arch = "wasm32")]
fn main() {
    compile_error!("This script is not setup for wasm")
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> std::process::ExitCode {
    native::run()
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::io::{self, Read, Write};
    use std::process::ExitCode;

    use engine::pgn::pgn_to_uci_lines;

    pub fn run() -> ExitCode {
        let pgn = match read_input() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error reading input: {e}");
                return ExitCode::FAILURE;
            }
        };

        let lines = match pgn_to_uci_lines(&pgn) {
            Ok(lines) => lines,
            Err(e) => {
                eprintln!("error parsing PGN: {e}");
                return ExitCode::FAILURE;
            }
        };

        let stdout = io::stdout();
        let mut out = stdout.lock();
        for line in &lines {
            if writeln!(out, "{line}").is_err() {
                return ExitCode::FAILURE;
            }
        }

        eprintln!("converted {} game(s)", lines.len());
        ExitCode::SUCCESS
    }

    /// Read PGN from the file named in the first CLI argument, or stdin if none.
    fn read_input() -> io::Result<String> {
        match std::env::args().nth(1) {
            Some(path) => std::fs::read_to_string(path),
            None => {
                let mut buf = String::new();
                io::stdin().read_to_string(&mut buf)?;
                Ok(buf)
            }
        }
    }
}

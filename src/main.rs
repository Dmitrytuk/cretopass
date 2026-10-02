use std::process::ExitCode;

const USAGE: &str = "\
cretopass - terminal-based secret keeper

Usage: cretopass

Options:
  -h, --help       Show this help message
  -V, --version    Show version information";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [] => {}
        [a] if a == "-h" || a == "--help" => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        [a] if a == "-V" || a == "--version" => {
            println!("cretopass {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        [other, ..] => {
            eprintln!("cretopass: unexpected argument '{other}'\n\n{USAGE}");
            return ExitCode::FAILURE;
        }
    }
    match cretopass::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e}");
            ExitCode::FAILURE
        }
    }
}

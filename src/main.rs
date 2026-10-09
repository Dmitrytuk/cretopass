use std::process::ExitCode;

const USAGE: &str = "\
cretopass - terminal-based secret keeper

Usage: cretopass [OPTIONS] [COMMAND]

Options:
  -h, --help       Show this help message
  -V, --version    Show version information

Commands:
  remove           Delete the app and all artifacts from the system
  update           Check for a new version and install it";

fn main() -> ExitCode {
    let mut parser = lexopt::Parser::from_env();
    let mut subcommand: Option<String> = None;

    loop {
        let arg = match parser.next() {
            Ok(Some(arg)) => arg,
            Ok(None) => break,
            Err(err) => {
                eprintln!("cretopass: {err}\n\n{USAGE}");
                return ExitCode::FAILURE;
            }
        };
        match arg {
            lexopt::Arg::Short('h') | lexopt::Arg::Long("help") => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            lexopt::Arg::Short('V') | lexopt::Arg::Long("version") => {
                println!("cretopass {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            lexopt::Arg::Value(cmd) => match cmd.to_str() {
                Some("remove") | Some("update") => {
                    if subcommand.is_some() {
                        eprintln!("cretopass: multiple commands are not allowed\n\n{USAGE}");
                        return ExitCode::FAILURE;
                    }
                    subcommand = Some(cmd.to_string_lossy().into_owned());
                }
                Some(other) => {
                    eprintln!("cretopass: unexpected argument '{other}'\n\n{USAGE}");
                    return ExitCode::FAILURE;
                }
                None => {
                    eprintln!("cretopass: unexpected argument (invalid UTF-8)\n\n{USAGE}");
                    return ExitCode::FAILURE;
                }
            },
            _ => {
                eprintln!("cretopass: unexpected argument\n\n{USAGE}");
                return ExitCode::FAILURE;
            }
        }
    }

    match subcommand.as_deref() {
        Some("remove") => {
            println!("cretopass: remove is not yet implemented.");
            ExitCode::SUCCESS
        }
        Some("update") => {
            println!("cretopass: update is not yet implemented.");
            ExitCode::SUCCESS
        }
        None => match cretopass::run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("Error: {e}");
                ExitCode::FAILURE
            }
        },
        _ => unreachable!(),
    }
}
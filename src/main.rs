#![deny(unsafe_code)]

use std::process::ExitCode;

use cretopass::cli::LifecycleCommand;
use cretopass::theming::texts;

const USAGE: &str = "\
cretopass - terminal-based secret keeper

Usage: cretopass [OPTIONS] [COMMAND]

Options:
  -h, --help       Show this help message
  -V, --version    Show version information

Commands:
  remove           Delete the app and all artifacts from the system
                   (self-managed installs only)";

/// Feedback block appended to `--help`.
///
/// Built from the shared app-identity constants in [`texts`] so the CLI help
/// and the TUI About overlay never drift apart (see
/// [ADR 0020](docs/adr/0020-feedback-and-issue-reporting.md)).
fn help_footer() -> String {
    let mut out = String::new();
    out.push_str("\nAbout:\n");
    out.push_str(&format!("  {} {}\n", texts::APP_NAME, texts::APP_VERSION));
    out.push_str(&format!("  {}\n", texts::APP_TAGLINE));
    out.push_str("\nFeedback:\n");
    out.push_str(&format!("  {}\n", texts::ABOUT_LABEL_BUGS));
    out.push_str(&format!("    {}\n", texts::ISSUES_URL));
    out.push_str(&format!("  {}\n", texts::ABOUT_LABEL_SECURITY));
    out.push_str(&format!("    {}\n", texts::SECURITY_ADVISORY_URL));
    out.push_str(&format!(
        "\n{}{}\n  {}\n",
        texts::ABOUT_LICENSE_PREFIX,
        texts::LICENSE_NAME,
        texts::LICENSE_URL
    ));
    out
}

fn main() -> ExitCode {
    let mut parser = lexopt::Parser::from_env();
    let mut subcommand: Option<LifecycleCommand> = None;

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
                print!("{}", help_footer());
                return ExitCode::SUCCESS;
            }
            lexopt::Arg::Short('V') | lexopt::Arg::Long("version") => {
                println!("{} {}", texts::APP_NAME, texts::APP_VERSION);
                return ExitCode::SUCCESS;
            }
            lexopt::Arg::Value(cmd) => {
                let parsed = cmd.to_str().and_then(LifecycleCommand::parse);
                match parsed {
                    Some(command) => {
                        if subcommand.is_some() {
                            eprintln!("cretopass: multiple commands are not allowed\n\n{USAGE}");
                            return ExitCode::FAILURE;
                        }
                        subcommand = Some(command);
                    }
                    None => {
                        let shown = cmd.to_string_lossy();
                        eprintln!("cretopass: unexpected argument '{shown}'\n\n{USAGE}");
                        return ExitCode::FAILURE;
                    }
                }
            }
            _ => {
                eprintln!("cretopass: unexpected argument\n\n{USAGE}");
                return ExitCode::FAILURE;
            }
        }
    }

    match subcommand {
        Some(command) => command.run(),
        None => match cretopass::run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("Error: {e}");
                ExitCode::FAILURE
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `--help` footer surfaces the app version and every feedback link, so
    /// a user reading plain help can find the same channels as the TUI overlay.
    #[test]
    fn help_footer_lists_version_and_feedback_links() {
        let footer = help_footer();
        assert!(footer.contains(texts::APP_VERSION), "{footer}");
        assert!(footer.contains(texts::ISSUES_URL), "{footer}");
        assert!(footer.contains(texts::SECURITY_ADVISORY_URL), "{footer}");
        assert!(footer.contains(texts::LICENSE_URL), "{footer}");
    }

    /// `--version` must stay a single bare `name version` line: the installer
    /// scripts parse it with `head -n1` and a semver search.
    #[test]
    fn version_line_is_a_bare_name_and_version() {
        let line = format!("{} {}", texts::APP_NAME, texts::APP_VERSION);
        assert_eq!(line, format!("cretopass {}", env!("CARGO_PKG_VERSION")));
        assert!(!line.contains('\n'));
    }
}

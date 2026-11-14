//! Command-line entry point and lifecycle commands.
//!
//! [`run`] is the binary's only entry point. It parses `argv` and either
//! starts the TUI ([`crate::app::run`]) or dispatches a lifecycle subcommand
//! that operates on the installation itself rather than on secret data — only
//! `remove` remains. Removal only works for GitHub Releases installations via
//! `install.sh` / `install.ps1`; pacman/AUR and `cargo install` are detected
//! and refused.
//!
//! The sub-modules are private; [`LifecycleCommand`] is the only surface they
//! expose (see ADR 0009), so the parser here never touches platform-specific
//! code. The module makes no network requests: `update` was removed so the
//! binary never downloads or executes remote code, and updating happens out of
//! process (see ADR 0027).

mod detect;
mod layout;
mod remove;

use std::process::ExitCode;

use crate::theming::texts;

/// Exit code used when `remove` refuses a foreign installation.
pub const EXIT_FOREIGN_INSTALL: u8 = 2;

const USAGE: &str = "\
cretopass - terminal-based secret keeper

Usage: cretopass [OPTIONS] [COMMAND]

Options:
  -h, --help       Show this help message
  -V, --version    Show version information

Commands:
  remove           Delete the app and all artifacts from the system
                   (self-managed installs only)";

/// Parses the command line and runs whatever it asks for.
///
/// With no subcommand the TUI starts through [`crate::app::run`]; a lifecycle
/// subcommand instead operates on the installation itself.
#[must_use]
pub fn run() -> ExitCode {
    match parse_args() {
        Ok(Some(command)) => command.run(),
        Ok(None) => match crate::app::run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("Error: {e}");
                ExitCode::FAILURE
            }
        },
        Err(exit) => exit,
    }
}

/// Parses `argv` into at most one [`LifecycleCommand`].
///
/// Returns `Ok(None)` when the TUI should start, and `Err(exit_code)` when the
/// argument list was rejected or `--help`/`--version` already printed their
/// output. At most one subcommand is accepted.
fn parse_args() -> Result<Option<LifecycleCommand>, ExitCode> {
    let mut parser = lexopt::Parser::from_env();
    let mut subcommand: Option<LifecycleCommand> = None;

    loop {
        let arg = match parser.next() {
            Ok(Some(arg)) => arg,
            Ok(None) => break,
            Err(err) => {
                eprintln!("cretopass: {err}\n\n{USAGE}");
                return Err(ExitCode::FAILURE);
            }
        };
        match arg {
            lexopt::Arg::Short('h') | lexopt::Arg::Long("help") => {
                println!("{USAGE}");
                print!("{}", help_footer());
                return Err(ExitCode::SUCCESS);
            }
            lexopt::Arg::Short('V') | lexopt::Arg::Long("version") => {
                println!("{} {}", texts::APP_NAME, texts::APP_VERSION);
                return Err(ExitCode::SUCCESS);
            }
            lexopt::Arg::Value(cmd) => match cmd.to_str().and_then(LifecycleCommand::parse) {
                Some(command) => {
                    if subcommand.is_some() {
                        eprintln!("cretopass: multiple commands are not allowed\n\n{USAGE}");
                        return Err(ExitCode::FAILURE);
                    }
                    subcommand = Some(command);
                }
                None => {
                    let shown = cmd.to_string_lossy();
                    eprintln!("cretopass: unexpected argument '{shown}'\n\n{USAGE}");
                    return Err(ExitCode::FAILURE);
                }
            },
            _ => {
                eprintln!("cretopass: unexpected argument\n\n{USAGE}");
                return Err(ExitCode::FAILURE);
            }
        }
    }

    Ok(subcommand)
}

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

/// A lifecycle subcommand that mutates the installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleCommand {
    /// Remove every installed artifact (`cretopass remove`).
    Remove,
}

impl LifecycleCommand {
    /// Parses a raw CLI argument into a lifecycle subcommand.
    ///
    /// Returns `None` for anything that is not `remove`.
    #[must_use]
    pub fn parse(arg: &str) -> Option<Self> {
        match arg {
            "remove" => Some(Self::Remove),
            _ => None,
        }
    }

    /// Runs the subcommand and returns its [`ExitCode`].
    #[must_use]
    pub fn run(self) -> ExitCode {
        match self {
            Self::Remove => remove::run_remove(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The known subcommand parses to its variant.
    #[test]
    fn parses_known_subcommands() {
        assert_eq!(
            LifecycleCommand::parse("remove"),
            Some(LifecycleCommand::Remove)
        );
    }

    /// Unknown arguments are rejected, including the retired `update`.
    #[test]
    fn rejects_unknown_subcommands() {
        assert_eq!(LifecycleCommand::parse("update"), None);
        assert_eq!(LifecycleCommand::parse("install"), None);
        assert_eq!(LifecycleCommand::parse(""), None);
    }

    /// The foreign-install exit code is the documented value.
    #[test]
    fn foreign_install_exit_code_is_two() {
        assert_eq!(EXIT_FOREIGN_INSTALL, 2);
    }

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

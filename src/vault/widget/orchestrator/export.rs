//! Writing the open tree to the file the user named, as plain text.

use std::io;

use ratatui::Terminal;
use ratatui::backend::Backend;

use crate::file_handling;
use crate::file_handling::ExportFormat;
use crate::theming::Status;
use crate::theming::texts;

use super::super::{HomeState, Modal, run_with_spinner};
use super::errors;

/// Writes the open tree to `path` as `format`, reporting the result in an alert.
///
/// The write runs off the UI thread so the title bar keeps animating its
/// `Saving` status while the tree is serialised and the file is fsynced.
pub(super) fn export<B: Backend>(
    state: &mut HomeState,
    terminal: &mut Terminal<B>,
    path: String,
    format: ExportFormat,
) -> io::Result<()> {
    let Some(node) = state
        .active_vault
        .as_ref()
        .and_then(|active| active.node.clone())
    else {
        // Nothing to write: the destination was named, so say why it went
        // nowhere instead of dropping the user back on the menu silently.
        state.modal = Some(Modal::error(texts::MSG_NO_OPEN_FILE.to_string()));
        return Ok(());
    };

    let result = run_with_spinner(terminal, state, Status::Saving, move || {
        file_handling::export_tree(&node, &path, format)
    })?;

    state.modal = Some(match result {
        Ok(out_path) => Modal::success(format!("{}{}", texts::MSG_EXPORT_SUCCESS, out_path)),
        Err(e) => Modal::error(errors::message(&e)),
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::node::Node;
    use crate::vault::active_file::ActiveVaultState;
    use crate::vault::utils::Configuration;
    use ratatui::backend::TestBackend;

    fn state_with_root() -> HomeState {
        let mut state = HomeState::with_config(Configuration::default());
        state.active_vault = Some(ActiveVaultState {
            path: None,
            passphrase: zeroize::Zeroizing::new(b"pw".to_vec()),
            node: Some(Node::new("Root")),
        });
        state
    }

    #[test]
    fn a_failed_export_is_reported_and_clears_the_status() {
        let mut state = state_with_root();
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();

        // A directory that does not exist: the atomic write fails.
        export(
            &mut state,
            &mut terminal,
            "/nonexistent_dir_xyz123/vault.age".to_string(),
            ExportFormat::Json,
        )
        .unwrap();

        match &state.modal {
            Some(Modal::Alert(alert)) => assert!(
                alert.message.starts_with(texts::MSG_FILE_ERROR),
                "the write failure must be reported: {}",
                alert.message
            ),
            _ => panic!("a failed export must be reported in an alert"),
        }
        assert!(
            state.status.is_none(),
            "the spinner is cleared once the write finishes"
        );
    }

    #[test]
    fn an_export_with_nothing_open_says_so_without_a_spinner() {
        let mut state = HomeState::with_config(Configuration::default());
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();

        export(
            &mut state,
            &mut terminal,
            "/tmp/whatever.age".to_string(),
            ExportFormat::Json,
        )
        .unwrap();

        match &state.modal {
            Some(Modal::Alert(alert)) => assert_eq!(alert.message, texts::MSG_NO_OPEN_FILE),
            _ => panic!("an export with no open file must be reported"),
        }
        assert!(
            state.status.is_none(),
            "the no-open-file guard must not leave a spinner"
        );
    }
}

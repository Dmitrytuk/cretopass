//! The one place that turns a modal's [`TreeAction`] into real work.
//!
//! Everything the Tree screen asks for lands here: disk I/O, the system
//! clipboard, screen transitions. The modals read and mutate the tree in memory
//! (see [`super::modal`]); this module performs what they cannot — anything
//! outside the process or the modal.

use std::io::{self, Write};
use std::path::Path;

use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::Backend;

use super::busy::run_with_busy;
use super::modal::TreeAction;
use super::{ExportDestination, Modal, ShareDestination, SharePassphrase, TreeState};
use crate::file_handling::share_tree;
use crate::file_handling::{ExportFormat, export_tree};
use crate::share::SharePassword;
use crate::theming::{Status, texts};
use crate::tree::node::Node;

/// Performs `action`. The modal that produced it has already been taken out of
/// [`TreeState`], so dismissing it is the act of not putting it back.
///
/// `terminal` and `draw` are only needed by actions that block (the share
/// write): they let [`run_with_busy`] keep the title-bar status animating while
/// the work runs off-thread. `draw` is the screen's full-frame renderer.
pub(crate) fn execute<B: Backend>(
    state: &mut TreeState,
    terminal: &mut Terminal<B>,
    action: TreeAction,
    draw: fn(&mut Frame, &mut TreeState),
) -> io::Result<()> {
    match action {
        TreeAction::Close => Ok(()),
        TreeAction::CopyToClipboard { text } => {
            copy_to_clipboard(&text);
            Ok(())
        }
        TreeAction::BeginExportDestination {
            node,
            include_children,
            format,
        } => {
            open_export_destination(state, node, include_children, format);
            Ok(())
        }
        TreeAction::Export {
            node,
            include_children,
            format,
            path,
        } => export(state, terminal, draw, node, include_children, format, path),
        TreeAction::BeginShareDestination {
            node,
            include_children,
        } => {
            open_share_destination(state, node, include_children);
            Ok(())
        }
        TreeAction::BeginSharePassphrase {
            node,
            include_children,
            dest,
        } => {
            state.modal = Some(Modal::SharePassphrase(Box::new(SharePassphrase::new(
                node,
                include_children,
                dest,
            ))));
            Ok(())
        }
        TreeAction::Share {
            node,
            include_children,
            dest,
            passphrase,
        } => share(
            state,
            terminal,
            draw,
            node,
            include_children,
            &dest,
            passphrase,
        ),
    }
}

/// Opens the File Browser to name where `node` is exported to.
///
/// The open secret file seeds the suggested name and the starting directory; the
/// browser falls back to its session memory when there is none.
fn open_export_destination(
    state: &mut TreeState,
    node: Node,
    include_children: bool,
    format: ExportFormat,
) {
    let destination = {
        let base_path = state.active_vault_path.as_deref().map(Path::new);
        ExportDestination::new(
            node,
            include_children,
            format,
            base_path,
            &state.browser_session,
        )
    };

    state.modal = Some(Modal::ExportDestination(Box::new(destination)));
}

/// Opens the File Browser to name the folder a share bundle of `node` is written
/// into.
///
/// The open secret file seeds the suggested folder name and the starting
/// directory; the browser falls back to its session memory when there is none.
fn open_share_destination(state: &mut TreeState, node: Node, include_children: bool) {
    let destination = {
        let base_path = state.active_vault_path.as_deref().map(Path::new);
        ShareDestination::new(node, include_children, base_path, &state.browser_session)
    };

    state.modal = Some(Modal::ShareDestination(Box::new(destination)));
}

/// `node` as the scope selects it: unchanged when `include_children`, otherwise
/// with its children dropped so a single node is written as a leaf.
///
/// Shared by the export and share flows, so "this node only" means the same
/// thing in both.
fn node_for_scope(node: Node, include_children: bool) -> Node {
    if include_children {
        return node;
    }
    let mut node = node;
    node.children = Vec::new();
    node
}

/// Writes `node` to `path` as `format` and reports the outcome as an alert.
///
/// The write runs off the UI thread so the title bar keeps animating its
/// `Saving` status while the tree is serialised and the file is fsynced —
/// the same path the neighbouring share flow takes.
fn export<B: Backend>(
    state: &mut TreeState,
    terminal: &mut Terminal<B>,
    draw: fn(&mut Frame, &mut TreeState),
    node: Node,
    include_children: bool,
    format: ExportFormat,
    path: String,
) -> io::Result<()> {
    let export_node = node_for_scope(node, include_children);

    let result = run_with_busy(terminal, state, Status::Saving, draw, move || {
        export_tree(&export_node, &path, format)
    })?;

    state.modal = Some(match result {
        Ok(out_path) => Modal::success(format!("{}{}", texts::MSG_EXPORT_NODE_SUCCESS, out_path)),
        Err(error) => Modal::error(format!("{error}")),
    });
    Ok(())
}

/// Writes a share bundle of `node`, with or without its children, into `path`
/// and reports the outcome as an alert.
///
/// The bundle is named after the Node. The write runs off the UI thread so the
/// title bar keeps animating its `Saving` status while the bundle is re-encrypted
/// and fsynced.
fn share<B: Backend>(
    state: &mut TreeState,
    terminal: &mut Terminal<B>,
    draw: fn(&mut Frame, &mut TreeState),
    node: Node,
    include_children: bool,
    path: &str,
    passphrase: SharePassword,
) -> io::Result<()> {
    // Mirror the export wizard: a node shared without its children drops them
    // before it is written.
    let bundle_root = node_for_scope(node, include_children);
    let node_name = Some(bundle_root.name.clone());
    let base_path = state.active_vault_path.clone();
    let path = path.to_string();

    // The password is moved into the worker as a `SharePassword`, so the bytes
    // are zeroized when the closure drops rather than lingering in a `String`.
    let result = run_with_busy(terminal, state, Status::Saving, draw, move || {
        share_tree(
            &bundle_root,
            Path::new(&path),
            base_path.as_deref().map(Path::new),
            node_name.as_deref(),
            passphrase.as_bytes(),
        )
    })?;

    state.modal = Some(match result {
        Ok(bundle) => Modal::success(format!("{}{}", texts::MSG_SHARE_SUCCESS, bundle.display())),
        Err(error) => Modal::error(format!("{error}")),
    });
    Ok(())
}

/// Copies `text` to the system clipboard, trying the platform's helper in turn.
///
/// A missing helper (headless, or a platform without one) is silently ignored:
/// the copy is a best-effort convenience, not a failure the user must handle.
fn copy_to_clipboard(text: &str) {
    let cmds: &[&[&str]] = if cfg!(target_os = "macos") {
        &[&["pbcopy"]]
    } else if cfg!(target_os = "linux") {
        &[
            &["wl-copy"],
            &["xclip", "-selection", "clipboard"],
            &["xsel", "-b"],
        ]
    } else {
        return;
    };
    for args in cmds {
        if let Ok(mut child) = std::process::Command::new(args[0])
            .args(&args[1..])
            .stdin(std::process::Stdio::piped())
            .spawn()
        {
            // Take the pipe out of the child and let it drop at the end of the
            // block: the helper (xclip/xsel) reads stdin to EOF before it takes
            // the selection, so the write end must be closed before `wait`.
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    /// A renderer that does nothing: the export test only cares that the write
    /// runs and its outcome is reported.
    fn noop_draw(_frame: &mut Frame, _state: &mut TreeState) {}

    #[test]
    fn a_failed_export_is_reported_and_clears_the_status() {
        let mut state = TreeState::new(Node::new("Root"));
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();

        // A directory that does not exist: the atomic write fails.
        export(
            &mut state,
            &mut terminal,
            noop_draw,
            Node::new("Root"),
            true,
            ExportFormat::Json,
            "/nonexistent_dir_xyz123/vault.age".to_string(),
        )
        .unwrap();

        match &state.modal {
            Some(Modal::Alert(alert)) => assert!(
                alert.message.contains("io:"),
                "the write failure must be reported: {}",
                alert.message
            ),
            _ => panic!("a failed export must be reported in an alert"),
        }
        assert!(
            state.operation_status.is_none(),
            "the busy status is cleared once the write finishes"
        );
    }
}

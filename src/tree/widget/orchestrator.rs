//! The one place that turns a modal's [`TreeAction`] into real work.
//!
//! Everything the Tree screen asks for lands here: disk I/O, the system
//! clipboard, screen transitions. The modals read and mutate the tree in memory
//! (see [`super::modal`]); this module performs what they cannot — anything
//! outside the process or the modal.

use std::io::{self, Write};
use std::path::Path;

use super::modal::TreeAction;
use super::{ExportDestination, Modal, TreeState};
use crate::file_handling::export::{ExportFormat, export_tree};
use crate::theming::texts;
use crate::tree::node::Node;

/// Performs `action`. The modal that produced it has already been taken out of
/// [`TreeState`], so dismissing it is the act of not putting it back.
pub(crate) fn execute(state: &mut TreeState, action: TreeAction) -> io::Result<()> {
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
        } => {
            export(state, node, include_children, format, &path);
            Ok(())
        }
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

/// Writes `node` to `path` as `format` and reports the outcome as an alert.
fn export(
    state: &mut TreeState,
    node: Node,
    include_children: bool,
    format: ExportFormat,
    path: &str,
) {
    let export_node = if include_children {
        node
    } else {
        let mut node = node;
        node.children = Vec::new();
        node
    };

    state.modal = Some(match export_tree(&export_node, path, format) {
        Ok(out_path) => Modal::alert(format!("{}{}", texts::MSG_EXPORT_NODE_SUCCESS, out_path)),
        Err(error) => Modal::alert(format!("{error}")),
    });
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

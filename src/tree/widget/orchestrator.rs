//! The one place that turns a modal's [`TreeAction`] into real work.
//!
//! Everything the Tree screen asks for lands here: disk I/O, the system
//! clipboard, screen transitions. The modals read and mutate the tree in memory
//! (see [`super::modal`]); this module performs what they cannot — anything
//! outside the process or the modal.

use std::io::{self, Write};
use std::path::Path;

use super::modal::TreeAction;
use super::{ExportDestination, Modal, ShareDestination, SharePassphrase, TreeState};
use crate::file_handling::export::{ExportFormat, export_tree};
use crate::file_handling::share::share_tree;
use crate::share::ShareScope;
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
        TreeAction::BeginShareDestination { node, scope } => {
            open_share_destination(state, node, scope);
            Ok(())
        }
        TreeAction::BeginSharePassphrase { node, scope, dest } => {
            state.modal = Some(Modal::SharePassphrase(Box::new(SharePassphrase::new(
                node, scope, dest,
            ))));
            Ok(())
        }
        TreeAction::Share {
            node,
            scope,
            dest,
            passphrase,
        } => {
            share(state, node, scope, &dest, &passphrase);
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

/// Opens the File Browser to name the folder a share bundle of `node` is written
/// into.
///
/// The open secret file seeds the suggested folder name and the starting
/// directory; the browser falls back to its session memory when there is none.
fn open_share_destination(state: &mut TreeState, node: Node, scope: ShareScope) {
    let destination = {
        let base_path = state.active_vault_path.as_deref().map(Path::new);
        ShareDestination::new(node, scope, base_path, &state.browser_session)
    };

    state.modal = Some(Modal::ShareDestination(Box::new(destination)));
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
        Ok(out_path) => Modal::success(format!("{}{}", texts::MSG_EXPORT_NODE_SUCCESS, out_path)),
        Err(error) => Modal::error(format!("{error}")),
    });
}

/// Writes a share bundle of `node` into `path` and reports the outcome as an
/// alert.
///
/// `WholeFile` shares the open Root rather than the Node selected when the wizard
/// opened, and names the bundle after the file's stem alone.
fn share(state: &mut TreeState, node: Node, scope: ShareScope, path: &str, passphrase: &str) {
    let (bundle_root, node_name) = match scope {
        ShareScope::Branch => (node.clone(), Some(node.name.clone())),
        ShareScope::WholeFile => (state.root.clone(), None),
    };
    let base_path = state.active_vault_path.as_deref().map(Path::new);

    let result = share_tree(
        &bundle_root,
        Path::new(path),
        base_path,
        node_name.as_deref(),
        passphrase,
    );

    state.modal = Some(match result {
        Ok(bundle) => Modal::success(format!("{}{}", texts::MSG_SHARE_SUCCESS, bundle.display())),
        Err(error) => Modal::error(format!("{error}")),
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

//! The one place that turns a modal's [`TreeAction`] into real work.
//!
//! Everything the Tree screen asks for lands here: disk I/O, the system
//! clipboard, screen transitions. The modals read and mutate the tree in memory
//! (see [`super::modal`]); this module performs what they cannot — anything
//! outside the process or the modal.

use std::io::{self, Write};

use super::modal::TreeAction;
use super::{Modal, TreeState};
use crate::file_handling::export::{ExportFormat, export_tree};
use crate::theming::texts;
use crate::tree::node::Node;

/// Performs `action`. The modal that produced it has already been taken out of
/// [`TreeState`], so dismissing it is the act of not putting it back.
pub fn execute(state: &mut TreeState, action: TreeAction) -> io::Result<()> {
    match action {
        TreeAction::Close => Ok(()),
        TreeAction::CopyToClipboard { text } => {
            copy_to_clipboard(&text);
            Ok(())
        }
        TreeAction::Export {
            node,
            include_children,
            format,
        } => {
            export(state, node, include_children, format);
            Ok(())
        }
    }
}

/// Writes `node` beside the open secret file and reports the outcome as an alert.
///
/// Without an open vault there is nowhere to write, so the user gets the
/// "no active file" alert instead.
fn export(state: &mut TreeState, node: Node, include_children: bool, format: ExportFormat) {
    let node_name = node.name.clone();
    let export_node = if include_children {
        node
    } else {
        let mut node = node;
        node.children = Vec::new();
        node
    };

    let Some(base_path) = state.active_vault_path.clone() else {
        state.modal = Some(Modal::alert(texts::MSG_NO_ACTIVE_FILE.to_string()));
        return;
    };

    let path = std::path::Path::new(&base_path);
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("export");
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("age");
    let node_name_clean: String = node_name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let export_path = parent.join(format!("{}_{}.{}", stem, node_name_clean, ext));
    let export_path_str = export_path.to_string_lossy().to_string();

    state.modal = Some(match export_tree(&export_node, &export_path_str, format) {
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
            if let Some(ref mut stdin) = child.stdin {
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
            break;
        }
    }
}

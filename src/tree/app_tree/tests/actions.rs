//! Modal-action and export/share routing tests.

use super::*;

#[test]
fn a_non_close_action_keeps_its_producer_when_nothing_is_installed() {
    // The busy-runner error path installs nothing, so the modal that
    // produced the action must be restored rather than dropped.
    let produced = Modal::Detail(Box::new(Detail::browse()));
    assert!(matches!(
        retained_modal(true, produced, None),
        Some(Modal::Detail(_))
    ));
}

#[test]
fn a_close_action_dismisses_the_modal() {
    let produced = Modal::Help(Help);
    assert!(retained_modal(false, produced, None).is_none());
}

#[test]
fn a_modal_installed_by_execute_replaces_the_produced_one() {
    let produced = Modal::Help(Help);
    let installed = Modal::success("exported");
    assert!(matches!(
        retained_modal(true, produced, Some(installed)),
        Some(Modal::Alert(_))
    ));
}

#[test]
fn exporting_a_node_asks_for_a_path_and_writes_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = TreeState::new(Node::new("Root"));
    state.active_vault_path = Some(dir.path().join("vault.age").to_string_lossy().to_string());

    dispatch_key(ctrl(KeyCode::Char('e')), &mut state); // arm the prefix
    assert!(
        matches!(state.mode, InputMode::PendingExport),
        "Ctrl+E must arm the export/share prefix"
    );
    dispatch_key(key(KeyCode::Char('e')), &mut state); // `e` -> export wizard
    dispatch_key(key(KeyCode::Enter), &mut state); // "this node" -> format
    dispatch_key(key(KeyCode::Enter), &mut state); // JSON -> destination
    assert!(
        matches!(state.modal, Some(Modal::ExportDestination(_))),
        "the format step must open the File Browser"
    );

    dispatch_key(key(KeyCode::Char('e')), &mut state); // write the suggested name

    let written = dir.path().join("vault_Root.json");
    assert!(
        written.exists(),
        "the export must be written where it was named"
    );
    assert!(matches!(state.modal, Some(Modal::Alert(_))));
}

#[test]
fn ctrl_e_s_opens_the_share_wizard() {
    let mut state = TreeState::new(Node::new("Root"));

    dispatch_key(ctrl(KeyCode::Char('e')), &mut state);
    dispatch_key(key(KeyCode::Char('s')), &mut state);

    assert!(matches!(state.modal, Some(Modal::Share(_))));
    assert!(matches!(state.mode, InputMode::Normal));
}

#[test]
fn the_export_prefix_cancels_on_any_other_key() {
    let mut state = TreeState::new(Node::new("Root"));
    dispatch_key(ctrl(KeyCode::Char('e')), &mut state);

    dispatch_key(key(KeyCode::Char('x')), &mut state);

    assert!(matches!(state.mode, InputMode::Normal));
    assert!(state.modal.is_none());
}

#[test]
fn the_pending_export_mode_advertises_its_hint() {
    let mut state = TreeState::new(Node::new("Root"));
    state.mode = InputMode::PendingExport;

    let (mode, _, hint) = status_bar_parts(&state);

    assert_eq!(mode, texts::MODE_PENDING_EXPORT);
    assert_eq!(hint.as_ref(), texts::HINT_PENDING_EXPORT);
}

#[test]
fn sharing_a_branch_writes_a_bundle_where_it_was_named() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = TreeState::new(Node::new("Root"));
    state.active_vault_path = Some(dir.path().join("vault.age").to_string_lossy().to_string());

    dispatch_key(ctrl(KeyCode::Char('e')), &mut state); // arm the prefix
    dispatch_key(key(KeyCode::Char('s')), &mut state); // `s` -> share wizard
    dispatch_key(key(KeyCode::Enter), &mut state); // "this branch" -> destination
    assert!(matches!(state.modal, Some(Modal::ShareDestination(_))));

    dispatch_key(key(KeyCode::Char('n')), &mut state); // open the folder input
    dispatch_key(key(KeyCode::Enter), &mut state); // commit the suggested folder
    assert!(matches!(state.modal, Some(Modal::SharePassphrase(_))));

    for c in "pw".chars() {
        dispatch_key(key(KeyCode::Char(c)), &mut state);
    }
    dispatch_key(key(KeyCode::Tab), &mut state);
    for c in "pw".chars() {
        dispatch_key(key(KeyCode::Char(c)), &mut state);
    }
    dispatch_key(key(KeyCode::Enter), &mut state);

    assert!(
        matches!(state.modal, Some(Modal::Alert(_))),
        "a successful share must be reported in an alert"
    );
    assert!(
        state.operation_status.is_none(),
        "the blocking status is cleared once the write finishes"
    );

    let bundle = dir.path().join("vault_Root");
    let file = bundle.join("vault_Root.cretopass");
    assert!(
        crate::file_handling::is_age_file(&file),
        "the bundle file must be encrypted"
    );
    assert!(bundle.join("HOW-TO-OPEN.txt").is_file());
    let data = std::fs::read(&file).unwrap();
    assert!(crate::file_handling::decrypt_to_node(&data, "pw").is_ok());
}

#[test]
fn the_tick_guard_opens_while_a_destination_browser_is_loading() {
    use crate::file_browser::FileBrowserSession;
    use crate::file_handling::ExportFormat;
    use crate::tree::widget::ExportDestination;

    let dir = tempfile::tempdir().unwrap();
    let mut state = TreeState::new(Node::new("Root"));
    state.modal = Some(Modal::ExportDestination(Box::new(ExportDestination::new(
        Node::new("Login"),
        false,
        ExportFormat::Json,
        Some(&dir.path().join("vault.age")),
        &FileBrowserSession::default(),
    ))));

    assert!(
        browser_loading(&state),
        "a fresh browser waits on its listing"
    );

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while browser_loading(&state) && std::time::Instant::now() < deadline {
        poll_browser_listing(&mut state);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    assert!(
        !browser_loading(&state),
        "the loop helper must apply the completed listing"
    );
}

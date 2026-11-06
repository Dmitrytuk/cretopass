//! Autosave submission, exit planning and save-overlay tests.

use super::*;

/// A synthetic file error for tests that need a `SaveError::Failed`.
fn failed(message: &str) -> SaveError {
    SaveError::Failed(crate::file_handling::FileHandlingError::Io(
        io::Error::other(message),
    ))
}

#[test]
fn start_autosave_without_a_target_returns_none() {
    let (tx, _rx) = mpsc::unbounded_channel();
    assert!(start_autosave(None, tx).unwrap().is_none());
}

#[test]
fn runtime_supports_bounded_timeouts() {
    let runtime = save_runtime().unwrap();
    let timed_out = runtime.block_on(async {
        tokio::time::timeout(
            std::time::Duration::from_millis(1),
            std::future::pending::<()>(),
        )
        .await
    });

    assert!(timed_out.is_err());
}

#[test]
fn autosave_action_submits_a_modified_tree_to_a_live_worker() {
    assert_eq!(
        autosave_action(true, true, &SaveState::Idle),
        AutosaveAction::Submit
    );
    assert_eq!(
        autosave_action(true, true, &SaveState::Saving),
        AutosaveAction::Submit
    );
}

#[test]
fn autosave_action_reports_a_dead_worker_on_the_next_edit() {
    // The tree still knows it is modified, but the user is told the save
    // cannot happen instead of finding out only at exit.
    assert_eq!(
        autosave_action(true, false, &SaveState::Idle),
        AutosaveAction::ReportWorkerStopped
    );
    assert_eq!(
        autosave_action(true, false, &SaveState::Saving),
        AutosaveAction::ReportWorkerStopped
    );
}

#[test]
fn autosave_action_does_not_re_report_a_failure() {
    // The guard keys on `has_failed()`, not on acknowledgement: an
    // unacknowledged failure the user is looking at must not be reset.
    for acknowledged in [false, true] {
        let failed = SaveState::Failed {
            error: SaveError::WorkerStopped,
            acknowledged,
        };
        assert_eq!(autosave_action(true, false, &failed), AutosaveAction::Idle);
    }
}

#[test]
fn autosave_action_ignores_an_unmodified_tree() {
    assert_eq!(
        autosave_action(false, true, &SaveState::Idle),
        AutosaveAction::Idle
    );
    assert_eq!(
        autosave_action(false, false, &SaveState::Idle),
        AutosaveAction::Idle
    );
}

#[test]
fn exit_plan_resubmits_after_a_failed_save() {
    let save_state = SaveState::Failed {
        error: failed("disk full"),
        acknowledged: false,
    };
    assert_eq!(
        exit_plan(false, &save_state, 1, 2),
        ExitPlan::ResubmitThenWait
    );
}

#[test]
fn exit_plan_waits_for_an_in_flight_snapshot() {
    assert_eq!(
        exit_plan(false, &SaveState::Saving, 1, 2),
        ExitPlan::Wait(2)
    );
}

#[test]
fn exit_plan_is_up_to_date_when_everything_is_written() {
    assert_eq!(exit_plan(false, &SaveState::Idle, 2, 2), ExitPlan::UpToDate);
}

#[test]
fn exit_plan_resubmits_when_a_commit_is_unsubmitted() {
    assert_eq!(
        exit_plan(true, &SaveState::Idle, 2, 2),
        ExitPlan::ResubmitThenWait
    );
}

#[test]
fn blocked_overlay_retries_and_keeps_the_exit_request() {
    let mut state = TreeState::new(Node::new("Root"));
    state.save_exit = SaveExit::Blocked;
    state.quit = true;

    let action = handle_save_overlay(key(KeyCode::Char('r')), &mut state);

    assert!(matches!(action, OverlayAction::Retry));
    assert_eq!(state.save_exit, SaveExit::None);
    assert!(state.quit);
}

#[test]
fn blocked_overlay_confirms_then_discards() {
    let mut state = TreeState::new(Node::new("Root"));
    state.save_exit = SaveExit::Blocked;
    state.return_to_menu = true;

    let action = handle_save_overlay(key(KeyCode::Char('q')), &mut state);
    assert!(matches!(action, OverlayAction::Stay));
    assert_eq!(state.save_exit, SaveExit::ConfirmDiscard);

    let action = handle_save_overlay(key(KeyCode::Char('y')), &mut state);
    assert!(matches!(action, OverlayAction::Discard));
    assert!(state.return_to_menu);
}

#[test]
fn blocked_overlay_other_key_cancels_the_exit() {
    let mut state = TreeState::new(Node::new("Root"));
    state.save_exit = SaveExit::Blocked;
    state.quit = true;

    let action = handle_save_overlay(key(KeyCode::Char('x')), &mut state);

    assert!(matches!(action, OverlayAction::Stay));
    assert_eq!(state.save_exit, SaveExit::None);
    assert!(!state.quit);
}

#[tokio::test]
async fn wait_for_save_reports_the_target_generation() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut saved = 0;

    tx.send(SaveEvent {
        generation: 1,
        result: Ok(()),
    })
    .unwrap();
    tx.send(SaveEvent {
        generation: 3,
        result: Ok(()),
    })
    .unwrap();

    let result = wait_for_generation(&mut rx, &mut saved, 3).await;

    assert!(result.is_ok());
    assert_eq!(saved, 3);
}

#[tokio::test]
async fn wait_for_save_keeps_the_last_successful_generation_on_error() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut saved = 0;

    tx.send(SaveEvent {
        generation: 1,
        result: Ok(()),
    })
    .unwrap();
    tx.send(SaveEvent {
        generation: 2,
        result: Err(failed("disk full")),
    })
    .unwrap();

    let result = wait_for_generation(&mut rx, &mut saved, 2).await;

    assert!(matches!(result, Err(SaveError::Failed(_))));
    assert_eq!(saved, 1);
}

#[tokio::test]
async fn wait_for_save_detects_a_stopped_worker() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    drop(tx);
    let mut saved = 0;

    let result = wait_for_generation(&mut rx, &mut saved, 1).await;

    assert!(matches!(result, Err(SaveError::WorkerStopped)));
}

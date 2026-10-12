use super::*;
use crate::api::schema::Method;
use crossterm::event::{MouseButton, MouseEventKind};

fn close_state(confirm: bool, tab_count: usize) -> ClientShellState {
    let mut projected = snapshot();
    for number in 2..=tab_count {
        let mut tab = projected.tabs[0].clone();
        tab.tab_id = format!("tab_{number}");
        tab.number = number;
        tab.focused = false;
        projected.tabs.push(tab);
    }
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.config.confirm_close = confirm;
    state.set_snapshot(Box::new(projected));
    state.set_pane_surface(surface());
    state.compose(106, 24).unwrap();
    state
}

fn click(state: &mut ClientShellState, rect: Rect) -> ClientShellInput {
    state.handle_raw_events(vec![crate::raw_input::RawInputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: rect.x,
        row: rect.y,
        modifiers: KeyModifiers::empty(),
    })])
}

fn request_close(state: &mut ClientShellState, menu: bool) -> ClientShellInput {
    if menu {
        state.open_tab_context_menu("tab_1".into(), 30, 1);
        state.compose(106, 24).unwrap();
        let close_row = state.hits.context_menu_rows[2].0;
        click(state, close_row)
    } else {
        let mut outcome = ClientShellInput::default();
        state.record_binding(
            crate::input::KeybindMatch::Action(crate::input::KeybindAction::CloseTab),
            &mut outcome,
        );
        outcome
    }
}

fn assert_no_close(outcome: &ClientShellInput) {
    assert!(outcome.requests.is_empty());
    assert!(outcome.actions.iter().all(|action| {
        !matches!(action, ClientShellAction::Endpoint { request, .. }
            if matches!(request.method, Method::TabClose(_) | Method::WorkspaceClose(_)))
    }));
}

fn assert_tab_close(outcome: &ClientShellInput) {
    let methods = outcome
        .actions
        .iter()
        .filter_map(|action| match action {
            ClientShellAction::Endpoint { request, .. } => Some(&request.method),
            _ => None,
        })
        .filter(|method| !matches!(method, Method::TabFocus(_)))
        .collect::<Vec<_>>();
    assert!(matches!(methods.as_slice(), [Method::TabClose(target)] if target.tab_id == "tab_1"));
}

#[test]
fn last_tab_close_waits_for_keyboard_or_mouse_confirmation() {
    for menu in [false, true] {
        let mut state = close_state(true, 1);
        let requested = request_close(&mut state, menu);
        assert_no_close(&requested);
        assert!(requested.repaint);
        assert!(matches!(
            state.overlay,
            Some(ClientShellOverlay::ConfirmClose(_))
        ));
        let frame = state.compose(106, 24).unwrap();
        let text = frame_rows(&frame).join("\n");
        assert!(text.contains("Close workspace?"));
        assert!(text.contains("1: client-shell / 1: 1"));
        assert!(text.contains("1 pane"));
        let accepted = if menu {
            let primary = state.hits.overlay_primary;
            click(&mut state, primary)
        } else {
            state.handle_input_bytes(b"\r")
        };
        assert_tab_close(&accepted);
        assert!(state.overlay.is_none());
    }
}

#[test]
fn tab_close_confirmation_can_be_cancelled() {
    for tabs in [1, 2] {
        for mouse in [false, true] {
            let mut state = close_state(true, tabs);
            assert_no_close(&request_close(&mut state, false));
            assert!(matches!(
                state.overlay,
                Some(ClientShellOverlay::ConfirmClose(_))
            ));
            state.compose(106, 24).unwrap();
            let cancelled = if mouse {
                let cancel = state.hits.overlay_cancel;
                click(&mut state, cancel)
            } else {
                state.handle_input_bytes(b"\x1b")
            };
            assert_no_close(&cancelled);
            assert!(state.overlay.is_none());
        }
    }
}

#[test]
fn tab_close_stays_immediate_with_confirmation_disabled() {
    for tabs in [1, 2] {
        for menu in [false, true] {
            let mut state = close_state(false, tabs);
            assert_tab_close(&request_close(&mut state, menu));
            assert!(state.overlay.is_none());
        }
    }
}

#[test]
fn any_tab_close_waits_for_keyboard_or_mouse_confirmation_and_counts_only_its_panes() {
    for menu in [false, true] {
        let mut state = close_state(true, 2);
        let mut projected = state.snapshot.as_deref().unwrap().clone();
        projected.tabs[0].label = "build".into();
        for (id, tab) in [("pane_2", "tab_1"), ("pane_3", "tab_2")] {
            projected.panes.push(ClientShellPane {
                pane_id: id.into(),
                tab_id: tab.into(),
                focused: false,
                ..projected.panes[0].clone()
            });
        }
        state.set_snapshot(Box::new(projected));
        assert_no_close(&request_close(&mut state, menu));
        let text = frame_rows(&state.compose(106, 24).unwrap()).join("\n");
        assert!(text.contains("Close tab?"));
        assert!(text.contains("1: client-shell / 1: build"));
        assert!(text.contains("2 panes"));
        assert!(!text.contains("Close workspace?"));
        assert!(!text.contains("3 panes"));
        let accepted = if menu {
            let confirm = state.hits.overlay_primary;
            click(&mut state, confirm)
        } else {
            state.handle_input_bytes(b"\r")
        };
        assert_tab_close(&accepted);
        assert!(state.overlay.is_none());
    }
}

#[test]
fn last_tab_confirmation_preserves_target_across_focus_changes_and_new_tabs() {
    let mut state = close_state(true, 1);
    let mut projected = state.snapshot.as_deref().unwrap().clone();
    let mut other_workspace = projected.workspaces[0].clone();
    other_workspace.workspace_id = "ws_2".into();
    other_workspace.active_tab_id = "other_tab".into();
    other_workspace.focused = false;
    projected.workspaces.push(other_workspace);
    let mut other_tab = projected.tabs[0].clone();
    other_tab.workspace_id = "ws_2".into();
    other_tab.tab_id = "other_tab".into();
    other_tab.focused = false;
    projected.tabs.push(other_tab);
    state.set_snapshot(Box::new(projected.clone()));

    assert_no_close(&request_close(&mut state, false));
    assert!(matches!(
        state.overlay,
        Some(ClientShellOverlay::ConfirmClose(_))
    ));
    let mut new_tab = projected.tabs[0].clone();
    new_tab.tab_id = "new_tab".into();
    new_tab.focused = false;
    projected.tabs.push(new_tab);
    projected.focused_workspace_id = Some("ws_2".into());
    projected.focused_tab_id = Some("other_tab".into());
    for workspace in &mut projected.workspaces {
        workspace.focused = workspace.workspace_id == "ws_2";
    }
    for tab in &mut projected.tabs {
        tab.focused = tab.tab_id == "other_tab";
    }
    state.set_snapshot(Box::new(projected));
    assert_tab_close(&state.handle_input_bytes(b"\r"));
}

#[test]
fn last_tab_confirmation_rejects_missing_moved_or_reconnected_targets() {
    for tabs in [1, 2] {
        for change in ["missing", "moved", "reconnected"] {
            let mut state = close_state(true, tabs);
            assert_no_close(&request_close(&mut state, false));
            assert!(matches!(
                state.overlay,
                Some(ClientShellOverlay::ConfirmClose(_))
            ));
            let mut projected = state.snapshot.as_deref().unwrap().clone();
            match change {
                "missing" => projected.tabs.clear(),
                "moved" => projected.tabs[0].workspace_id = "different_workspace".into(),
                "reconnected" => state.endpoints[0].snapshot_generation = Some(2),
                _ => unreachable!(),
            }
            if change != "reconnected" {
                state.set_snapshot(Box::new(projected));
            }
            assert_no_close(&state.handle_input_bytes(b"\r"));
            assert!(state.overlay.is_none());
        }
    }
}

#[test]
fn last_tab_close_preserves_parent_group_and_linked_workspace_scope() {
    for linked in [false, true] {
        let mut state = close_state(true, 1);
        let mut projected = state.snapshot.as_deref().unwrap().clone();
        projected.workspaces[0].worktree = Some(ClientShellWorktree {
            key: "repo".into(),
            label: "repo".into(),
            is_linked_worktree: linked,
        });
        let mut sibling = projected.workspaces[0].clone();
        sibling.workspace_id = "ws_2".into();
        sibling.active_tab_id = "tab_2".into();
        sibling.focused = false;
        sibling.worktree.as_mut().unwrap().is_linked_worktree = !linked;
        projected.workspaces.push(sibling);
        let mut tab = projected.tabs[0].clone();
        tab.tab_id = "tab_2".into();
        tab.workspace_id = "ws_2".into();
        tab.focused = false;
        projected.tabs.push(tab);
        state.set_snapshot(Box::new(projected));

        let close = request_close(&mut state, false);
        if linked {
            assert_no_close(&close);
            assert!(matches!(
                state.overlay,
                Some(ClientShellOverlay::ConfirmClose(_))
            ));
            assert_tab_close(&state.handle_input_bytes(b"\r"));
        } else {
            assert_no_close(&close);
            assert!(matches!(state.overlay.as_ref(),
                Some(ClientShellOverlay::ConfirmClose(confirm)) if confirm.title == "Close worktree group?"));
            let accepted = state.handle_input_bytes(b"\r");
            assert!(matches!(accepted.actions.as_slice(),
                [ClientShellAction::Endpoint { request, .. }]
                    if matches!(&request.method, Method::WorkspaceClose(params)
                        if params.workspace_id == "ws_1" && params.close_group)));
        }
    }
}

#[test]
fn ordinary_parent_group_tab_close_confirms_only_the_tab() {
    let mut state = close_state(true, 2);
    let mut projected = state.snapshot.as_deref().unwrap().clone();
    projected.workspaces[0].worktree = Some(ClientShellWorktree {
        key: "repo".into(),
        label: "repo".into(),
        is_linked_worktree: false,
    });
    let mut child = projected.workspaces[0].clone();
    child.workspace_id = "ws_2".into();
    child.worktree.as_mut().unwrap().is_linked_worktree = true;
    projected.workspaces.push(child);
    state.set_snapshot(Box::new(projected));
    assert_no_close(&request_close(&mut state, false));
    assert!(
        matches!(state.overlay.as_ref(), Some(ClientShellOverlay::ConfirmClose(confirm))
        if confirm.title == "Close tab?")
    );
    assert_tab_close(&state.handle_input_bytes(b"\r"));
}

#[test]
fn tab_becoming_last_requires_updated_workspace_confirmation() {
    let mut state = close_state(true, 2);
    assert_no_close(&request_close(&mut state, false));
    let mut projected = state.snapshot.as_deref().unwrap().clone();
    projected.tabs.retain(|tab| tab.tab_id == "tab_1");
    state.set_snapshot(Box::new(projected));
    assert_no_close(&state.handle_input_bytes(b"\r"));
    assert!(
        matches!(state.overlay.as_ref(), Some(ClientShellOverlay::ConfirmClose(confirm))
        if confirm.title == "Close workspace?")
    );
    assert_tab_close(&state.handle_input_bytes(b"\r"));
}

#[test]
fn expanding_worktree_group_requires_fresh_confirmation() {
    let mut state = close_state(true, 1);
    let mut projected = state.snapshot.as_deref().unwrap().clone();
    projected.workspaces[0].worktree = Some(ClientShellWorktree {
        key: "repo".into(),
        label: "repo".into(),
        is_linked_worktree: false,
    });
    let mut child = projected.workspaces[0].clone();
    child.workspace_id = "ws_2".into();
    child.focused = false;
    child.worktree.as_mut().unwrap().is_linked_worktree = true;
    projected.workspaces.push(child.clone());
    state.set_snapshot(Box::new(projected.clone()));
    assert_no_close(&request_close(&mut state, false));
    child.workspace_id = "ws_3".into();
    projected.workspaces.push(child);
    state.set_snapshot(Box::new(projected));
    assert_no_close(&state.handle_input_bytes(b"\r"));
    assert!(
        matches!(state.overlay.as_ref(), Some(ClientShellOverlay::ConfirmClose(confirm))
        if confirm.title == "Close worktree group?" && confirm.detail.contains("3 workspaces"))
    );
    assert!(matches!(state.handle_input_bytes(b"\r").actions.as_slice(),
        [ClientShellAction::Endpoint { request, .. }]
            if matches!(&request.method, Method::WorkspaceClose(params)
                if params.workspace_id == "ws_1" && params.close_group)));
}

#[test]
fn group_tab_confirmation_does_not_close_newly_created_tabs() {
    let mut state = close_state(true, 1);
    let mut projected = state.snapshot.as_deref().unwrap().clone();
    projected.workspaces[0].worktree = Some(ClientShellWorktree {
        key: "repo".into(),
        label: "repo".into(),
        is_linked_worktree: false,
    });
    let mut child = projected.workspaces[0].clone();
    child.workspace_id = "ws_2".into();
    child.worktree.as_mut().unwrap().is_linked_worktree = true;
    projected.workspaces.push(child);
    state.set_snapshot(Box::new(projected.clone()));
    assert_no_close(&request_close(&mut state, false));
    let mut new_tab = projected.tabs[0].clone();
    new_tab.tab_id = "new_tab".into();
    projected.tabs.push(new_tab);
    state.set_snapshot(Box::new(projected));
    assert_tab_close(&state.handle_input_bytes(b"\r"));
}

#[test]
fn confirmation_indices_use_workspace_and_local_tab_positions_not_public_numbers() {
    for (extra_workspaces, other_local_tabs) in [(1, 2), (10, 10)] {
        let mut state = close_state(true, other_local_tabs + 1);
        let mut projected = state.snapshot.as_deref().unwrap().clone();
        projected.workspaces[0].number = 47;
        projected.tabs[0].number = 71;
        projected.tabs[0].label = "build".into();
        let workspace = projected.workspaces[0].clone();
        for index in 0..extra_workspaces {
            projected.workspaces.insert(
                index,
                ClientShellWorkspace {
                    workspace_id: format!("other_ws_{index}"),
                    focused: false,
                    number: 90 + index,
                    ..workspace.clone()
                },
            );
        }
        let target = projected.tabs.remove(0);
        let unrelated = ClientShellTab {
            tab_id: "unrelated_tab".into(),
            workspace_id: "other_ws_0".into(),
            focused: false,
            ..target.clone()
        };
        projected.tabs.insert(0, unrelated);
        projected.tabs.push(target);
        state.set_snapshot(Box::new(projected.clone()));
        assert_no_close(&request_close(&mut state, false));
        let Some(ClientShellOverlay::ConfirmClose(confirm)) = state.overlay.as_ref() else {
            panic!("tab confirmation");
        };
        assert_eq!(
            confirm.detail,
            format!(
                "{}: client-shell / {}: build — 1 pane",
                extra_workspaces + 1,
                other_local_tabs + 1
            )
        );
        assert_tab_close(&state.handle_input_bytes(b"\r"));
        assert_eq!(
            state.snapshot.as_deref().unwrap().workspaces,
            projected.workspaces
        );
        assert_eq!(state.snapshot.as_deref().unwrap().tabs, projected.tabs);
    }
}

#[test]
fn workspace_confirmation_indices_follow_grouped_and_collapsed_sidebar_order() {
    let mut state = close_state(true, 1);
    let mut projected = state.snapshot.as_deref().unwrap().clone();
    let mut parent = projected.workspaces[0].clone();
    parent.workspace_id = "parent".into();
    parent.focused = false;
    parent.worktree = Some(ClientShellWorktree {
        key: "repo".into(),
        label: "repo".into(),
        is_linked_worktree: false,
    });
    let mut child = parent.clone();
    child.workspace_id = "child".into();
    child.worktree.as_mut().unwrap().is_linked_worktree = true;
    projected.workspaces.insert(0, parent);
    projected.workspaces.push(child);
    state.set_snapshot(Box::new(projected));
    for (collapsed, expected_index) in [(false, 3), (true, 2)] {
        if collapsed {
            state.collapsed_groups.insert("repo".into());
        }
        state.open_confirm_close_overlay("ws_1".into());
        let Some(ClientShellOverlay::ConfirmClose(confirm)) = state.overlay.as_ref() else {
            panic!("workspace confirmation");
        };
        assert_eq!(confirm.title, "Close workspace?");
        assert_eq!(
            confirm.detail,
            format!("{expected_index}: client-shell — 1 pane")
        );
        assert!(confirm.tab_target.is_none());
    }
}

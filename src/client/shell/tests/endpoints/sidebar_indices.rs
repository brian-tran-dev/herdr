use super::*;

fn indexed_snapshot(workspace_count: usize, agent_count: usize) -> ClientShellSnapshot {
    let mut projected = snapshot();
    let workspace = projected.workspaces[0].clone();
    projected.workspaces = (0..workspace_count)
        .map(|index| ClientShellWorkspace {
            workspace_id: format!("ws_{}", index + 1),
            number: 100 + index,
            label: format!("space-{}", index + 1),
            custom_label: true,
            branch: None,
            focused: index == 0,
            ..workspace.clone()
        })
        .collect();
    projected.agents = (0..agent_count)
        .map(|index| ClientShellAgent {
            pane_id: format!("pane_{}", index + 1),
            focused: index == 0,
            ..agent(
                &format!("agent-{}", index + 1),
                AgentStatus::Idle,
                index as u64,
            )
        })
        .collect();
    let pane = projected.panes[0].clone();
    if agent_count > 0 {
        projected.panes = projected
            .agents
            .iter()
            .map(|agent| ClientShellPane {
                pane_id: agent.pane_id.clone(),
                focused: agent.focused,
                ..pane.clone()
            })
            .collect();
    }
    projected
}

fn badge_config() -> ClientShellConfig {
    let config: Config = toml::from_str(
        r#"
[ui.sidebar.spaces]
rows = [["state_icon", "workspace"]]
row_gap = 0
[ui.sidebar.agents]
rows = [["state_icon", "workspace"], ["agent"]]
row_gap = 0
"#,
    )
    .unwrap();
    ClientShellConfig::from_config(&config)
}

fn line(buffer: &Buffer, rect: Rect, row: u16) -> String {
    (rect.x..rect.right())
        .map(|x| buffer[(x, rect.y + row)].symbol())
        .collect()
}

fn expected_prefix(index: usize) -> String {
    format!("{}: ", index + 1)
}

fn expected_agent_prefix(status: AgentStatus, index: usize, config: &ClientShellConfig) -> String {
    let prefix = expected_prefix(index);
    format!(
        "{} {prefix}",
        super::super::super::status_icon(status, config.status_indicators)
    )
}

#[test]
fn sidebar_indices_continue_beyond_nine_with_no_placeholder() {
    for (index, expected) in [
        (0, "1: "),
        (8, "9: "),
        (9, "10: "),
        (10, "11: "),
        (98, "99: "),
        (99, "100: "),
    ] {
        assert_eq!(sidebar::index_prefix(index), expected);
    }
}

#[test]
fn workspace_badges_follow_display_order_without_renaming_snapshot() {
    let projected = indexed_snapshot(12, 0);
    let original = projected.workspaces.clone();
    let mut state = ClientShellState::new(badge_config());
    state.set_snapshot(Box::new(projected));
    let buffer = state.compose(120, 80).unwrap().to_ratatui_buffer().unwrap();
    assert_eq!(state.hits.workspaces.len(), 12);
    for (index, hit) in state.hits.workspaces.iter().enumerate() {
        let expected = format!("{}space-{}", expected_prefix(index), index + 1);
        assert!(line(&buffer, hit.rect, 0).contains(&expected), "{expected}");
        assert_eq!(hit.workspace_id, original[index].workspace_id);
    }
    assert_eq!(state.snapshot.as_ref().unwrap().workspaces, original);
}

#[test]
fn workspace_badges_follow_grouping_collapsing_and_shortcut_targets() {
    let mut projected = indexed_snapshot(3, 0);
    for (index, linked) in [(0, false), (2, true)] {
        projected.workspaces[index].worktree = Some(ClientShellWorktree {
            key: "repo".into(),
            label: "repo".into(),
            is_linked_worktree: linked,
        });
    }
    let mut state = ClientShellState::new(badge_config());
    state.set_snapshot(Box::new(projected));
    for collapsed in [false, true] {
        if collapsed {
            state.collapsed_groups.insert("repo".into());
        }
        let buffer = state.compose(120, 40).unwrap().to_ratatui_buffer().unwrap();
        let ids: Vec<_> = state
            .hits
            .workspaces
            .iter()
            .map(|hit| hit.workspace_id.clone())
            .collect();
        assert_eq!(
            ids,
            if collapsed {
                vec!["ws_1", "ws_2"]
            } else {
                vec!["ws_1", "ws_3", "ws_2"]
            }
        );
        for (index, hit) in state.hits.workspaces.iter().enumerate() {
            assert!(line(&buffer, hit.rect, 0).contains(&expected_prefix(index)));
        }
        let mut outcome = ClientShellInput::default();
        state.record_binding(
            crate::input::KeybindMatch::Action(crate::input::KeybindAction::SwitchWorkspace(1)),
            &mut outcome,
        );
        assert!(matches!(
            &outcome.actions[..],
            [ClientShellAction::Endpoint { request, .. }]
                if matches!(&request.method, crate::api::schema::Method::WorkspaceFocus(target)
                    if target.workspace_id == ids[1])
        ));
    }
}

#[test]
fn sidebar_badges_do_not_restart_at_scroll_position() {
    let mut state = ClientShellState::new(badge_config());
    state.set_snapshot(Box::new(indexed_snapshot(24, 12)));
    state.compose(120, 28).unwrap();
    state.workspace_scroll = 8;
    state.agent_scroll = state.hits.agent_max_scroll;
    let buffer = state.compose(120, 28).unwrap().to_ratatui_buffer().unwrap();
    assert_eq!(state.hits.workspaces[0].workspace_id, "ws_9");
    assert!(line(&buffer, state.hits.workspaces[0].rect, 0).contains("9: space-9"));
    assert!(line(&buffer, state.hits.workspaces[1].rect, 0).contains("10: space-10"));
    assert_ne!(state.hits.agents[0].1, "pane_1");
    for (rect, pane_id) in &state.hits.agents {
        let index = state
            .snapshot
            .as_ref()
            .unwrap()
            .agents
            .iter()
            .position(|agent| &agent.pane_id == pane_id)
            .unwrap();
        assert!(line(&buffer, *rect, 0)
            .trim_start()
            .starts_with(&expected_agent_prefix(
                AgentStatus::Idle,
                index,
                &state.config
            )));
    }
}

#[test]
fn workspace_removal_updates_badges_but_not_public_numbers() {
    let mut projected = indexed_snapshot(3, 0);
    projected.workspaces.remove(1);
    let mut state = ClientShellState::new(badge_config());
    state.set_snapshot(Box::new(projected));
    let buffer = state.compose(120, 40).unwrap().to_ratatui_buffer().unwrap();
    assert_eq!(state.hits.workspaces[1].workspace_id, "ws_3");
    assert!(line(&buffer, state.hits.workspaces[1].rect, 0).contains("2: space-3"));
    assert_eq!(state.snapshot.as_ref().unwrap().workspaces[1].number, 102);
}

#[test]
fn agent_badges_appear_only_on_first_line_and_preserve_names() {
    let projected = indexed_snapshot(1, 12);
    let original = projected.agents.clone();
    let mut state = ClientShellState::new(badge_config());
    state.set_snapshot(Box::new(projected));
    let buffer = state.compose(120, 80).unwrap().to_ratatui_buffer().unwrap();
    assert_eq!(state.hits.agents.len(), 12);
    for (index, (rect, pane_id)) in state.hits.agents.iter().enumerate() {
        assert!(line(&buffer, *rect, 0)
            .trim_start()
            .starts_with(&expected_agent_prefix(
                original[index].agent_status,
                index,
                &state.config
            )));
        assert_eq!(
            line(&buffer, *rect, 1).trim(),
            format!("agent-{}", index + 1)
        );
        assert_eq!(pane_id, &original[index].pane_id);
    }
    assert_eq!(state.snapshot.as_ref().unwrap().agents, original);
}

#[test]
fn agent_badges_track_priority_and_filtered_shortcut_order() {
    for filtered in [false, true] {
        let mut projected = indexed_snapshot(1, 12);
        projected.agents[11].agent_status = AgentStatus::Blocked;
        let mut config = badge_config();
        config.agent_panel_sort = crate::config::AgentPanelSortConfig::Priority;
        if filtered {
            projected.agent_view_label = Some("selected".into());
            projected.agent_order = vec!["pane_10".into(), "pane_2".into()];
        }
        let mut state = ClientShellState::new(config);
        state.set_snapshot(Box::new(projected));
        let buffer = state.compose(120, 80).unwrap().to_ratatui_buffer().unwrap();
        assert_eq!(
            state.hits.agents[0].1,
            if filtered { "pane_10" } else { "pane_12" }
        );
        assert!(line(&buffer, state.hits.agents[0].0, 0)
            .trim_start()
            .starts_with(&expected_agent_prefix(
                if filtered {
                    AgentStatus::Idle
                } else {
                    AgentStatus::Blocked
                },
                0,
                &state.config,
            )));
        let mut outcome = ClientShellInput::default();
        state.record_binding(
            crate::input::KeybindMatch::Action(crate::input::KeybindAction::FocusAgent(0)),
            &mut outcome,
        );
        assert!(matches!(
            &outcome.actions[..],
            [ClientShellAction::Endpoint { request, .. }]
                if matches!(&request.method, crate::api::schema::Method::PaneFocus(target)
                    if target.pane_id == state.hits.agents[0].1)
        ));
    }
}

#[test]
fn endpoint_badges_use_per_machine_workspace_and_combined_agent_shortcuts() {
    let (mut state, remote) = state_with_remote();
    state.config = badge_config();
    for endpoint in [ClientEndpointId::Local, remote.clone()] {
        state.set_endpoint_snapshot(&endpoint, Box::new(indexed_snapshot(12, 6)));
    }
    let buffer = state
        .compose(140, 120)
        .unwrap()
        .to_ratatui_buffer()
        .unwrap();
    assert_eq!(state.hits.workspaces.len(), 24);
    for endpoint in [ClientEndpointId::Local, remote] {
        for (index, hit) in state
            .hits
            .workspaces
            .iter()
            .filter(|hit| hit.endpoint_id == endpoint)
            .enumerate()
        {
            assert!(line(&buffer, hit.rect, 0).contains(&format!(
                "{}space-{}",
                expected_prefix(index),
                index + 1
            )));
        }
    }
    assert_eq!(state.hits.endpoint_agents.len(), 12);
    let targets = super::super::super::aggregate_navigation::online_agent_targets(
        &state.endpoints,
        &state.active_endpoint_id,
        state.config.agent_panel_sort,
    );
    for (index, (rect, endpoint, pane)) in state.hits.endpoint_agents.iter().enumerate() {
        assert!(line(&buffer, *rect, 0)
            .trim_start()
            .starts_with(&expected_agent_prefix(
                AgentStatus::Idle,
                index,
                &state.config
            )));
        assert_eq!(endpoint, &targets[index].endpoint_id);
        assert_eq!(pane, &targets[index].pane_id);
    }
}

#[test]
fn offline_agents_keep_display_indices_without_changing_online_shortcuts() {
    let (mut state, remote) = state_with_remote();
    state.config = badge_config();
    for endpoint in [ClientEndpointId::Local, remote.clone()] {
        state.set_endpoint_snapshot(&endpoint, Box::new(indexed_snapshot(1, 2)));
    }
    state.set_endpoint_status(&ClientEndpointId::Local, ClientEndpointStatus::Attention);
    let buffer = state.compose(120, 50).unwrap().to_ratatui_buffer().unwrap();
    assert_eq!(state.hits.endpoint_agents.len(), 4);
    for (index, (rect, _, _)) in state.hits.endpoint_agents.iter().enumerate() {
        let prefix = expected_agent_prefix(AgentStatus::Idle, index, &state.config);
        assert!(line(&buffer, *rect, 0).trim_start().starts_with(&prefix));
    }
    let targets = super::super::super::aggregate_navigation::online_agent_targets(
        &state.endpoints,
        &state.active_endpoint_id,
        state.config.agent_panel_sort,
    );
    assert_eq!(targets.len(), 2);
    assert!(targets.iter().all(|target| target.endpoint_id == remote));
}

#[test]
fn agent_badges_handle_icon_only_and_icon_free_first_lines() {
    for icon_only in [false, true] {
        let mut config = badge_config();
        config.agents.rows = vec![vec![if icon_only {
            crate::config::AgentSidebarToken::StateIcon
        } else {
            crate::config::AgentSidebarToken::Agent
        }]];
        let mut state = ClientShellState::new(config);
        state.set_snapshot(Box::new(indexed_snapshot(1, 1)));
        let buffer = state.compose(120, 40).unwrap().to_ratatui_buffer().unwrap();
        let rect = state.hits.agents[0].0;
        let expected = if icon_only {
            expected_agent_prefix(AgentStatus::Idle, 0, &state.config)
                .trim_end()
                .to_owned()
        } else {
            "1: agent-1".into()
        };
        assert_eq!(line(&buffer, rect, 0).trim(), expected);
        if icon_only {
            assert_eq!(
                buffer[(rect.x + 1, rect.y)].fg,
                super::super::super::status_color(AgentStatus::Idle, &state.config.palette)
            );
        }
    }
}

#[test]
fn workspace_badges_preserve_style_rules_for_the_original_name() {
    let projected = indexed_snapshot(1, 0);
    let config: Config = toml::from_str(
        r##"
[ui.sidebar.spaces]
rows = [[{ token = "workspace", rules = [{ equals = "space-1", fg = "#a7f3d0", bold = false, dim = false }] }]]
"##,
    )
    .unwrap();
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&config));
    state.set_snapshot(Box::new(projected));
    let buffer = state.compose(120, 40).unwrap().to_ratatui_buffer().unwrap();
    let rect = state.hits.workspaces[0].rect;
    let text = line(&buffer, rect, 0);
    let offset = text.find("1: space-1").unwrap() as u16;
    for x in rect.x + offset..rect.x + offset + 10 {
        assert_eq!(
            buffer[(x, rect.y)].fg,
            ratatui::style::Color::Rgb(167, 243, 208)
        );
        assert!(!buffer[(x, rect.y)]
            .modifier
            .intersects(Modifier::BOLD | Modifier::DIM));
    }
}

#[test]
fn badges_clip_safely_with_unicode_labels_and_tiny_sidebar_widths() {
    let mut projected = indexed_snapshot(1, 1);
    projected.workspaces[0].label = "機能 🦀".into();
    let config = badge_config();
    let row =
        super::super::super::agent_sidebar::agent_row(&projected, "pane_1", &config, None).unwrap();
    for display_index in [8, 9, 99] {
        for width in [0, 1, 2, 3, 4, 5, 8, 12, 24] {
            for agent_row in [false, true] {
                let area = Rect::new(0, 0, 40, 3);
                let mut buffer = Buffer::empty(area);
                for y in 0..area.height {
                    buffer.set_string(0, y, ".".repeat(40), Style::default());
                }
                let rect = Rect::new(1, 0, width, 2);
                if agent_row {
                    super::super::super::agent_sidebar::render_agent_row(
                        &mut buffer,
                        rect,
                        &row,
                        display_index,
                        &config,
                    );
                } else {
                    let mut rows = sidebar::workspace_rows(
                        &projected.workspaces[0],
                        AgentStatus::Idle,
                        false,
                        &config.spaces,
                    );
                    sidebar::prefix_workspace_rows(&mut rows, display_index);
                    sidebar::render_workspace_rows(
                        &mut buffer,
                        rect,
                        AgentStatus::Idle,
                        config.status_indicators,
                        &WorkspaceEntry {
                            index: 0,
                            indented: false,
                            last_child: false,
                        },
                        rows,
                        true,
                        false,
                        false,
                        false,
                        &config.palette,
                    );
                }
                for y in 0..area.height {
                    for x in 0..area.width {
                        if !super::super::super::contains(rect, (x, y)) {
                            assert_eq!(buffer[(x, y)].symbol(), ".", "{width}: ({x}, {y})");
                        }
                    }
                }
            }
        }
    }
    assert_eq!(projected.workspaces[0].label, "機能 🦀");
}

#[test]
fn collapsed_sidebar_keeps_existing_compact_labels() {
    let config = badge_config();
    let projected = indexed_snapshot(12, 12);
    let area = Rect::new(0, 0, 4, 40);
    let mut buffer = Buffer::empty(area);
    let mut hits = ShellHitMap::default();
    sidebar::render_collapsed_sidebar(&mut buffer, area, &projected, &config, None, &mut hits);
    let compact_label = line(&buffer, hits.workspaces[0].rect, 0);
    assert!(
        compact_label.trim_start().starts_with('1'),
        "compact label: {compact_label:?}"
    );
    for hit in &hits.workspaces {
        assert!(!line(&buffer, hit.rect, 0).contains(':'));
    }
    for (rect, _) in &hits.agents {
        assert!(!line(&buffer, *rect, 0).contains(':'));
    }
}

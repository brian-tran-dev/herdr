use std::{borrow::Cow, collections::HashMap};

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::Line,
    widgets::{Paragraph, Widget},
};

use super::*;

pub(super) struct AgentRow {
    pub(super) pane_id: String,
    pub(super) status: crate::api::schema::AgentStatus,
    pub(super) focused: bool,
    pub(super) rows: Vec<Vec<crate::ui::ResolvedToken>>,
}

pub(super) fn ordered_agent_pane_ids(
    snapshot: &ClientShellSnapshot,
    sort: crate::config::AgentPanelSortConfig,
) -> Vec<String> {
    if snapshot.agent_view_label.is_some() {
        return snapshot
            .agent_order
            .iter()
            .filter(|pane_id| {
                snapshot
                    .agents
                    .iter()
                    .any(|agent| agent.pane_id == pane_id.as_str())
            })
            .cloned()
            .collect();
    }
    let mut agents = snapshot.agents.iter().collect::<Vec<_>>();
    if sort == crate::config::AgentPanelSortConfig::Priority {
        agents.sort_by_key(|agent| {
            (
                std::cmp::Reverse(status_priority(agent.agent_status)),
                std::cmp::Reverse(agent.state_change_seq),
            )
        });
    }
    agents
        .into_iter()
        .map(|agent| agent.pane_id.clone())
        .collect()
}

pub(super) fn render_agent_panel(
    buffer: &mut Buffer,
    area: Rect,
    snapshot: &ClientShellSnapshot,
    config: &ClientShellConfig,
    agent_scroll: &mut usize,
    hits: &mut ShellHitMap,
) {
    if !render_agent_panel_header(
        buffer,
        area,
        snapshot.agent_view_label.as_deref(),
        config,
        hits,
    ) {
        return;
    }

    let rows = agent_rows(snapshot, config, None);
    render_agent_list(
        buffer,
        area,
        &rows,
        snapshot
            .agent_view_label
            .as_ref()
            .map(|_| " no matching agents"),
        config,
        agent_scroll,
        hits,
        |row| row.rows.len(),
        |buffer, rect, index, row, hits| {
            hits.agents.push((rect, row.pane_id.clone()));
            render_agent_row(buffer, rect, row, index, config);
        },
    );
}

pub(super) fn render_agent_panel_header(
    buffer: &mut Buffer,
    area: Rect,
    agent_view_label: Option<&str>,
    config: &ClientShellConfig,
    hits: &mut ShellHitMap,
) -> bool {
    if area.height == 0 {
        return false;
    }
    put_text(
        buffer,
        area.x,
        area.y,
        area.width,
        &"─".repeat(area.width as usize),
        Style::default().fg(config.palette.surface_dim),
    );
    if area.height < 2 {
        return false;
    }
    put_text(
        buffer,
        area.x,
        area.y + 1,
        area.width,
        " agents",
        Style::default()
            .fg(config.palette.overlay0)
            .add_modifier(Modifier::BOLD),
    );
    let sort_label = agent_view_label.unwrap_or(match config.agent_panel_sort {
        crate::config::AgentPanelSortConfig::Spaces => "grouped",
        crate::config::AgentPanelSortConfig::Priority => "priority",
    });
    let sort_width = display_width(sort_label).min(area.width as usize) as u16;
    let sort_rect = Rect::new(
        area.right().saturating_sub(sort_width),
        area.y + 1,
        sort_width,
        1,
    );
    hits.agent_sort_toggle = if config.mouse_capture && agent_view_label.is_none() {
        sort_rect
    } else {
        Rect::default()
    };
    put_text(
        buffer,
        sort_rect.x,
        sort_rect.y,
        sort_rect.width,
        sort_label,
        Style::default()
            .fg(if agent_view_label.is_some() {
                config.palette.accent
            } else {
                config.palette.overlay0
            })
            .add_modifier(Modifier::BOLD),
    );
    true
}

pub(super) fn render_agent_list<T>(
    buffer: &mut Buffer,
    area: Rect,
    rows: &[T],
    empty_message: Option<&str>,
    config: &ClientShellConfig,
    agent_scroll: &mut usize,
    hits: &mut ShellHitMap,
    row_lines: impl Fn(&T) -> usize,
    mut render_row: impl FnMut(&mut Buffer, Rect, usize, &T, &mut ShellHitMap),
) {
    let body = Rect::new(
        area.x,
        area.y.saturating_add(3),
        area.width,
        area.height.saturating_sub(3),
    );
    hits.agent_body = body;
    if body.is_empty() || rows.is_empty() {
        *agent_scroll = 0;
        if let Some(message) = empty_message.filter(|_| !body.is_empty()) {
            put_text(
                buffer,
                body.x,
                body.y,
                body.width,
                message,
                Style::default()
                    .fg(config.palette.overlay0)
                    .add_modifier(Modifier::DIM),
            );
        }
        return;
    }

    let row_heights = rows
        .iter()
        .map(|row| row_lines(row).max(1).min(u16::MAX as usize) as u16)
        .collect::<Vec<_>>();
    let gaps = rows
        .iter()
        .enumerate()
        .map(|(index, _)| {
            if index + 1 < rows.len() {
                config.agents.row_gap
            } else {
                0
            }
        })
        .collect::<Vec<_>>();
    let metrics =
        super::scroll::list_scroll_metrics(&row_heights, &gaps, body.height, *agent_scroll);
    hits.agent_max_scroll = metrics.max_offset_from_bottom;
    hits.agent_scroll_metrics = Some(metrics);
    *agent_scroll = metrics
        .max_offset_from_bottom
        .saturating_sub(metrics.offset_from_bottom);
    let show_scrollbar = metrics.max_offset_from_bottom > 0 && body.width > 1;
    let content_width = body.width.saturating_sub(u16::from(show_scrollbar));
    let mut y = body.y;
    for (index, row) in rows.iter().enumerate().skip(*agent_scroll) {
        let height = row_heights[index].min(body.height);
        if y.saturating_add(height) > body.bottom() {
            break;
        }
        let rect = Rect::new(body.x, y, content_width, height);
        render_row(buffer, rect, index, row, hits);
        y = y
            .saturating_add(height)
            .saturating_add(if index + 1 < rows.len() {
                config.agents.row_gap
            } else {
                0
            });
    }

    if show_scrollbar {
        let track = Rect::new(body.right().saturating_sub(1), body.y, 1, body.height);
        hits.agent_scrollbar = track;
        super::scroll::render_list_scrollbar(buffer, track, metrics, &config.palette);
    }
}

pub(super) fn agent_rows(
    snapshot: &ClientShellSnapshot,
    config: &ClientShellConfig,
    machine: Option<&str>,
) -> Vec<AgentRow> {
    ordered_agent_pane_ids(snapshot, config.agent_panel_sort)
        .into_iter()
        .filter_map(|pane_id| agent_row(snapshot, &pane_id, config, machine))
        .collect()
}

pub(super) fn agent_row(
    snapshot: &ClientShellSnapshot,
    pane_id: &str,
    config: &ClientShellConfig,
    machine: Option<&str>,
) -> Option<AgentRow> {
    let agent = snapshot
        .agents
        .iter()
        .find(|agent| agent.pane_id == pane_id)?;
    let workspace = snapshot
        .workspaces
        .iter()
        .find(|workspace| workspace.workspace_id == agent.workspace_id)?;
    let tab = snapshot.tabs.iter().find(|tab| tab.tab_id == agent.tab_id);
    let pane = snapshot
        .panes
        .iter()
        .find(|pane| pane.pane_id == agent.pane_id);
    let tab_label = tab.map(|tab| tab.label.as_str());
    let agent_label = agent
        .display_agent
        .as_deref()
        .or(agent.name.as_deref())
        .or(agent.agent.as_deref())
        .or(agent.title.as_deref());
    let labels = agent
        .state_labels
        .iter()
        .cloned()
        .collect::<HashMap<_, _>>();
    let tokens = agent.tokens.iter().cloned().collect::<HashMap<_, _>>();
    let state_text = labels
        .get(status_text(agent.agent_status))
        .map(String::as_str)
        .unwrap_or_else(|| sidebar_status_text(agent.agent_status));
    let canonical_agent = agent
        .agent
        .as_deref()
        .and_then(crate::detect::parse_agent_label);
    let rows = crate::ui::sidebar_agent_rows(
        &config.agents,
        crate::ui::AgentTokenContext {
            machine,
            workspace: &workspace.label,
            tab: tab_label,
            primary_pane_name: pane
                .and_then(|pane| pane.label.as_deref())
                .filter(|name| !name.trim().is_empty()),
            pane: agent
                .title
                .as_deref()
                .or_else(|| pane.and_then(|pane| pane.label.as_deref())),
            agent_label,
            terminal_title: agent.terminal_title.as_deref(),
            terminal_title_stripped: agent.terminal_title_stripped.as_deref(),
            canonical_agent,
            tokens: &tokens,
        },
        state_text,
    );
    Some(AgentRow {
        pane_id: agent.pane_id.clone(),
        status: agent.agent_status,
        focused: agent.focused,
        rows,
    })
}

pub(super) fn render_agent_row(
    buffer: &mut Buffer,
    rect: Rect,
    row: &AgentRow,
    display_index: usize,
    config: &ClientShellConfig,
) {
    let palette = &config.palette;
    let row_style = if row.focused {
        Style::default().bg(palette.active_row_bg)
    } else {
        Style::default()
    };
    let name_style = if row.focused {
        Style::default()
            .fg(palette.text)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(palette.subtext0)
            .add_modifier(Modifier::BOLD)
    };
    let status_style = Style::default().fg(status_color(row.status, palette));
    let secondary = Style::default().fg(palette.overlay0);
    let icon = (
        status_icon(row.status, config.status_indicators),
        Style::default().fg(status_color(row.status, palette)),
    );
    let rows = if row.rows.is_empty() {
        vec![vec![crate::ui::ResolvedToken {
            kind: crate::ui::ResolvedTokenKind::StateIcon,
            style: Default::default(),
        }]]
    } else {
        row.rows.clone()
    };
    for (index, tokens) in rows.iter().take(rect.height as usize).enumerate() {
        let indent = if index == 0 { 1 } else { 3 };
        let mut spans = vec![ratatui::text::Span::raw(" ".repeat(indent))];
        let prefix = if index == 0 {
            super::sidebar::index_prefix(display_index)
        } else {
            Cow::Borrowed("")
        };
        let mut token_spans = crate::ui::resolved_token_spans(
            tokens,
            icon,
            status_style,
            name_style,
            secondary,
            secondary,
            palette,
            rect.width
                .saturating_sub(indent as u16)
                .saturating_sub(prefix.len() as u16) as usize,
        );
        if !prefix.is_empty() {
            let badge_position = if matches!(
                tokens.first().map(|token| &token.kind),
                Some(crate::ui::ResolvedTokenKind::StateIcon)
            ) {
                // Keep the leading status icon and its separator ahead of the
                // badge, including rows whose only visible token is the icon.
                if token_spans.len() == 1 {
                    token_spans.push(ratatui::text::Span::raw(" "));
                }
                token_spans.len().min(2)
            } else {
                0
            };
            token_spans.insert(
                badge_position,
                ratatui::text::Span::styled(prefix, name_style),
            );
        }
        spans.extend(token_spans);
        Paragraph::new(Line::from(spans)).style(row_style).render(
            Rect::new(rect.x, rect.y + index as u16, rect.width, 1),
            buffer,
        );
    }
}

fn put_text(buffer: &mut Buffer, x: u16, y: u16, width: u16, text: &str, style: Style) {
    for (offset, character) in text.chars().take(width as usize).enumerate() {
        if let Some(cell) = buffer.cell_mut((x + offset as u16, y)) {
            cell.set_char(character).set_style(style);
        }
    }
}

fn display_width(text: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(text)
}

fn sidebar_status_text(status: crate::api::schema::AgentStatus) -> &'static str {
    use crate::api::schema::AgentStatus;
    match status {
        AgentStatus::Blocked => "blocked",
        AgentStatus::Done => "done",
        AgentStatus::Working => "working",
        AgentStatus::Idle | AgentStatus::Unknown => "idle",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::schema::AgentStatus;
    use crate::protocol::ClientShellAgent;

    fn snapshot_with_agent() -> ClientShellSnapshot {
        let mut snapshot = super::super::tests::snapshot();
        snapshot.workspaces[0].label = "repo".into();
        snapshot.tabs[0].label = "dev".into();
        snapshot.agents.push(ClientShellAgent {
            pane_id: "pane_1".into(),
            workspace_id: "ws_1".into(),
            tab_id: "tab_1".into(),
            name: Some("assigned-agent-name".into()),
            display_agent: Some("OpenCode".into()),
            agent: Some("opencode".into()),
            title: Some("reported-title".into()),
            terminal_title: Some("terminal-title".into()),
            terminal_title_stripped: Some("terminal-title".into()),
            agent_status: AgentStatus::Working,
            state_change_seq: 42,
            state_labels: Vec::new(),
            tokens: Vec::new(),
            focused: true,
        });
        snapshot
    }

    fn rendered_lines(row: &AgentRow, config: &ClientShellConfig) -> Vec<String> {
        let area = Rect::new(0, 0, 100, row.rows.len() as u16);
        let mut buffer = Buffer::empty(area);
        render_agent_row(&mut buffer, area, row, 0, config);
        (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    .trim()
                    .to_owned()
            })
            .collect()
    }

    #[test]
    fn agent_primary_label_prefers_manual_pane_name_and_preserves_details() {
        let mut snapshot = snapshot_with_agent();
        snapshot.panes[0].label = Some("reviewer".into());
        let original = snapshot.clone();
        let config = ClientShellConfig::from_config(&Config::default());
        let icon = status_icon(AgentStatus::Working, config.status_indicators);
        for (machine, expected) in [
            (None, format!("{icon} 1: reviewer")),
            (Some("Build"), format!("{icon} 1: Build · reviewer")),
        ] {
            let row = agent_row(&snapshot, "pane_1", &config, machine).unwrap();
            assert_eq!(rendered_lines(&row, &config), [expected, "OpenCode".into()]);
            assert_eq!(row.pane_id, "pane_1");
            assert_eq!(row.status, AgentStatus::Working);
            assert!(row.focused);
        }
        assert_eq!(snapshot, original, "presentation must not rename anything");
    }

    #[test]
    fn agent_primary_label_falls_back_for_missing_or_blank_pane_names() {
        let config = ClientShellConfig::from_config(&Config::default());
        let icon = status_icon(AgentStatus::Working, config.status_indicators);
        for label in [None, Some(""), Some(" \t ")] {
            for custom_label in [false, true] {
                let mut snapshot = snapshot_with_agent();
                snapshot.panes[0].label = label.map(str::to_owned);
                snapshot.tabs[0].custom_label = custom_label;
                let row = agent_row(&snapshot, "pane_1", &config, None).unwrap();
                assert_eq!(
                    rendered_lines(&row, &config),
                    [format!("{icon} 1: repo · dev"), "OpenCode".into()]
                );
            }
        }
        let mut snapshot = snapshot_with_agent();
        snapshot.panes.clear();
        let row = agent_row(&snapshot, "pane_1", &config, None).unwrap();
        assert_eq!(
            rendered_lines(&row, &config)[0],
            format!("{icon} 1: repo · dev")
        );
    }

    #[test]
    fn pane_first_primary_label_preserves_custom_rows_and_name_styles() {
        let mut snapshot = snapshot_with_agent();
        snapshot.panes[0].label = Some("reviewer".into());
        let config: Config = toml::from_str(
            r#"
[ui.sidebar.agents]
rows = [
    ["state_icon", { token = "workspace", dim = false, rules = [{ equals = "reviewer", bold = false }] }, "tab"],
    ["workspace", "tab"],
    ["agent"],
]
"#,
        )
        .unwrap();
        let config = ClientShellConfig::from_config(&config);
        let row = agent_row(&snapshot, "pane_1", &config, None).unwrap();
        let icon = status_icon(AgentStatus::Working, config.status_indicators);
        assert_eq!(
            rendered_lines(&row, &config),
            [
                format!("{icon} 1: reviewer"),
                "repo · dev".into(),
                "OpenCode".into()
            ]
        );
        assert_eq!(row.rows[0][1].style.dim, Some(false));
        assert_eq!(row.rows[0][1].style.bold, Some(false));

        let mut config = config;
        config.agents.rows = vec![vec![crate::config::AgentSidebarToken::Workspace]];
        let row = agent_row(&snapshot, "pane_1", &config, None).unwrap();
        assert_eq!(rendered_lines(&row, &config), ["1: repo"]);
    }
}

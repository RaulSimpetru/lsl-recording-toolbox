//! UI rendering for the TUI launcher.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Frame,
};

use super::app::{App, TOOLS};
use super::tab::TabMode;
use super::ui_dialog;
use super::ui_form;
use super::ui_tabs;

/// Render the entire UI based on application state.
pub fn render(frame: &mut Frame, app: &App) {
    // First render the main content
    if app.is_in_menu() {
        render_menu(frame, app);
    } else {
        render_tab_view(frame, app);
    }

    // Then render dialog overlay if needed
    if app.has_confirmation_dialog() {
        ui_dialog::render_close_confirmation(frame, app);
    }
}

/// Render the tool selection menu.
fn render_menu(frame: &mut Frame, app: &App) {
    // If there are tabs, show tab bar at top
    let (menu_area, has_tabs) = if !app.tabs.is_empty() {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Tab bar
                Constraint::Min(0),    // Menu content
            ])
            .split(frame.area());

        // Render tab bar (with "Menu" as active)
        render_menu_tab_bar(frame, chunks[0], app);
        (chunks[1], true)
    } else {
        (frame.area(), false)
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // Title
            Constraint::Min(0),    // Tool list
            Constraint::Length(3), // Help text / tab indicator
        ])
        .split(menu_area);

    // Title block
    let title_text = vec![
        Line::from(Span::styled(
            "LSL Recording Toolbox",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Select a tool to run:",
            Style::default().fg(Color::White),
        )),
    ];
    let title = Paragraph::new(title_text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );
    frame.render_widget(title, chunks[0]);

    // Tool list
    let items: Vec<ListItem> = TOOLS
        .iter()
        .enumerate()
        .map(|(i, tool)| {
            let is_selected = i == app.selected_index;
            let prefix = if is_selected { ">" } else { " " };
            let content = Line::from(vec![
                Span::styled(
                    format!("{} ", prefix),
                    Style::default().fg(if is_selected {
                        Color::Yellow
                    } else {
                        Color::DarkGray
                    }),
                ),
                Span::styled(
                    tool.name,
                    Style::default()
                        .fg(if is_selected { Color::Yellow } else { Color::White })
                        .add_modifier(if is_selected {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
                Span::styled(
                    format!(" - {}", tool.description),
                    Style::default().fg(if is_selected {
                        Color::Gray
                    } else {
                        Color::DarkGray
                    }),
                ),
            ]);
            ListItem::new(content)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Tools ")
            .border_style(Style::default().fg(Color::White)),
    );
    frame.render_widget(list, chunks[1]);

    // Help text
    let help_spans = if has_tabs {
        vec![
            Span::styled(" [", Style::default().fg(Color::DarkGray)),
            Span::styled("Up/Dn", Style::default().fg(Color::Cyan)),
            Span::styled("] Navigate  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[", Style::default().fg(Color::DarkGray)),
            Span::styled("Enter", Style::default().fg(Color::Cyan)),
            Span::styled("] New Tab  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[", Style::default().fg(Color::DarkGray)),
            Span::styled("Tab", Style::default().fg(Color::Cyan)),
            Span::styled("] Switch Tab  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[", Style::default().fg(Color::DarkGray)),
            Span::styled("Esc", Style::default().fg(Color::Cyan)),
            Span::styled("] Quit", Style::default().fg(Color::DarkGray)),
        ]
    } else {
        vec![
            Span::styled(" [", Style::default().fg(Color::DarkGray)),
            Span::styled("Up/Dn", Style::default().fg(Color::Cyan)),
            Span::styled("] Navigate  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[", Style::default().fg(Color::DarkGray)),
            Span::styled("Enter", Style::default().fg(Color::Cyan)),
            Span::styled("] Run  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[", Style::default().fg(Color::DarkGray)),
            Span::styled("Esc", Style::default().fg(Color::Cyan)),
            Span::styled("] Quit", Style::default().fg(Color::DarkGray)),
        ]
    };

    let help = Paragraph::new(Line::from(help_spans));
    frame.render_widget(help, chunks[2]);
}

/// Render the tab bar when in menu view (shows Menu as active + all tabs).
fn render_menu_tab_bar(frame: &mut Frame, area: Rect, app: &App) {
    let mut spans: Vec<Span> = Vec::new();

    // Menu item (active)
    spans.push(Span::styled("[", Style::default().fg(Color::Yellow)));
    spans.push(Span::styled(
        "[=] Menu",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::styled("]", Style::default().fg(Color::Yellow)));

    // Tab items
    for tab in app.tabs.iter() {
        spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));

        let status = match tab.mode {
            TabMode::Configure => "[*]",
            TabMode::Running => "[>]",
            TabMode::Completed => "[x]",
        };

        let status_style = match tab.mode {
            TabMode::Configure => Style::default().fg(Color::Cyan),
            TabMode::Running => Style::default().fg(Color::Green),
            TabMode::Completed => Style::default().fg(Color::Yellow),
        };

        // Truncate title if needed
        let max_title_len = 18;
        let title = if tab.title.len() > max_title_len {
            format!("{}...", &tab.title[..max_title_len - 3])
        } else {
            tab.title.clone()
        };

        spans.push(Span::styled(" ", Style::default()));
        spans.push(Span::styled(status, status_style));
        spans.push(Span::styled(" ", Style::default()));
        spans.push(Span::styled(title, Style::default().fg(Color::Gray)));
        spans.push(Span::styled(" ", Style::default()));
    }

    let line = Line::from(spans);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Tabs ")
        .title_bottom(Line::from(vec![
            Span::styled("[", Style::default().fg(Color::DarkGray)),
            Span::styled("Tab", Style::default().fg(Color::Cyan)),
            Span::styled("/", Style::default().fg(Color::DarkGray)),
            Span::styled("Shift+Tab", Style::default().fg(Color::Cyan)),
            Span::styled("] Cycle", Style::default().fg(Color::DarkGray)),
        ]))
        .border_style(Style::default().fg(Color::White));

    let tabs = Paragraph::new(line).block(block);
    frame.render_widget(tabs, area);
}

/// Render the tab view (tab bar + active tab content).
fn render_tab_view(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Tab bar
            Constraint::Min(0),    // Tab content
        ])
        .split(frame.area());

    // Render tab bar
    ui_tabs::render_tab_bar(frame, chunks[0], app);

    // Render active tab content
    if let Some(tab) = app.active_tab() {
        match tab.mode {
            TabMode::Configure => {
                if let Some(ref form) = tab.form_state {
                    let binary_name = TOOLS[tab.tool_index].binary;
                    ui_form::render_configure_form_for_tab(frame, chunks[1], form, binary_name);
                }
            }
            TabMode::Running | TabMode::Completed => {
                render_output_for_tab(frame, chunks[1], tab);
            }
        }
    }
}

/// Render the tool output view for a tab.
fn render_output_for_tab(frame: &mut Frame, area: ratatui::layout::Rect, tab: &super::tab::TabState) {
    // Status for border color and title
    let (status_text, status_color) = match tab.mode {
        TabMode::Running => ("Running", Color::Green),
        TabMode::Completed => ("Completed", Color::Yellow),
        TabMode::Configure => ("Configure", Color::Cyan), // Should not happen
    };

    // Show command with "$ " prefix, matching form style
    let display_text = tab.command.as_deref().unwrap_or(&tab.title);
    let cmd_with_prompt = format!("$ {}", display_text);

    // Calculate command box height based on content length and available width
    let inner_width = area.width.saturating_sub(2) as usize; // Account for borders
    let cmd_lines = if inner_width > 0 {
        ((cmd_with_prompt.len() + inner_width - 1) / inner_width) as u16
    } else {
        1
    };
    let cmd_height = cmd_lines + 2; // Add 2 for borders

    // Layout changes based on whether we show input line (only for Running mode)
    let chunks = if tab.mode == TabMode::Running {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(cmd_height), // Command (dynamic)
                Constraint::Min(0),             // Output
                Constraint::Length(3),          // Input field
                Constraint::Length(2),          // Help text
            ])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(cmd_height), // Command (dynamic)
                Constraint::Min(0),             // Output
                Constraint::Length(2),          // Help text
            ])
            .split(area)
    };

    // Command box styled like form's command preview
    let cmd_box = Paragraph::new(cmd_with_prompt)
        .style(Style::default().fg(Color::White))
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {} ", status_text))
                .border_style(Style::default().fg(status_color)),
        );
    frame.render_widget(cmd_box, chunks[0]);

    // Output area
    let output_area = chunks[1];
    let visible_height = output_area.height.saturating_sub(2) as usize; // Account for borders

    let output_lines: Vec<Line> = tab
        .output_lines
        .iter()
        .skip(tab.scroll_offset)
        .take(visible_height)
        .map(|s| Line::from(s.as_str()))
        .collect();

    let scroll_indicator = if tab.output_lines.len() > visible_height {
        let first_line = tab.scroll_offset + 1;
        let last_line = (tab.scroll_offset + visible_height).min(tab.output_lines.len());
        format!(" [lines {}-{}/{}] ", first_line, last_line, tab.output_lines.len())
    } else {
        String::new()
    };

    let output = Paragraph::new(output_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" Output{} ", scroll_indicator))
                .border_style(Style::default().fg(Color::White)),
        );
    frame.render_widget(output, output_area);

    // Render scrollbar if content exceeds visible area
    if tab.output_lines.len() > visible_height {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
        let mut scrollbar_state = ScrollbarState::new(tab.output_lines.len())
            .position(tab.scroll_offset);
        frame.render_stateful_widget(
            scrollbar,
            output_area.inner(ratatui::layout::Margin { vertical: 1, horizontal: 0 }),
            &mut scrollbar_state,
        );
    }

    // Input field (only for running mode)
    if tab.mode == TabMode::Running {
        let input_area = chunks[2];

        // Show input buffer with cursor
        let input_display = if tab.input_buffer.is_empty() {
            "Type here to send input to the process...".to_string()
        } else {
            tab.input_buffer.clone()
        };

        let input_style = if tab.input_buffer.is_empty() {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::White)
        };

        let input = Paragraph::new(format!("> {}", input_display))
            .style(input_style)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Input (Enter to send) ")
                    .border_style(Style::default().fg(Color::Cyan)),
            );
        frame.render_widget(input, input_area);
    }

    // Help text
    let help_chunk_idx = if tab.mode == TabMode::Running { 3 } else { 2 };
    let help_text = if tab.mode == TabMode::Running {
        vec![
            Span::styled(" [", Style::default().fg(Color::DarkGray)),
            Span::styled("PgUp/PgDn", Style::default().fg(Color::Cyan)),
            Span::styled("] Scroll  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[", Style::default().fg(Color::DarkGray)),
            Span::styled("Enter", Style::default().fg(Color::Cyan)),
            Span::styled("] Send  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[", Style::default().fg(Color::DarkGray)),
            Span::styled("Ctrl+C", Style::default().fg(Color::Cyan)),
            Span::styled("/", Style::default().fg(Color::DarkGray)),
            Span::styled("Esc", Style::default().fg(Color::Cyan)),
            Span::styled("] Stop", Style::default().fg(Color::DarkGray)),
        ]
    } else {
        vec![
            Span::styled(" [", Style::default().fg(Color::DarkGray)),
            Span::styled("Up/Dn", Style::default().fg(Color::Cyan)),
            Span::styled("] Scroll  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[", Style::default().fg(Color::DarkGray)),
            Span::styled("Enter", Style::default().fg(Color::Cyan)),
            Span::styled("/", Style::default().fg(Color::DarkGray)),
            Span::styled("Esc", Style::default().fg(Color::Cyan)),
            Span::styled("] Close Tab", Style::default().fg(Color::DarkGray)),
        ]
    };

    let help = Paragraph::new(Line::from(help_text));
    frame.render_widget(help, chunks[help_chunk_idx]);
}

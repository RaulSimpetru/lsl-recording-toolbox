//! UI rendering for the TUI launcher.

use ratatui::{
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Frame,
};

use super::app::{App, ToolCategory, TOOLS};
use super::tab::{TabMode, TabState};
use super::ui_dialog;
use super::ui_file_browser;
use super::ui_form;
use super::ui_helpers::{calculate_command_height, help_item, help_item_dual, render_tab_item};
use super::ui_tabs;

/// Render the entire UI based on application state.
pub fn render(frame: &mut Frame, app: &App) {
    if app.is_in_menu() {
        render_menu(frame, app);
    } else {
        render_tab_view(frame, app);
    }

    // Render dialog overlays (priority: file browser > rename > close confirmation)
    if let Some(ref browser) = app.file_browser {
        ui_file_browser::render_file_browser(frame, browser);
    } else if app.is_renaming() {
        ui_dialog::render_rename_dialog(frame, app);
    } else if app.has_confirmation_dialog() {
        ui_dialog::render_close_confirmation(frame, app);
    }
}

/// Render the tool selection menu.
fn render_menu(frame: &mut Frame, app: &App) {
    let (menu_area, has_tabs) = if !app.tabs.is_empty() {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Tab bar
                Constraint::Length(1), // Spacer
                Constraint::Min(0),    // Menu content
            ])
            .split(frame.area());

        render_menu_tab_bar(frame, chunks[0], app);
        (chunks[2], true)
    } else {
        (frame.area(), false)
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // Title
            Constraint::Length(1), // Spacer
            Constraint::Min(0),    // Tool list
            Constraint::Length(1), // Spacer
            Constraint::Length(2), // Help text
        ])
        .split(menu_area);

    // Title block
    let title = Paragraph::new(vec![
        Line::from(Span::styled(
            "LSL Recording Toolbox",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled("Select a tool to run:", Style::default().fg(Color::White))),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );
    frame.render_widget(title, chunks[0]);

    // Build tool list with category headers
    let items = build_tool_list_items(app);
    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Tools ")
            .border_style(Style::default().fg(Color::White)),
    );
    frame.render_widget(list, chunks[2]);

    // Help text
    let help_spans = build_menu_help_spans(has_tabs);
    let help = Paragraph::new(Line::from(help_spans));
    frame.render_widget(help, chunks[4]);
}

/// Build the list items for the tool menu.
fn build_tool_list_items(app: &App) -> Vec<ListItem<'static>> {
    let mut items: Vec<ListItem> = Vec::new();
    let mut current_category: Option<ToolCategory> = None;

    for (i, tool) in TOOLS.iter().enumerate() {
        // Add category header if this is a new category
        if current_category != Some(tool.category) {
            current_category = Some(tool.category);
            let header = Line::from(Span::styled(
                format!("-- {} --", tool.category.display_name()),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ));
            items.push(ListItem::new(header));
        }

        let is_selected = i == app.selected_index;
        let prefix = if is_selected { ">" } else { " " };
        let (prefix_color, name_color, desc_color) = if is_selected {
            (Color::Yellow, Color::Yellow, Color::Gray)
        } else {
            (Color::DarkGray, Color::White, Color::DarkGray)
        };

        let content = Line::from(vec![
            Span::styled(format!("  {} ", prefix), Style::default().fg(prefix_color)),
            Span::styled(
                tool.name,
                Style::default()
                    .fg(name_color)
                    .add_modifier(if is_selected { Modifier::BOLD } else { Modifier::empty() }),
            ),
            Span::styled(format!(" - {}", tool.description), Style::default().fg(desc_color)),
        ]);
        items.push(ListItem::new(content));
    }

    items
}

/// Build help spans for the menu view.
fn build_menu_help_spans(has_tabs: bool) -> Vec<Span<'static>> {
    let mut spans = vec![Span::styled(" ", Style::default())];
    spans.extend(help_item("Up/Dn", "Navigate "));

    if has_tabs {
        spans.extend(help_item("Enter", "New Tab "));
        spans.extend(help_item("Tab", "Switch Tab "));
    } else {
        spans.extend(help_item("Enter", "Run "));
    }

    spans.extend(help_item("Esc", "Quit"));
    spans
}

/// Render the tab bar when in menu view (shows Menu as active + all tabs).
fn render_menu_tab_bar(frame: &mut Frame, area: Rect, app: &App) {
    let mut spans: Vec<Span> = Vec::new();

    // Menu item (active)
    spans.push(Span::styled("[", Style::default().fg(Color::Yellow)));
    spans.push(Span::styled(
        "[=] Menu",
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::styled("]", Style::default().fg(Color::Yellow)));

    // Tab items (inactive)
    for tab in app.tabs.iter() {
        spans.extend(render_tab_item(tab.mode.clone(), &tab.title, false));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Tabs ")
        .title_bottom(Line::from(help_item_dual("Tab", "Shift+Tab", "Cycle Tabs")))
        .border_style(Style::default().fg(Color::White));

    let tabs = Paragraph::new(Line::from(spans)).block(block);
    frame.render_widget(tabs, area);
}

/// Render the tab view (tab bar + active tab content).
fn render_tab_view(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Tab bar
            Constraint::Length(1), // Spacer
            Constraint::Min(0),    // Tab content
        ])
        .split(frame.area());

    ui_tabs::render_tab_bar(frame, chunks[0], app);

    if let Some(tab) = app.active_tab() {
        match tab.mode {
            TabMode::Configure => {
                if let Some(ref form) = tab.form_state {
                    let binary_name = TOOLS[tab.tool_index].binary;
                    ui_form::render_configure_form_for_tab(frame, chunks[2], form, binary_name);
                }
            }
            TabMode::Running | TabMode::Completed => {
                render_output_for_tab(frame, chunks[2], tab);
            }
        }
    }
}

/// Render the tool output view for a tab.
fn render_output_for_tab(frame: &mut Frame, area: Rect, tab: &TabState) {
    let (status_text, status_color) = match tab.mode {
        TabMode::Running => ("Running", Color::Green),
        TabMode::Completed => ("Completed", Color::Yellow),
        TabMode::Configure => ("Configure", Color::Cyan),
    };

    let display_text = tab.command.as_deref().unwrap_or(&tab.title);
    let cmd_with_prompt = format!("$ {}", display_text);
    let (_, cmd_height) = calculate_command_height(cmd_with_prompt.len(), area.width);

    let is_running = tab.mode == TabMode::Running;
    let constraints: Vec<Constraint> = if is_running {
        vec![
            Constraint::Length(cmd_height), // Command
            Constraint::Length(1),          // Spacer
            Constraint::Min(0),             // Output
            Constraint::Length(1),          // Spacer
            Constraint::Length(3),          // Input field
            Constraint::Length(2),          // Help text
        ]
    } else {
        vec![
            Constraint::Length(cmd_height), // Command
            Constraint::Length(1),          // Spacer
            Constraint::Min(0),             // Output
            Constraint::Length(2),          // Help text
        ]
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    // Command box
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
    render_output_area(frame, chunks[2], tab);

    // Input field (running mode only)
    if is_running {
        render_input_field(frame, chunks[4], tab);
    }

    // Help text
    let help_chunk_idx = if is_running { 5 } else { 3 };
    let help_spans = build_output_help_spans(is_running);
    let help = Paragraph::new(Line::from(help_spans));
    frame.render_widget(help, chunks[help_chunk_idx]);
}

/// Render the output area with scrolling.
fn render_output_area(frame: &mut Frame, area: Rect, tab: &TabState) {
    let visible_height = area.height.saturating_sub(2) as usize;

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

    let output = Paragraph::new(output_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" Output{} ", scroll_indicator))
            .border_style(Style::default().fg(Color::White)),
    );
    frame.render_widget(output, area);

    // Render scrollbar if needed
    if tab.output_lines.len() > visible_height {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
        let mut scrollbar_state = ScrollbarState::new(tab.output_lines.len()).position(tab.scroll_offset);
        frame.render_stateful_widget(scrollbar, area.inner(Margin { vertical: 1, horizontal: 0 }), &mut scrollbar_state);
    }
}

/// Render the input field for running processes.
fn render_input_field(frame: &mut Frame, area: Rect, tab: &TabState) {
    let (input_display, input_style) = if tab.input_buffer.is_empty() {
        (
            "Type here to send input to the process...".to_string(),
            Style::default().fg(Color::DarkGray),
        )
    } else {
        (tab.input_buffer.clone(), Style::default().fg(Color::White))
    };

    let input = Paragraph::new(format!("> {}", input_display))
        .style(input_style)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Input (Enter to send) ")
                .border_style(Style::default().fg(Color::Cyan)),
        );
    frame.render_widget(input, area);
}

/// Build help spans for the output view.
fn build_output_help_spans(is_running: bool) -> Vec<Span<'static>> {
    let mut spans = vec![Span::styled(" ", Style::default())];

    if is_running {
        spans.extend(help_item("Up/Dn", "Scroll "));
        spans.extend(help_item("Enter", "Send "));
        spans.extend(help_item_dual("Ctrl+C", "Esc", "Stop"));
    } else {
        spans.extend(help_item("Up/Dn", "Scroll "));
        spans.extend(help_item_dual("Enter", "Esc", "Close Tab"));
    }

    spans
}

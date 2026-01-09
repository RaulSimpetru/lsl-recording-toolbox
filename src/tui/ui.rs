//! UI rendering for the TUI launcher.

use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use super::app::{App, AppMode, TOOLS};
use super::ui_form;

/// Render the entire UI based on application state.
pub fn render(frame: &mut Frame, app: &App) {
    match app.mode {
        AppMode::Menu => render_menu(frame, app),
        AppMode::Configure => ui_form::render_configure_form(frame, app),
        AppMode::Running | AppMode::Completed => render_output(frame, app),
    }
}

/// Render the tool selection menu.
fn render_menu(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // Title
            Constraint::Min(0),    // Tool list
            Constraint::Length(2), // Help text
        ])
        .split(frame.area());

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
    let help = Paragraph::new(Line::from(vec![
        Span::styled(" [", Style::default().fg(Color::DarkGray)),
        Span::styled("↑↓", Style::default().fg(Color::Cyan)),
        Span::styled("] Navigate  ", Style::default().fg(Color::DarkGray)),
        Span::styled("[", Style::default().fg(Color::DarkGray)),
        Span::styled("Enter", Style::default().fg(Color::Cyan)),
        Span::styled("] Run  ", Style::default().fg(Color::DarkGray)),
        Span::styled("[", Style::default().fg(Color::DarkGray)),
        Span::styled("Esc", Style::default().fg(Color::Cyan)),
        Span::styled("] Quit", Style::default().fg(Color::DarkGray)),
    ]));
    frame.render_widget(help, chunks[2]);
}

/// Render the tool output view.
fn render_output(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Min(0),    // Output
            Constraint::Length(2), // Help text
        ])
        .split(frame.area());

    // Title with running/completed status
    let status = match app.mode {
        AppMode::Running => ("Running", Color::Green),
        AppMode::Completed => ("Completed", Color::Yellow),
        AppMode::Menu | AppMode::Configure => ("", Color::White), // Should not happen
    };

    let tool_name = app
        .current_tool_name
        .as_deref()
        .unwrap_or("Unknown");

    let title_text = Line::from(vec![
        Span::styled("[", Style::default().fg(Color::DarkGray)),
        Span::styled(status.0, Style::default().fg(status.1)),
        Span::styled("] ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            tool_name,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let title = Paragraph::new(title_text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(status.1)),
    );
    frame.render_widget(title, chunks[0]);

    // Output area
    let output_area = chunks[1];
    let visible_height = output_area.height.saturating_sub(2) as usize; // Account for borders

    let output_lines: Vec<Line> = app
        .output_lines
        .iter()
        .skip(app.scroll_offset)
        .take(visible_height)
        .map(|s| Line::from(s.as_str()))
        .collect();

    let scroll_indicator = if app.output_lines.len() > visible_height {
        let first_line = app.scroll_offset + 1;
        let last_line = (app.scroll_offset + visible_height).min(app.output_lines.len());
        format!(" [lines {}-{}/{}] ", first_line, last_line, app.output_lines.len())
    } else {
        String::new()
    };

    let output = Paragraph::new(output_lines)
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" Output{} ", scroll_indicator))
                .border_style(Style::default().fg(Color::White)),
        );
    frame.render_widget(output, output_area);

    // Help text
    let help_text = if app.mode == AppMode::Running {
        vec![
            Span::styled(" [", Style::default().fg(Color::DarkGray)),
            Span::styled("↑↓", Style::default().fg(Color::Cyan)),
            Span::styled("] Scroll  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[", Style::default().fg(Color::DarkGray)),
            Span::styled("Ctrl+C", Style::default().fg(Color::Cyan)),
            Span::styled("/", Style::default().fg(Color::DarkGray)),
            Span::styled("Esc", Style::default().fg(Color::Cyan)),
            Span::styled("] Stop & Return", Style::default().fg(Color::DarkGray)),
        ]
    } else {
        vec![
            Span::styled(" [", Style::default().fg(Color::DarkGray)),
            Span::styled("↑↓", Style::default().fg(Color::Cyan)),
            Span::styled("] Scroll  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[", Style::default().fg(Color::DarkGray)),
            Span::styled("Enter", Style::default().fg(Color::Cyan)),
            Span::styled("/", Style::default().fg(Color::DarkGray)),
            Span::styled("Esc", Style::default().fg(Color::Cyan)),
            Span::styled("] Return to Menu", Style::default().fg(Color::DarkGray)),
        ]
    };

    let help = Paragraph::new(Line::from(help_text));
    frame.render_widget(help, chunks[2]);
}

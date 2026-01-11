//! File browser UI rendering.

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

use super::file_browser::FileBrowserState;

/// Render the file browser modal overlay.
pub fn render_file_browser(frame: &mut Frame, browser: &FileBrowserState) {
    let area = frame.area();

    // Calculate dialog size (80% of screen, max 80 cols)
    let dialog_width = (area.width * 80 / 100).clamp(40, 80);
    let dialog_height = (area.height * 80 / 100).clamp(10, 30);
    let x = (area.width.saturating_sub(dialog_width)) / 2;
    let y = (area.height.saturating_sub(dialog_height)) / 2;

    let dialog_area = Rect {
        x,
        y,
        width: dialog_width,
        height: dialog_height,
    };

    // Clear the area behind the dialog
    frame.render_widget(Clear, dialog_area);

    // Split into: title bar, path display, file list, help bar
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Path display with border
            Constraint::Min(3),    // File list
            Constraint::Length(2), // Help text
        ])
        .split(dialog_area);

    // Title based on mode
    let title = if browser.select_dir {
        " Select Directory "
    } else {
        " Select File "
    };

    // Current path display
    let path_text = browser.current_dir.to_string_lossy().to_string();
    let path_widget = Paragraph::new(path_text)
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
                .border_style(Style::default().fg(Color::Cyan))
                .style(Style::default().bg(Color::Black)),
        );
    frame.render_widget(path_widget, chunks[0]);

    // Calculate visible height for the file list
    let list_inner_height = chunks[1].height.saturating_sub(2) as usize; // -2 for borders

    // Build file list items
    let items: Vec<ListItem> = browser
        .entries
        .iter()
        .enumerate()
        .skip(browser.scroll_offset)
        .take(list_inner_height)
        .map(|(i, entry)| {
            let is_selected = i == browser.selected_index;
            let prefix = if is_selected { "> " } else { "  " };

            let (name_display, style) = if entry.is_dir {
                (
                    format!("{}/", entry.name),
                    if is_selected {
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Cyan)
                    },
                )
            } else {
                (
                    entry.name.clone(),
                    if is_selected {
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::White)
                    },
                )
            };

            let line = Line::from(vec![
                Span::styled(
                    prefix,
                    if is_selected {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    },
                ),
                Span::styled(name_display, style),
            ]);
            ListItem::new(line)
        })
        .collect();

    // Show error or file list
    if let Some(ref error) = browser.error {
        let error_widget = Paragraph::new(error.as_str())
            .style(Style::default().fg(Color::Red))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan))
                    .style(Style::default().bg(Color::Black)),
            );
        frame.render_widget(error_widget, chunks[1]);
    } else {
        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .style(Style::default().bg(Color::Black)),
        );
        frame.render_widget(list, chunks[1]);
    }

    // Help bar - use helper function for consistent formatting
    fn help_span(key: &str, action: &str) -> Vec<Span<'static>> {
        vec![
            Span::styled("[", Style::default().fg(Color::DarkGray)),
            Span::styled(key.to_string(), Style::default().fg(Color::Cyan)),
            Span::styled(format!("] {} ", action), Style::default().fg(Color::DarkGray)),
        ]
    }

    let mut help_spans = vec![Span::styled(" ", Style::default())];
    help_spans.extend(help_span("Up/Dn", "Navigate"));
    if browser.select_dir {
        help_spans.extend(help_span("Enter", "Open Dir"));
        help_spans.extend(help_span("Space", "Select This Dir"));
    } else {
        help_spans.extend(help_span("Enter", "Select/Open"));
        help_spans.extend(help_span("Backspace", "Go Up"));
    }
    help_spans.extend(help_span("Esc", "Cancel"));

    let help = Paragraph::new(Line::from(help_spans))
        .style(Style::default().bg(Color::Black))
        .alignment(Alignment::Center);
    frame.render_widget(help, chunks[2]);
}

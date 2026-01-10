//! Tab bar rendering for the TUI.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use super::app::App;
use super::tab::TabMode;

/// Render the tab bar at the top of the screen.
pub fn render_tab_bar(frame: &mut Frame, area: Rect, app: &App) {
    let mut spans: Vec<Span> = Vec::new();

    // Menu item (not active when viewing a tab)
    spans.push(Span::styled(" ", Style::default()));
    spans.push(Span::styled(
        "[=] Menu",
        Style::default().fg(Color::Gray),
    ));
    spans.push(Span::styled(" ", Style::default()));

    // Tab items
    for (i, tab) in app.tabs.iter().enumerate() {
        let is_active = Some(i) == app.active_tab_index;

        spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));

        // Status indicator
        let status = match tab.mode {
            TabMode::Configure => "[*]",
            TabMode::Running => "[>]",
            TabMode::Completed => "[x]",
        };

        // Tab title (truncate if needed)
        let max_title_len = 18;
        let title = if tab.title.len() > max_title_len {
            format!("{}...", &tab.title[..max_title_len - 3])
        } else {
            tab.title.clone()
        };

        let tab_style = if is_active {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };

        let status_style = match tab.mode {
            TabMode::Configure => Style::default().fg(Color::Cyan),
            TabMode::Running => Style::default().fg(Color::Green),
            TabMode::Completed => Style::default().fg(Color::Yellow),
        };

        // Tab content: [status title]
        if is_active {
            spans.push(Span::styled("[", Style::default().fg(Color::Yellow)));
        } else {
            spans.push(Span::styled(" ", Style::default()));
        }

        spans.push(Span::styled(status, status_style));
        spans.push(Span::styled(" ", Style::default()));
        spans.push(Span::styled(title, tab_style));

        if is_active {
            spans.push(Span::styled("]", Style::default().fg(Color::Yellow)));
        } else {
            spans.push(Span::styled(" ", Style::default()));
        }
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
            Span::styled("] Cycle Tabs/Menu  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[", Style::default().fg(Color::DarkGray)),
            Span::styled("Esc", Style::default().fg(Color::Cyan)),
            Span::styled("] Close", Style::default().fg(Color::DarkGray)),
        ]))
        .border_style(Style::default().fg(Color::White));

    let tabs = Paragraph::new(line).block(block);
    frame.render_widget(tabs, area);
}

//! Shared UI helper functions for the TUI.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

use super::tab::TabMode;

/// Maximum title length for tabs before truncation.
pub const MAX_TAB_TITLE_LEN: usize = 18;

/// Calculate the height needed for a wrapped command box.
/// Returns (line_count, box_height) where box_height includes borders.
pub fn calculate_command_height(text_len: usize, available_width: u16) -> (u16, u16) {
    let inner_width = available_width.saturating_sub(2) as usize;
    let lines = if inner_width > 0 {
        text_len.div_ceil(inner_width) as u16
    } else {
        1
    };
    (lines, lines + 2) // Add 2 for borders
}

/// Truncate a string with ellipsis if it exceeds max length.
pub fn truncate_with_ellipsis(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    } else {
        s.to_string()
    }
}

/// Get the status indicator and color for a tab mode.
pub fn tab_status_indicator(mode: TabMode) -> (&'static str, Color) {
    match mode {
        TabMode::Configure => ("[*]", Color::Cyan),
        TabMode::Running => ("[>]", Color::Green),
        TabMode::Completed => ("[x]", Color::Yellow),
    }
}

/// Create a help item with key and description, e.g., "[Key] Action  ".
pub fn help_item(key: &str, action: &str) -> Vec<Span<'static>> {
    vec![
        Span::styled("[", Style::default().fg(Color::DarkGray)),
        Span::styled(key.to_string(), Style::default().fg(Color::Cyan)),
        Span::styled(format!("] {} ", action), Style::default().fg(Color::DarkGray)),
    ]
}

/// Create a help item with green-highlighted key (for primary actions like "Run").
pub fn help_item_primary(key: &str, action: &str) -> Vec<Span<'static>> {
    vec![
        Span::styled("[", Style::default().fg(Color::DarkGray)),
        Span::styled(key.to_string(), Style::default().fg(Color::Green)),
        Span::styled(format!("] {} ", action), Style::default().fg(Color::DarkGray)),
    ]
}

/// Create a compound help item with two keys separated by "/".
pub fn help_item_dual(key1: &str, key2: &str, action: &str) -> Vec<Span<'static>> {
    vec![
        Span::styled("[", Style::default().fg(Color::DarkGray)),
        Span::styled(key1.to_string(), Style::default().fg(Color::Cyan)),
        Span::styled("/", Style::default().fg(Color::DarkGray)),
        Span::styled(key2.to_string(), Style::default().fg(Color::Cyan)),
        Span::styled(format!("] {} ", action), Style::default().fg(Color::DarkGray)),
    ]
}

/// Render a single tab item for the tab bar.
/// Returns spans for: " | [status title]" or " |  status title  " depending on active state.
pub fn render_tab_item(mode: TabMode, title: &str, is_active: bool) -> Vec<Span<'static>> {
    let (status, status_color) = tab_status_indicator(mode);
    let truncated_title = truncate_with_ellipsis(title, MAX_TAB_TITLE_LEN);

    let tab_style = if is_active {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };

    let (open_bracket, close_bracket) = if is_active {
        (
            Span::styled("[", Style::default().fg(Color::Yellow)),
            Span::styled("]", Style::default().fg(Color::Yellow)),
        )
    } else {
        (Span::styled(" ", Style::default()), Span::styled(" ", Style::default()))
    };

    vec![
        Span::styled(" | ", Style::default().fg(Color::DarkGray)),
        open_bracket,
        Span::styled(status, Style::default().fg(status_color)),
        Span::styled(" ", Style::default()),
        Span::styled(truncated_title, tab_style),
        close_bracket,
    ]
}

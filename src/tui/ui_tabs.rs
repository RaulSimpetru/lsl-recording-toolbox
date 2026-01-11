//! Tab bar rendering for the TUI.

use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use super::app::App;
use super::ui_helpers::{help_item, help_item_dual, render_tab_item};

/// Render the tab bar at the top of the screen.
pub fn render_tab_bar(frame: &mut Frame, area: Rect, app: &App) {
    let mut spans: Vec<Span> = Vec::new();

    // Menu item (inactive when viewing a tab)
    spans.push(Span::styled(" ", Style::default()));
    spans.push(Span::styled("[=] Menu", Style::default().fg(Color::Gray)));
    spans.push(Span::styled(" ", Style::default()));

    // Tab items
    for (i, tab) in app.tabs.iter().enumerate() {
        let is_active = Some(i) == app.active_tab_index;
        spans.extend(render_tab_item(tab.mode.clone(), &tab.title, is_active));
    }

    // Build help text
    let mut help_spans = vec![Span::styled(" ", Style::default())];
    help_spans.extend(help_item_dual("Tab", "Shift+Tab", "Cycle "));
    help_spans.extend(help_item("Ctrl+R", "Rename "));

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Tabs ")
        .title_bottom(Line::from(help_spans))
        .border_style(Style::default().fg(Color::White));

    let tabs = Paragraph::new(Line::from(spans)).block(block);
    frame.render_widget(tabs, area);
}

//! Dialog rendering for the TUI.

use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use super::app::App;

/// Render the close confirmation dialog as a centered modal.
pub fn render_close_confirmation(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Calculate centered dialog position
    let dialog_width = 50u16;
    let dialog_height = 7u16;
    let x = area.width.saturating_sub(dialog_width) / 2;
    let y = area.height.saturating_sub(dialog_height) / 2;

    let dialog_area = Rect {
        x,
        y,
        width: dialog_width.min(area.width),
        height: dialog_height.min(area.height),
    };

    // Get tab name for the dialog
    let tab_name = app
        .close_confirmation
        .as_ref()
        .and_then(|conf| app.tabs.get(conf.tab_index))
        .map(|tab| tab.title.as_str())
        .unwrap_or("Unknown");

    // Clear the dialog area
    frame.render_widget(Clear, dialog_area);

    // Build dialog content
    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("Tool ", Style::default().fg(Color::White)),
            Span::styled(
                tab_name,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" is still running.", Style::default().fg(Color::White)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Close anyway?", Style::default().fg(Color::Yellow)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("[", Style::default().fg(Color::DarkGray)),
            Span::styled("Y", Style::default().fg(Color::Green)),
            Span::styled("] Yes  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[", Style::default().fg(Color::DarkGray)),
            Span::styled("N", Style::default().fg(Color::Red)),
            Span::styled("] No  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[", Style::default().fg(Color::DarkGray)),
            Span::styled("D", Style::default().fg(Color::Cyan)),
            Span::styled("] Don't ask again", Style::default().fg(Color::DarkGray)),
        ]),
    ];

    let dialog = Paragraph::new(lines)
        .alignment(Alignment::Center)
        .style(Style::default().bg(Color::Black))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Confirm Close ")
                .title_style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )
                .border_style(Style::default().fg(Color::Yellow))
                .style(Style::default().bg(Color::Black)),
        );

    frame.render_widget(dialog, dialog_area);
}

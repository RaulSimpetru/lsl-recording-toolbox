//! Form rendering for tool configuration.

use ratatui::{
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Frame,
};

use super::form::{FieldType, FormField, FormState};
use super::tool_config;
use super::ui_helpers::{calculate_command_height, help_item, help_item_primary};

/// Render the configuration form for a tab.
pub fn render_configure_form_for_tab(frame: &mut Frame, area: Rect, form: &FormState, binary_name: &str) {
    let cmd = tool_config::build_command_preview(binary_name, form);
    let cmd_with_prompt = format!("$ {}", cmd);
    let (_, cmd_box_height) = calculate_command_height(cmd_with_prompt.len(), area.width);
    let bottom_height = cmd_box_height + 2; // cmd box + help text

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),             // Title
            Constraint::Length(1),             // Spacer
            Constraint::Min(0),                // Form fields
            Constraint::Length(1),             // Spacer
            Constraint::Length(bottom_height), // Command preview + help
        ])
        .split(area);

    // Title
    let title = Paragraph::new(Line::from(vec![
        Span::styled("Configure: ", Style::default().fg(Color::White)),
        Span::styled(
            &form.tool_name,
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );
    frame.render_widget(title, chunks[0]);

    // Form fields area
    render_form_fields(frame, form, chunks[2]);

    // Bottom area: command preview + help/error
    render_bottom_area(frame, form, chunks[4], &cmd_with_prompt);
}

/// Render the bottom area with command preview and help text.
fn render_bottom_area(frame: &mut Frame, form: &FormState, area: Rect, cmd_with_prompt: &str) {
    let (_, cmd_height) = calculate_command_height(cmd_with_prompt.len(), area.width);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(cmd_height), // Command preview
            Constraint::Length(2),          // Help/error
        ])
        .split(area);

    // Command preview with word wrap
    let cmd_preview = Paragraph::new(cmd_with_prompt)
        .style(Style::default().fg(Color::White))
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Command ")
                .border_style(Style::default().fg(Color::Green)),
        );
    frame.render_widget(cmd_preview, chunks[0]);

    // Help text or error message
    let help_or_error = if let Some(ref err) = form.error_message {
        Paragraph::new(Line::from(vec![
            Span::styled(" Error: ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::styled(err, Style::default().fg(Color::Red)),
        ]))
    } else {
        let mut spans = vec![Span::styled(" ", Style::default())];
        spans.extend(help_item_primary("Ctrl+Enter", "Run "));
        spans.extend(help_item("Up/Dn", "Navigate "));
        spans.extend(help_item("Esc", "Close"));
        Paragraph::new(Line::from(spans))
    };
    frame.render_widget(help_or_error, chunks[1]);
}

/// Render the form fields with scrolling.
fn render_form_fields(frame: &mut Frame, form: &FormState, area: Rect) {
    let inner_area = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    // Draw border
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Fields (Up/Dn to navigate) ")
        .border_style(Style::default().fg(Color::White));
    frame.render_widget(block, area);

    // Calculate visible items
    let visible_height = inner_area.height as usize;
    let total_items = form.fields.len();

    // Adjust scroll to keep active item visible
    let scroll_offset = if form.active_field_idx >= form.scroll_offset + visible_height {
        form.active_field_idx - visible_height + 1
    } else if form.active_field_idx < form.scroll_offset {
        form.active_field_idx
    } else {
        form.scroll_offset
    };

    // Render visible fields
    for (i, field) in form.fields.iter().enumerate().skip(scroll_offset).take(visible_height) {
        let y = inner_area.y + (i - scroll_offset) as u16;
        let is_active = i == form.active_field_idx;
        render_field(frame, field, inner_area.x, y, inner_area.width, is_active);
    }

    // Scrollbar if needed
    if total_items > visible_height {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
        let mut scrollbar_state = ScrollbarState::new(total_items).position(scroll_offset);
        frame.render_stateful_widget(scrollbar, area.inner(Margin { vertical: 1, horizontal: 0 }), &mut scrollbar_state);
    }
}

/// Render a single form field.
fn render_field(frame: &mut Frame, field: &FormField, x: u16, y: u16, width: u16, is_active: bool) {
    let label_width = 18u16;
    let input_width = width.saturating_sub(label_width + 3);

    // Label with required indicator
    let label_style = if is_active {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else if field.required {
        Style::default().fg(Color::White)
    } else {
        Style::default().fg(Color::Gray)
    };

    let label_text = if field.label.len() > label_width as usize - 1 {
        format!("{}:", &field.label[..label_width as usize - 1])
    } else {
        format!("{}:", field.label)
    };

    let label = Paragraph::new(label_text).style(label_style);
    frame.render_widget(label, Rect { x, y, width: label_width, height: 1 });

    // Input field
    let input_x = x + label_width + 1;

    match &field.field_type {
        FieldType::Bool => render_bool_field(frame, field, input_x, y, input_width, is_active),
        FieldType::Select(options) => render_select_field(frame, field, options, input_x, y, input_width, is_active),
        FieldType::Integer | FieldType::Float | FieldType::Text => render_text_field(frame, field, input_x, y, input_width, is_active),
        FieldType::Path { .. } => render_path_field(frame, field, input_x, y, input_width, is_active),
    }
}

/// Render a boolean toggle field.
fn render_bool_field(frame: &mut Frame, field: &FormField, x: u16, y: u16, width: u16, is_active: bool) {
    let is_true = field.value == "true";
    let bg_style = if is_active { Style::default().bg(Color::DarkGray) } else { Style::default() };

    // Determine styles: active option is bold, inactive is dimmed
    let (on_style, off_style) = if is_true {
        (
            Style::default().fg(Color::Green).add_modifier(if is_active { Modifier::BOLD } else { Modifier::empty() }),
            Style::default().fg(Color::DarkGray),
        )
    } else {
        (
            Style::default().fg(Color::DarkGray),
            Style::default().fg(Color::Red).add_modifier(if is_active { Modifier::BOLD } else { Modifier::empty() }),
        )
    };

    // " ON / OFF " = 10 chars, pad the rest with background
    let toggle_text = " ON / OFF ";
    let padding_width = width.saturating_sub(toggle_text.len() as u16) as usize;
    let padding = " ".repeat(padding_width);

    let toggle = Paragraph::new(Line::from(vec![
        Span::styled(" ", bg_style),
        Span::styled("ON", on_style.patch(bg_style)),
        Span::styled(" / ", bg_style.fg(Color::Gray)),
        Span::styled("OFF", off_style.patch(bg_style)),
        Span::styled(format!(" {}", padding), bg_style),
    ]));

    frame.render_widget(toggle, Rect { x, y, width, height: 1 });
}

/// Render a select/dropdown field.
fn render_select_field(frame: &mut Frame, field: &FormField, options: &[String], x: u16, y: u16, width: u16, is_active: bool) {
    let current_idx = field.select_idx;
    let total = options.len();

    let arrow_style = if is_active {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD).bg(Color::DarkGray)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let value_style = if is_active {
        Style::default().fg(Color::Yellow).bg(Color::DarkGray)
    } else {
        Style::default().fg(Color::White)
    };

    let bg_style = if is_active { Style::default().bg(Color::DarkGray) } else { Style::default() };

    // Format: < value > (n/m) + padding
    let counter = format!(" ({}/{})", current_idx + 1, total);
    let fixed_chars = 4 + counter.len(); // "< " + " >" + counter
    let max_value_width = width.saturating_sub(fixed_chars as u16) as usize;
    let display_value = if field.value.len() > max_value_width {
        format!("{}...", &field.value[..max_value_width.saturating_sub(3)])
    } else {
        field.value.clone()
    };

    // Calculate padding to fill the rest of the line
    let used_width = 2 + display_value.len() + 2 + counter.len(); // "< " + value + " >" + counter
    let padding_width = (width as usize).saturating_sub(used_width);
    let padding = " ".repeat(padding_width);

    let select = Paragraph::new(Line::from(vec![
        Span::styled("< ", arrow_style),
        Span::styled(display_value, value_style),
        Span::styled(" >", arrow_style),
        Span::styled(counter, Style::default().fg(Color::DarkGray).patch(bg_style)),
        Span::styled(padding, bg_style),
    ]));

    frame.render_widget(select, Rect { x, y, width, height: 1 });
}

/// Insert cursor indicator at the given position in a string.
fn insert_cursor(s: &str, cursor_pos: usize) -> String {
    let pos = cursor_pos.min(s.len());
    format!("{}|{}", &s[..pos], &s[pos..])
}

/// Render a text/number input field.
fn render_text_field(frame: &mut Frame, field: &FormField, x: u16, y: u16, width: u16, is_active: bool) {
    let value = &field.value;

    // Type indicator for numeric fields
    let type_indicator = match field.field_type {
        FieldType::Integer => " #",
        FieldType::Float => " .#",
        _ => "",
    };

    // Calculate max display width (account for brackets, cursor, and type indicator)
    let cursor_width = if is_active { 1 } else { 0 };
    let max_display = width.saturating_sub(2 + type_indicator.len() as u16) as usize - cursor_width;

    let (display_text, input_style) = if value.is_empty() && !is_active {
        // Show hint for empty inactive fields
        let hint = if field.hint.len() > max_display {
            format!("{}...", &field.hint[..max_display.saturating_sub(3)])
        } else {
            field.hint.clone()
        };
        (hint, Style::default().fg(Color::DarkGray))
    } else if is_active {
        // Active field: show value with cursor (or just cursor if empty)
        let truncated = if value.is_empty() {
            String::new()
        } else if value.len() > max_display {
            format!("{}...", &value[..max_display.saturating_sub(3)])
        } else {
            value.clone()
        };
        let with_cursor = insert_cursor(&truncated, field.cursor_pos.min(truncated.len()));
        (with_cursor, Style::default().fg(Color::Yellow).bg(Color::DarkGray))
    } else {
        // Inactive field with value
        let truncated = if value.len() > max_display {
            format!("{}...", &value[..max_display.saturating_sub(3)])
        } else {
            value.clone()
        };
        (truncated, Style::default().fg(Color::White))
    };

    let input = Paragraph::new(format!("[{}]{}", display_text, type_indicator)).style(input_style);
    frame.render_widget(input, Rect { x, y, width, height: 1 });
}

/// Render a path input field with browse indicator.
fn render_path_field(frame: &mut Frame, field: &FormField, x: u16, y: u16, width: u16, is_active: bool) {
    let value = &field.value;
    let browse_indicator = " [Space]";

    // Reserve space for browse indicator and cursor when active
    let browse_width = if is_active { browse_indicator.len() } else { 0 };
    let cursor_width = if is_active { 1 } else { 0 };
    let max_display = width.saturating_sub(2 + browse_width as u16) as usize - cursor_width;

    let (display_text, input_style) = if value.is_empty() && !is_active {
        // Show hint for empty inactive fields
        let hint = if field.hint.len() > max_display {
            format!("...{}", &field.hint[field.hint.len().saturating_sub(max_display.saturating_sub(3))..])
        } else {
            field.hint.clone()
        };
        (hint, Style::default().fg(Color::DarkGray))
    } else if is_active {
        // Active field: show value with cursor (or just cursor if empty)
        if value.is_empty() {
            ("|".to_string(), Style::default().fg(Color::Yellow).bg(Color::DarkGray))
        } else {
            let truncated = if value.len() > max_display {
                format!("...{}", &value[value.len().saturating_sub(max_display.saturating_sub(3))..])
            } else {
                value.clone()
            };
            // Adjust cursor position for truncation
            let visible_cursor_pos = if value.len() > max_display {
                let hidden_chars = value.len() - max_display + 3; // +3 for "..."
                field.cursor_pos.saturating_sub(hidden_chars) + 3 // +3 to account for "..."
            } else {
                field.cursor_pos
            };
            let with_cursor = insert_cursor(&truncated, visible_cursor_pos.min(truncated.len()));
            (with_cursor, Style::default().fg(Color::Yellow).bg(Color::DarkGray))
        }
    } else {
        // Inactive field with value
        let truncated = if value.len() > max_display {
            format!("...{}", &value[value.len().saturating_sub(max_display.saturating_sub(3))..])
        } else {
            value.clone()
        };
        (truncated, Style::default().fg(Color::White))
    };

    let spans = if is_active {
        vec![
            Span::styled(format!("[{}]", display_text), input_style),
            Span::styled(browse_indicator, Style::default().fg(Color::Cyan)),
        ]
    } else {
        vec![Span::styled(format!("[{}]", display_text), input_style)]
    };

    let input = Paragraph::new(Line::from(spans));
    frame.render_widget(input, Rect { x, y, width, height: 1 });
}

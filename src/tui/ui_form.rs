//! Form rendering for tool configuration.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Frame,
};

use super::form::{FieldType, FormState};
use super::tool_config;

/// Render the configuration form for a tab.
pub fn render_configure_form_for_tab(frame: &mut Frame, area: Rect, form: &FormState, binary_name: &str) {
    // Calculate command preview height first
    let cmd = tool_config::build_command_preview(binary_name, form);
    let cmd_with_prompt = format!("$ {}", cmd);
    let inner_width = area.width.saturating_sub(2) as usize;
    let cmd_lines = if inner_width > 0 {
        ((cmd_with_prompt.len() + inner_width - 1) / inner_width) as u16
    } else {
        1
    };
    let bottom_height = cmd_lines + 2 + 2; // cmd lines + borders + help text

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),            // Title
            Constraint::Min(0),               // Form fields
            Constraint::Length(bottom_height), // Command preview + help (auto-sized)
        ])
        .split(area);

    // Title
    let title = Paragraph::new(Line::from(vec![
        Span::styled("Configure: ", Style::default().fg(Color::White)),
        Span::styled(
            &form.tool_name,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );
    frame.render_widget(title, chunks[0]);

    // Form fields area
    render_form_fields(frame, form, chunks[1]);

    // Bottom area: command preview + help/error
    render_bottom_area(frame, form, chunks[2], binary_name, &cmd_with_prompt);
}

/// Render the bottom area with command preview and help text.
fn render_bottom_area(frame: &mut Frame, form: &super::form::FormState, area: Rect, _binary_name: &str, cmd_with_prompt: &str) {
    // Calculate command preview height based on content
    let inner_width = area.width.saturating_sub(2) as usize; // Account for borders
    let cmd_lines = if inner_width > 0 {
        ((cmd_with_prompt.len() + inner_width - 1) / inner_width) as u16
    } else {
        1
    };
    let cmd_height = cmd_lines + 2; // Add 2 for borders

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(cmd_height), // Command preview (auto-sized)
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
        Paragraph::new(Line::from(vec![
            Span::styled(" [", Style::default().fg(Color::DarkGray)),
            Span::styled("Up/Dn", Style::default().fg(Color::Cyan)),
            Span::styled("] Navigate  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[", Style::default().fg(Color::DarkGray)),
            Span::styled("Space/L/R", Style::default().fg(Color::Cyan)),
            Span::styled("] Toggle  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[", Style::default().fg(Color::DarkGray)),
            Span::styled("Tab", Style::default().fg(Color::Cyan)),
            Span::styled("] Cycle  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[", Style::default().fg(Color::DarkGray)),
            Span::styled("Esc", Style::default().fg(Color::Cyan)),
            Span::styled("] Close", Style::default().fg(Color::DarkGray)),
        ]))
    };
    frame.render_widget(help_or_error, chunks[1]);
}

/// Render the form fields with scrolling.
fn render_form_fields(frame: &mut Frame, form: &super::form::FormState, area: Rect) {
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

    // Calculate visible items (fields + 1 for Run button)
    let visible_height = inner_area.height as usize;
    let total_items = form.fields.len() + 1; // +1 for Run button

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

    // Render Run button if visible
    let button_idx = form.fields.len();
    if button_idx >= scroll_offset && button_idx < scroll_offset + visible_height {
        let y = inner_area.y + (button_idx - scroll_offset) as u16;
        let is_button_active = form.is_run_button_focused();
        render_run_button(frame, inner_area.x, y, inner_area.width, is_button_active);
    }

    // Scrollbar if needed
    if total_items > visible_height {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
        let mut scrollbar_state = ScrollbarState::new(total_items)
            .position(scroll_offset);
        frame.render_stateful_widget(
            scrollbar,
            area.inner(ratatui::layout::Margin { vertical: 1, horizontal: 0 }),
            &mut scrollbar_state,
        );
    }
}

/// Render the Run button.
fn render_run_button(frame: &mut Frame, x: u16, y: u16, width: u16, is_active: bool) {
    let button_text = "[ > RUN ]";
    let padding = (width as usize).saturating_sub(button_text.len()) / 2;
    let padded = format!("{:>width$}{}", "", button_text, width = padding);

    let style = if is_active {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD)
    };

    let button = Paragraph::new(padded).style(style);
    frame.render_widget(button, Rect { x, y, width, height: 1 });
}

/// Render a single form field.
fn render_field(frame: &mut Frame, field: &super::form::FormField, x: u16, y: u16, width: u16, is_active: bool) {
    let label_width = 18u16;
    let input_width = width.saturating_sub(label_width + 3); // 3 for ": " and spacing

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

    // Input field - render based on field type
    let input_x = x + label_width + 1;

    match &field.field_type {
        FieldType::Bool => {
            render_bool_field(frame, field, input_x, y, input_width, is_active);
        }
        FieldType::Select(options) => {
            render_select_field(frame, field, options, input_x, y, input_width, is_active);
        }
        FieldType::Integer | FieldType::Float | FieldType::Text => {
            render_text_field(frame, field, input_x, y, input_width, is_active);
        }
    }
}

/// Render a boolean toggle field.
fn render_bool_field(frame: &mut Frame, field: &super::form::FormField, x: u16, y: u16, width: u16, is_active: bool) {
    let is_true = field.value == "true";

    let (on_style, off_style) = if is_active {
        (
            if is_true {
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            },
            if !is_true {
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        )
    } else {
        (
            if is_true {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::DarkGray)
            },
            if !is_true {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        )
    };

    let bg_style = if is_active {
        Style::default().bg(Color::DarkGray)
    } else {
        Style::default()
    };

    let toggle = Paragraph::new(Line::from(vec![
        Span::styled(" ", bg_style),
        Span::styled("ON", on_style.patch(bg_style)),
        Span::styled(" / ", bg_style.fg(Color::Gray)),
        Span::styled("OFF", off_style.patch(bg_style)),
        Span::styled(" ", bg_style),
    ]));

    frame.render_widget(toggle, Rect { x, y, width: width.min(12), height: 1 });
}

/// Render a select/dropdown field.
fn render_select_field(
    frame: &mut Frame,
    field: &super::form::FormField,
    options: &[String],
    x: u16,
    y: u16,
    width: u16,
    is_active: bool,
) {
    let current_idx = field.select_idx;
    let total = options.len();

    let arrow_style = if is_active {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let value_style = if is_active {
        Style::default().fg(Color::Yellow).bg(Color::DarkGray)
    } else {
        Style::default().fg(Color::White)
    };

    let bg_style = if is_active {
        Style::default().bg(Color::DarkGray)
    } else {
        Style::default()
    };

    // Format: < value > (n/m)
    let value = &field.value;
    let max_value_width = width.saturating_sub(12) as usize; // Room for arrows and counter
    let display_value = if value.len() > max_value_width {
        format!("{}...", &value[..max_value_width.saturating_sub(3)])
    } else {
        value.clone()
    };

    let select = Paragraph::new(Line::from(vec![
        Span::styled("< ", arrow_style),
        Span::styled(display_value, value_style.patch(bg_style)),
        Span::styled(" >", arrow_style),
        Span::styled(format!(" ({}/{})", current_idx + 1, total), Style::default().fg(Color::DarkGray)),
    ]));

    frame.render_widget(select, Rect { x, y, width, height: 1 });
}

/// Render a text/number input field.
fn render_text_field(frame: &mut Frame, field: &super::form::FormField, x: u16, y: u16, width: u16, is_active: bool) {
    let input_style = if is_active {
        Style::default().fg(Color::Yellow).bg(Color::DarkGray)
    } else {
        Style::default().fg(Color::White)
    };

    // Build input display with cursor
    let value = &field.value;
    let display_value = if value.is_empty() && !is_active {
        // Show hint for empty inactive fields
        field.hint.clone()
    } else {
        value.clone()
    };

    // Truncate if too long
    let max_display = width.saturating_sub(2) as usize;
    let truncated = if display_value.len() > max_display {
        format!("{}...", &display_value[..max_display.saturating_sub(3)])
    } else {
        display_value
    };

    // Add type indicator for numeric fields
    let type_indicator = match field.field_type {
        FieldType::Integer => " #",
        FieldType::Float => " .#",
        _ => "",
    };

    let input = Paragraph::new(format!("[{}]{}", truncated, type_indicator))
        .style(if value.is_empty() && !is_active {
            Style::default().fg(Color::DarkGray)
        } else {
            input_style
        });
    frame.render_widget(input, Rect { x, y, width, height: 1 });
}

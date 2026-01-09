//! LSL Toolbox - TUI launcher for the LSL Recording Toolbox
//!
//! This is the main entry point when running `cargo run`. It provides a terminal
//! user interface to select and run the various LSL tools in the toolbox.

use std::io;
use std::panic;

use anyhow::Result;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

mod tui;

use crossterm::event::KeyCode;
use tui::{
    events::{
        is_backspace, is_ctrl_c, is_delete, is_down, is_end, is_enter, is_esc, is_home, is_left,
        is_page_down, is_page_up, is_right, is_shift_tab, is_space, is_tab, is_up, Event,
        EventHandler,
    },
    process::{ProcessEvent, ProcessManager},
    tool_config,
    ui::render,
    App, AppMode,
};

fn main() -> Result<()> {
    // Display license notice before entering TUI
    lsl_recording_toolbox::display_license_notice("lsl-toolbox");

    // Setup panic hook to restore terminal on panic
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        cleanup_terminal();
        original_hook(panic_info);
    }));

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run the application
    let result = run_app(&mut terminal);

    // Cleanup
    cleanup_terminal();

    result
}

fn cleanup_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen);
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let mut app = App::new();
    let events = EventHandler::default();
    let mut process_manager: Option<ProcessManager> = None;

    loop {
        // Get visible height for scrolling calculations
        let visible_height = terminal.get_frame().area().height.saturating_sub(7) as usize;

        // Draw the UI
        terminal.draw(|f| render(f, &app))?;

        // Handle events
        match events.next()? {
            Event::Key(key) => {
                match app.mode {
                    AppMode::Menu => {
                        if is_esc(&key) {
                            app.should_quit = true;
                        } else if is_up(&key) {
                            app.select_previous();
                        } else if is_down(&key) {
                            app.select_next();
                        } else if is_enter(&key) {
                            // Enter configure mode for the selected tool
                            app.enter_configure_mode();
                        }
                    }
                    AppMode::Configure => {
                        if is_esc(&key) {
                            // Return to menu without running
                            app.exit_configure_mode();
                        } else if is_tab(&key) || is_down(&key) {
                            // Move to next field
                            if let Some(ref mut form) = app.form_state {
                                form.next_field();
                            }
                        } else if is_shift_tab(&key) || is_up(&key) {
                            // Move to previous field
                            if let Some(ref mut form) = app.form_state {
                                form.prev_field();
                            }
                        } else if is_left(&key) {
                            // Move cursor left
                            if let Some(ref mut form) = app.form_state {
                                form.move_cursor_left();
                            }
                        } else if is_right(&key) {
                            // Move cursor right
                            if let Some(ref mut form) = app.form_state {
                                form.move_cursor_right();
                            }
                        } else if is_home(&key) {
                            // Move cursor to start
                            if let Some(ref mut form) = app.form_state {
                                form.move_cursor_home();
                            }
                        } else if is_end(&key) {
                            // Move cursor to end
                            if let Some(ref mut form) = app.form_state {
                                form.move_cursor_end();
                            }
                        } else if is_backspace(&key) {
                            // Delete char before cursor
                            if let Some(ref mut form) = app.form_state {
                                form.backspace();
                            }
                        } else if is_delete(&key) {
                            // Delete char at cursor
                            if let Some(ref mut form) = app.form_state {
                                form.delete_char();
                            }
                        } else if is_space(&key) || is_enter(&key) {
                            // Check if Run button is focused
                            if let Some(ref mut form) = app.form_state {
                                if form.is_run_button_focused() && is_enter(&key) {
                                    // Run the tool
                                    match tool_config::validate_form(form) {
                                        Ok(()) => {
                                            let tool = app.selected_tool();
                                            let args = app.get_configured_args().unwrap_or_default();
                                            let args_refs: Vec<&str> =
                                                args.iter().map(|s| s.as_str()).collect();

                                            match ProcessManager::spawn(tool, &args_refs) {
                                                Ok(pm) => {
                                                    app.start_tool(tool.name.to_string());
                                                    app.form_state = None;
                                                    process_manager = Some(pm);
                                                }
                                                Err(e) => {
                                                    app.start_tool(tool.name.to_string());
                                                    app.form_state = None;
                                                    app.add_output(format!("Error: {}", e));
                                                    app.tool_completed(None);
                                                }
                                            }
                                        }
                                        Err(msg) => {
                                            form.error_message = Some(msg);
                                        }
                                    }
                                } else if let Some(field) = form.active_field() {
                                    // Toggle bool or cycle select option (if field supports it)
                                    if !field.accepts_text_input() {
                                        form.toggle_or_cycle();
                                    }
                                }
                            }
                        } else if let KeyCode::Char(c) = key.code {
                            // Insert character at cursor
                            if let Some(ref mut form) = app.form_state {
                                form.insert_char(c);
                            }
                        }
                    }
                    AppMode::Running => {
                        if is_esc(&key) || is_ctrl_c(&key) {
                            // Kill the process and return to menu
                            if let Some(ref mut pm) = process_manager {
                                pm.kill();
                            }
                            process_manager = None;
                            app.return_to_menu();
                        } else if is_up(&key) {
                            app.scroll_up(1);
                        } else if is_down(&key) {
                            app.scroll_down(1, visible_height);
                        } else if is_page_up(&key) {
                            app.scroll_up(visible_height / 2);
                        } else if is_page_down(&key) {
                            app.scroll_down(visible_height / 2, visible_height);
                        }
                    }
                    AppMode::Completed => {
                        if is_esc(&key) || is_enter(&key) {
                            process_manager = None;
                            app.return_to_menu();
                        } else if is_up(&key) {
                            app.scroll_up(1);
                        } else if is_down(&key) {
                            app.scroll_down(1, visible_height);
                        } else if is_page_up(&key) {
                            app.scroll_up(visible_height / 2);
                        } else if is_page_down(&key) {
                            app.scroll_down(visible_height / 2, visible_height);
                        }
                    }
                }
            }
            Event::Resize(_, _) => {
                // Terminal will redraw on next iteration
            }
            Event::Tick => {
                // Check for process output
                if let Some(ref pm) = process_manager {
                    while let Some(event) = pm.try_recv() {
                        match event {
                            ProcessEvent::Output(line) => {
                                app.add_output(line);
                                // Auto-scroll to bottom
                                app.auto_scroll(visible_height);
                            }
                            ProcessEvent::Error(err) => {
                                app.add_output(format!("[Error: {}]", err));
                            }
                            ProcessEvent::Exited(_) => {
                                // Not used - exit is detected via check_exit()
                            }
                        }
                    }
                }

                // Check for process exit
                if let Some(ref mut pm) = process_manager {
                    if let Some(exit_code) = pm.check_exit() {
                        app.tool_completed(exit_code);
                    }
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

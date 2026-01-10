//! LSL Toolbox - TUI launcher for the LSL Recording Toolbox
//!
//! This is the main entry point when running `cargo run`. It provides a terminal
//! user interface to select and run the various LSL tools in the toolbox.
//! Supports multiple concurrent tools running in separate tabs.

use std::io;
use std::panic;

use anyhow::Result;
use crossterm::{
    cursor::{Hide, Show},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

mod tui;

use crossterm::event::KeyCode;
use tui::{
    app::TOOLS,
    events::{
        is_backspace, is_ctrl_c, is_delete, is_down, is_end, is_enter, is_esc, is_home, is_left,
        is_page_down, is_page_up, is_right, is_shift_tab, is_space, is_tab, is_up, Event,
        EventHandler,
    },
    process::{ProcessEvent, ProcessManager},
    tab::TabMode,
    tool_config,
    ui::render,
    App,
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
    execute!(stdout, EnterAlternateScreen, Hide)?;
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
    let _ = execute!(io::stdout(), LeaveAlternateScreen, Show);
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let mut app = App::new();
    let events = EventHandler::default();
    let mut needs_full_redraw = true; // Start with a full redraw

    loop {
        // Get current terminal size for scroll calculations and process spawning
        let term_size = terminal.get_frame().area();
        let term_width = term_size.width;
        let term_height = term_size.height;
        let visible_height = (term_height as usize).saturating_sub(15).max(1);

        for tab in &mut app.tabs {
            if matches!(tab.mode, TabMode::Running | TabMode::Completed) {
                // Update visible height and ensure scroll_offset stays valid
                tab.update_visible_height(visible_height);
            }
        }

        // Clear on specific events to prevent artifacts
        if needs_full_redraw {
            terminal.clear()?;
            needs_full_redraw = false;
        }

        // Draw the UI
        terminal.draw(|f| render(f, &app))?;

        // Handle events
        match events.next()? {
            Event::Key(key) => {
                // Handle file browser first (highest priority when open)
                if app.has_file_browser() {
                    let mut should_close = false;
                    let mut selected_path: Option<String> = None;

                    if let Some(browser) = app.file_browser_mut() {
                        if is_esc(&key) {
                            should_close = true;
                        } else if is_up(&key) {
                            browser.select_previous();
                        } else if is_down(&key) {
                            browser.select_next();
                        } else if is_page_up(&key) {
                            browser.page_up(10);
                        } else if is_page_down(&key) {
                            browser.page_down(10);
                        } else if is_backspace(&key) {
                            browser.go_up();
                        } else if is_enter(&key) {
                            // Enter directory or select file
                            if let Some(path) = browser.enter_selected() {
                                selected_path = Some(path.to_string_lossy().to_string());
                            }
                        } else if is_space(&key) && browser.select_dir {
                            // Space selects current directory (only in dir mode)
                            selected_path = Some(browser.select_current_dir().to_string_lossy().to_string());
                        }
                    }

                    if should_close {
                        app.close_file_browser();
                        needs_full_redraw = true;
                    } else if let Some(path) = selected_path {
                        // Get field index before closing browser
                        let field_idx = app.file_browser.as_ref().map(|b| b.field_index);
                        app.close_file_browser();

                        // Set the path in the form field
                        if let Some(idx) = field_idx {
                            if let Some(tab) = app.active_tab_mut() {
                                if let Some(ref mut form) = tab.form_state {
                                    if let Some(field) = form.fields.get_mut(idx) {
                                        field.value = path;
                                        field.cursor_pos = field.value.len();
                                    }
                                }
                            }
                        }
                        needs_full_redraw = true;
                    }
                    continue;
                }

                // Handle close confirmation dialog (high priority)
                if app.has_confirmation_dialog() {
                    if is_enter(&key) || key.code == KeyCode::Char('y') || key.code == KeyCode::Char('Y') {
                        app.confirm_close();
                        needs_full_redraw = true;
                    } else if is_esc(&key) || key.code == KeyCode::Char('n') || key.code == KeyCode::Char('N') {
                        app.cancel_close();
                        needs_full_redraw = true;
                    } else if key.code == KeyCode::Char('d') || key.code == KeyCode::Char('D') {
                        app.confirm_close_dont_ask();
                        needs_full_redraw = true;
                    }
                    continue;
                }

                // Global tab/menu navigation (works in any mode)
                if is_tab(&key) {
                    app.next_tab();
                    needs_full_redraw = true;
                    continue;
                } else if is_shift_tab(&key) {
                    app.prev_tab();
                    needs_full_redraw = true;
                    continue;
                }

                // Menu mode (no active tab)
                if app.is_in_menu() {
                    if is_esc(&key) {
                        app.should_quit = true;
                    } else if is_up(&key) {
                        app.select_previous();
                    } else if is_down(&key) {
                        app.select_next();
                    } else if is_enter(&key) {
                        // Create new tab for selected tool
                        app.create_tab_from_menu();
                        needs_full_redraw = true;
                    }
                } else {
                    // Tab mode - we have an active tab
                    // Track if we need a redraw after this event
                    let mut mode_changed = false;

                    // Get active tab for mode-specific handling
                    if let Some(tab) = app.active_tab_mut() {
                        match tab.mode {
                            TabMode::Configure => {
                                if is_esc(&key) {
                                    // Close tab or return to menu if only tab
                                    if app.tabs.len() == 1 {
                                        app.close_tab(0);
                                        mode_changed = true;
                                    } else {
                                        app.request_close_active_tab();
                                        mode_changed = true;
                                    }
                                } else if is_up(&key) {
                                    // Navigate to previous field
                                    if let Some(ref mut form) = tab.form_state {
                                        form.prev_field();
                                    }
                                } else if is_down(&key) {
                                    // Navigate to next field
                                    if let Some(ref mut form) = tab.form_state {
                                        form.next_field();
                                    }
                                } else if is_left(&key) {
                                    // Move cursor left / toggle
                                    if let Some(ref mut form) = tab.form_state {
                                        form.move_cursor_left();
                                    }
                                } else if is_right(&key) {
                                    // Move cursor right / toggle
                                    if let Some(ref mut form) = tab.form_state {
                                        form.move_cursor_right();
                                    }
                                } else if is_home(&key) {
                                    // Move cursor to start
                                    if let Some(ref mut form) = tab.form_state {
                                        form.move_cursor_home();
                                    }
                                } else if is_end(&key) {
                                    // Move cursor to end
                                    if let Some(ref mut form) = tab.form_state {
                                        form.move_cursor_end();
                                    }
                                } else if is_backspace(&key) {
                                    // Delete char before cursor
                                    if let Some(ref mut form) = tab.form_state {
                                        form.backspace();
                                    }
                                } else if is_delete(&key) {
                                    // Delete char at cursor
                                    if let Some(ref mut form) = tab.form_state {
                                        form.delete_char();
                                    }
                                } else if is_space(&key) || is_enter(&key) {
                                    // Check if Run button is focused
                                    if let Some(ref mut form) = tab.form_state {
                                        if form.is_run_button_focused() && is_enter(&key) {
                                            // Run the tool
                                            match form.validate() {
                                                Ok(()) => {
                                                    let tool = &TOOLS[tab.tool_index];
                                                    let args = tool_config::form_to_args(form);
                                                    let args_refs: Vec<&str> =
                                                        args.iter().map(|s| s.as_str()).collect();

                                                    match ProcessManager::spawn(tool, &args_refs, (term_width, term_height)) {
                                                        Ok(pm) => {
                                                            // Build command string for display
                                                            let cmd = if args.is_empty() {
                                                                tool.binary.to_string()
                                                            } else {
                                                                format!("{} {}", tool.binary, args.join(" "))
                                                            };
                                                            tab.start_running(pm, cmd);
                                                            mode_changed = true;
                                                        }
                                                        Err(e) => {
                                                            tab.form_state = None;
                                                            tab.add_output(format!("Error: {}", e));
                                                            tab.complete(None);
                                                            mode_changed = true;
                                                        }
                                                    }
                                                }
                                                Err(msg) => {
                                                    form.error_message = Some(msg);
                                                }
                                            }
                                        } else if let Some(field) = form.active_field() {
                                            // Check if this is a path field - open file browser
                                            if field.is_path_field() && is_space(&key) {
                                                let current_value = field.value.clone();
                                                let select_dir = field.selects_directory();
                                                let field_idx = form.active_field_idx;
                                                app.open_file_browser(&current_value, select_dir, field_idx);
                                                needs_full_redraw = true;
                                            } else if !field.accepts_text_input() {
                                                // Toggle bool or cycle select option
                                                form.toggle_or_cycle();
                                            }
                                        }
                                    }
                                } else if let KeyCode::Char(c) = key.code {
                                    // Insert character at cursor
                                    if let Some(ref mut form) = tab.form_state {
                                        form.insert_char(c);
                                    }
                                }
                            }
                            TabMode::Running => {
                                if is_esc(&key) || is_ctrl_c(&key) {
                                    // Kill the process and close tab
                                    app.request_close_active_tab();
                                    mode_changed = true;
                                } else if is_enter(&key) {
                                    // Send input to the process
                                    tab.send_input();
                                } else if is_up(&key) {
                                    tab.scroll_up(1);
                                } else if is_down(&key) {
                                    tab.scroll_down(1);
                                } else if is_page_up(&key) {
                                    let page_size = tab.cached_visible_height / 2;
                                    tab.scroll_up(page_size);
                                } else if is_page_down(&key) {
                                    let page_size = tab.cached_visible_height / 2;
                                    tab.scroll_down(page_size);
                                } else if is_left(&key) {
                                    tab.input_cursor_left();
                                } else if is_right(&key) {
                                    tab.input_cursor_right();
                                } else if is_home(&key) {
                                    tab.input_cursor_home();
                                } else if is_end(&key) {
                                    tab.input_cursor_end();
                                } else if is_backspace(&key) {
                                    tab.input_backspace();
                                } else if is_delete(&key) {
                                    tab.input_delete();
                                } else if let KeyCode::Char(c) = key.code {
                                    tab.input_insert(c);
                                }
                            }
                            TabMode::Completed => {
                                if is_esc(&key) || is_enter(&key) {
                                    // Close the completed tab
                                    app.request_close_active_tab();
                                    mode_changed = true;
                                } else if is_up(&key) {
                                    tab.scroll_up(1);
                                } else if is_down(&key) {
                                    tab.scroll_down(1);
                                } else if is_page_up(&key) {
                                    let page_size = tab.cached_visible_height / 2;
                                    tab.scroll_up(page_size);
                                } else if is_page_down(&key) {
                                    let page_size = tab.cached_visible_height / 2;
                                    tab.scroll_down(page_size);
                                }
                            }
                        }
                    }

                    if mode_changed {
                        needs_full_redraw = true;
                    }
                }
            }
            Event::Resize(_, h) => {
                // Update cached visible height for all tabs
                let new_visible_height = (h as usize).saturating_sub(15).max(1);
                for tab in &mut app.tabs {
                    if matches!(tab.mode, TabMode::Running | TabMode::Completed) {
                        tab.update_visible_height(new_visible_height);
                    }
                }
                needs_full_redraw = true;
            }
            Event::Tick => {
                // Process events for all tabs (not just active one)
                let mut any_completed = false;
                for tab in &mut app.tabs {
                    // Collect events first, then process them
                    let mut events_to_process = Vec::new();
                    if let Some(ref pm) = tab.process_manager {
                        while let Some(event) = pm.try_recv() {
                            events_to_process.push(event);
                        }
                    }

                    // Now process events (we can borrow tab mutably)
                    for event in events_to_process {
                        match event {
                            ProcessEvent::Output(line) => {
                                tab.add_output(line);
                                tab.auto_scroll();
                            }
                            ProcessEvent::Error(err) => {
                                tab.add_output(format!("[Error: {}]", err));
                            }
                            ProcessEvent::Exited(_) => {
                                // Not used - exit is detected via check_exit()
                            }
                        }
                    }

                    // Check for process exit
                    if let Some(ref mut pm) = tab.process_manager {
                        if let Some(exit_code) = pm.check_exit() {
                            tab.complete(exit_code);
                            any_completed = true;
                        }
                    }
                }
                if any_completed {
                    needs_full_redraw = true;
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

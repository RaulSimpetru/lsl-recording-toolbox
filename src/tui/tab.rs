//! Tab state management for multi-tool support.
//!
//! Each tab encapsulates the complete state for one tool instance:
//! form configuration, process management, and output display.

use super::form::FormState;
use super::process::ProcessManager;

/// Mode for an individual tab.
#[derive(Clone, PartialEq)]
pub enum TabMode {
    /// Configuring tool arguments via form
    Configure,
    /// Tool is actively running
    Running,
    /// Tool has completed execution
    Completed,
}

/// State of a single tab - encapsulates everything needed for one tool instance.
pub struct TabState {
    /// Unique identifier for this tab (for future tab management features)
    #[allow(dead_code)]
    pub id: usize,
    /// Display title for the tab (tool name)
    pub title: String,
    /// Index of the tool this tab is running (index into TOOLS)
    pub tool_index: usize,
    /// Current mode of this tab
    pub mode: TabMode,
    /// Command that was run (binary + args)
    pub command: Option<String>,
    /// Form state if in Configure mode
    pub form_state: Option<FormState>,
    /// Process manager if running
    pub process_manager: Option<ProcessManager>,
    /// Output buffer from process
    pub output_lines: Vec<String>,
    /// Scroll offset for output viewing
    pub scroll_offset: usize,
    /// Cached visible height for scroll calculations (updated on resize)
    pub cached_visible_height: usize,
    /// Whether auto-scroll is enabled (disabled when user manually scrolls up)
    pub auto_scroll_enabled: bool,
    /// Input buffer for interactive tools
    pub input_buffer: String,
    /// Cursor position in input buffer
    pub input_cursor: usize,
}

impl TabState {
    /// Create a new tab in Configure mode.
    pub fn new(id: usize, tool_index: usize, tool_name: &str, form: FormState) -> Self {
        Self {
            id,
            title: tool_name.to_string(),
            tool_index,
            mode: TabMode::Configure,
            command: None,
            form_state: Some(form),
            process_manager: None,
            output_lines: Vec::new(),
            scroll_offset: 0,
            cached_visible_height: 20, // Default, will be updated on first render
            auto_scroll_enabled: true,
            input_buffer: String::new(),
            input_cursor: 0,
        }
    }

    /// Start running the tool with the given process manager.
    pub fn start_running(&mut self, process_manager: ProcessManager, command: String) {
        self.mode = TabMode::Running;
        self.command = Some(command);
        self.form_state = None;
        self.process_manager = Some(process_manager);
        self.output_lines.clear();
        self.scroll_offset = 0;
        self.auto_scroll_enabled = true;
        self.input_buffer.clear();
        self.input_cursor = 0;
    }

    /// Mark the tool as completed with optional exit code.
    pub fn complete(&mut self, exit_code: Option<i32>) {
        self.mode = TabMode::Completed;
        self.process_manager = None;
        if let Some(code) = exit_code {
            self.output_lines
                .push(format!("\n[Process exited with code: {}]", code));
        } else {
            self.output_lines.push("\n[Process terminated]".to_string());
        }
    }

    /// Add output line from the running process.
    /// Sanitizes the line to remove ANSI escape sequences and control characters.
    pub fn add_output(&mut self, line: String) {
        const MAX_LINES: usize = 10000;
        const TRIM_AMOUNT: usize = 1000;
        if self.output_lines.len() >= MAX_LINES {
            self.output_lines.drain(0..TRIM_AMOUNT);
            self.scroll_offset = self.scroll_offset.saturating_sub(TRIM_AMOUNT);
        }
        self.output_lines.push(sanitize_output(&line));
    }

    /// Check if this tab has a running process.
    pub fn is_running(&self) -> bool {
        self.mode == TabMode::Running && self.process_manager.is_some()
    }

    /// Kill the running process if any.
    pub fn kill_process(&mut self) {
        if let Some(ref mut pm) = self.process_manager {
            pm.kill();
        }
        self.process_manager = None;
    }

    /// Scroll output up (disables auto-scroll).
    pub fn scroll_up(&mut self, amount: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
        self.auto_scroll_enabled = false;
    }

    /// Scroll output down. Re-enables auto-scroll if we reach the bottom.
    pub fn scroll_down(&mut self, amount: usize) {
        let max_scroll = self.output_lines.len().saturating_sub(self.cached_visible_height);
        self.scroll_offset = (self.scroll_offset + amount).min(max_scroll);
        // Re-enable auto-scroll if we reached the bottom
        if self.scroll_offset >= max_scroll {
            self.auto_scroll_enabled = true;
        }
    }

    /// Auto-scroll to bottom of output (only if auto-scroll is enabled).
    pub fn auto_scroll(&mut self) {
        if self.auto_scroll_enabled {
            let max_scroll = self.output_lines.len().saturating_sub(self.cached_visible_height);
            self.scroll_offset = max_scroll;
        }
    }

    /// Update the cached visible height and adjust scroll offset if needed.
    /// Called on terminal resize.
    pub fn update_visible_height(&mut self, new_height: usize) {
        self.cached_visible_height = new_height;
        // Clamp scroll_offset to valid range
        let max_scroll = self.output_lines.len().saturating_sub(new_height);
        if self.auto_scroll_enabled {
            // Maintain position at bottom
            self.scroll_offset = max_scroll;
        } else {
            // Clamp to valid range
            self.scroll_offset = self.scroll_offset.min(max_scroll);
        }
    }

    // Input buffer manipulation methods

    /// Insert a character at the cursor position.
    pub fn input_insert(&mut self, c: char) {
        if self.input_buffer.len() < 1024 {
            self.input_buffer.insert(self.input_cursor, c);
            self.input_cursor += 1;
        }
    }

    /// Delete character before cursor (backspace).
    pub fn input_backspace(&mut self) {
        if self.input_cursor > 0 {
            self.input_cursor -= 1;
            self.input_buffer.remove(self.input_cursor);
        }
    }

    /// Delete character at cursor (delete).
    pub fn input_delete(&mut self) {
        if self.input_cursor < self.input_buffer.len() {
            self.input_buffer.remove(self.input_cursor);
        }
    }

    /// Move cursor left.
    pub fn input_cursor_left(&mut self) {
        self.input_cursor = self.input_cursor.saturating_sub(1);
    }

    /// Move cursor right.
    pub fn input_cursor_right(&mut self) {
        if self.input_cursor < self.input_buffer.len() {
            self.input_cursor += 1;
        }
    }

    /// Move cursor to start.
    pub fn input_cursor_home(&mut self) {
        self.input_cursor = 0;
    }

    /// Move cursor to end.
    pub fn input_cursor_end(&mut self) {
        self.input_cursor = self.input_buffer.len();
    }

    /// Send input to the process and clear buffer.
    pub fn send_input(&mut self) -> Option<String> {
        if let Some(ref mut pm) = self.process_manager {
            let input = self.input_buffer.clone();
            if pm.write_line(&input).is_ok() {
                // Echo input to output
                self.output_lines.push(format!("> {}", input));
                self.input_buffer.clear();
                self.input_cursor = 0;
                return Some(input);
            }
        }
        None
    }
}

/// Sanitize output by removing ANSI escape sequences and control characters.
/// This prevents terminal artifacts when displaying process output.
fn sanitize_output(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            // ESC character - start of ANSI escape sequence
            '\x1b' => {
                // Skip the escape sequence
                if let Some(&next) = chars.peek() {
                    if next == '[' {
                        chars.next(); // consume '['
                        // Skip until we hit a letter (end of CSI sequence)
                        while let Some(&seq_char) = chars.peek() {
                            chars.next();
                            if seq_char.is_ascii_alphabetic() {
                                break;
                            }
                        }
                    } else if next == ']' {
                        // OSC sequence - skip until BEL or ST
                        chars.next(); // consume ']'
                        while let Some(&seq_char) = chars.peek() {
                            chars.next();
                            if seq_char == '\x07' || seq_char == '\\' {
                                break;
                            }
                        }
                    }
                }
            }
            // Filter out other control characters except tab and newline
            c if c.is_control() && c != '\t' && c != '\n' => {}
            // Keep normal characters
            _ => result.push(c),
        }
    }

    result
}

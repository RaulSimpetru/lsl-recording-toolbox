//! Application state management for the TUI launcher.

use std::env;
use std::path::PathBuf;

use super::form::FormState;
use super::tool_config;

/// Metadata for a tool in the toolbox.
#[derive(Clone)]
pub struct ToolMetadata {
    /// Display name of the tool
    pub name: &'static str,
    /// Binary name (used for execution)
    pub binary: &'static str,
    /// Short description of what the tool does
    pub description: &'static str,
}

/// All available tools in the toolbox.
pub const TOOLS: &[ToolMetadata] = &[
    ToolMetadata {
        name: "LSL Recorder",
        binary: "lsl-recorder",
        description: "Record a single LSL stream to Zarr format",
    },
    ToolMetadata {
        name: "LSL Multi-Recorder",
        binary: "lsl-multi-recorder",
        description: "Record multiple LSL streams simultaneously",
    },
    ToolMetadata {
        name: "LSL Inspect",
        binary: "lsl-inspect",
        description: "Inspect Zarr recording contents and metadata",
    },
    ToolMetadata {
        name: "LSL Validate",
        binary: "lsl-validate",
        description: "Validate recording synchronization quality",
    },
    ToolMetadata {
        name: "LSL Sync",
        binary: "lsl-sync",
        description: "Synchronize timestamps across streams",
    },
    ToolMetadata {
        name: "LSL Replay",
        binary: "lsl-replay",
        description: "Replay recorded LSL streams",
    },
    ToolMetadata {
        name: "LSL Dummy Stream",
        binary: "lsl-dummy-stream",
        description: "Generate test LSL streams for development",
    },
];

/// Current mode of the application.
#[derive(Clone, PartialEq)]
pub enum AppMode {
    /// Browsing the tool menu
    Menu,
    /// Configuring tool arguments
    Configure,
    /// A tool is currently running
    Running,
    /// Tool has finished, viewing output
    Completed,
}

/// Main application state.
pub struct App {
    /// Currently selected tool index
    pub selected_index: usize,
    /// Current application mode
    pub mode: AppMode,
    /// Form state for Configure mode
    pub form_state: Option<FormState>,
    /// Output buffer from running/completed tool
    pub output_lines: Vec<String>,
    /// Scroll offset for output viewing
    pub scroll_offset: usize,
    /// Name of the currently running/completed tool
    pub current_tool_name: Option<String>,
    /// Whether the application should quit
    pub should_quit: bool,
}

impl App {
    /// Create a new application instance.
    pub fn new() -> Self {
        Self {
            selected_index: 0,
            mode: AppMode::Menu,
            form_state: None,
            output_lines: Vec::new(),
            scroll_offset: 0,
            current_tool_name: None,
            should_quit: false,
        }
    }

    /// Get the currently selected tool.
    pub fn selected_tool(&self) -> &ToolMetadata {
        &TOOLS[self.selected_index]
    }

    /// Move selection up in the menu.
    pub fn select_previous(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    /// Move selection down in the menu.
    pub fn select_next(&mut self) {
        if self.selected_index < TOOLS.len() - 1 {
            self.selected_index += 1;
        }
    }

    /// Start running a tool.
    pub fn start_tool(&mut self, tool_name: String) {
        self.mode = AppMode::Running;
        self.current_tool_name = Some(tool_name);
        self.output_lines.clear();
        self.scroll_offset = 0;
    }

    /// Mark the current tool as completed.
    pub fn tool_completed(&mut self, exit_code: Option<i32>) {
        self.mode = AppMode::Completed;
        if let Some(code) = exit_code {
            self.output_lines
                .push(format!("\n[Process exited with code: {}]", code));
        } else {
            self.output_lines.push("\n[Process terminated]".to_string());
        }
    }

    /// Add output line from the running process.
    pub fn add_output(&mut self, line: String) {
        // Limit buffer to prevent memory growth
        const MAX_LINES: usize = 10000;
        const TRIM_AMOUNT: usize = 1000;
        if self.output_lines.len() >= MAX_LINES {
            self.output_lines.drain(0..TRIM_AMOUNT);
            // Adjust scroll offset to account for removed lines
            self.scroll_offset = self.scroll_offset.saturating_sub(TRIM_AMOUNT);
        }
        self.output_lines.push(line);
    }

    /// Return to menu mode.
    pub fn return_to_menu(&mut self) {
        self.mode = AppMode::Menu;
        self.current_tool_name = None;
        self.output_lines.clear();
        self.scroll_offset = 0;
    }

    /// Scroll output up.
    pub fn scroll_up(&mut self, amount: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    /// Scroll output down.
    pub fn scroll_down(&mut self, amount: usize, visible_height: usize) {
        let max_scroll = self.output_lines.len().saturating_sub(visible_height);
        self.scroll_offset = (self.scroll_offset + amount).min(max_scroll);
    }

    /// Check if we should auto-scroll to bottom.
    pub fn auto_scroll(&mut self, visible_height: usize) {
        let max_scroll = self.output_lines.len().saturating_sub(visible_height);
        self.scroll_offset = max_scroll;
    }

    /// Enter configure mode for the selected tool.
    pub fn enter_configure_mode(&mut self) {
        let form = tool_config::create_config_form(self.selected_index);
        self.form_state = Some(form);
        self.mode = AppMode::Configure;
    }

    /// Exit configure mode and return to menu.
    pub fn exit_configure_mode(&mut self) {
        self.form_state = None;
        self.mode = AppMode::Menu;
    }

    /// Get configured arguments from form state.
    pub fn get_configured_args(&self) -> Option<Vec<String>> {
        self.form_state.as_ref().map(tool_config::form_to_args)
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

/// Find the binary path for a tool.
/// Checks target/release/, target/debug/, and PATH.
pub fn find_binary(binary_name: &str) -> PathBuf {
    // Get the directory of the current executable
    if let Ok(exe_path) = env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let sibling = exe_dir.join(binary_name);
            if sibling.exists() {
                return sibling;
            }
        }
    }

    // Check target/release/
    let release_path = PathBuf::from(format!("target/release/{}", binary_name));
    if release_path.exists() {
        return release_path;
    }

    // Check target/debug/
    let debug_path = PathBuf::from(format!("target/debug/{}", binary_name));
    if debug_path.exists() {
        return debug_path;
    }

    // Assume it's in PATH
    PathBuf::from(binary_name)
}

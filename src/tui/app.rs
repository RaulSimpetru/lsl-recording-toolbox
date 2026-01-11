//! Application state management for the TUI launcher with multi-tab support.

use std::env;
use std::path::PathBuf;

use super::file_browser::FileBrowserState;
use super::tab::TabState;
use super::tool_config;

/// Category for grouping tools in the menu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToolCategory {
    Recording,
    Analysis,
    PostProcessing,
    Development,
}

impl ToolCategory {
    /// Display name for the category header.
    pub fn display_name(&self) -> &'static str {
        match self {
            ToolCategory::Recording => "Recording",
            ToolCategory::Analysis => "Analysis",
            ToolCategory::PostProcessing => "Post-Processing",
            ToolCategory::Development => "Development",
        }
    }
}

/// Metadata for a tool in the toolbox.
#[derive(Clone)]
pub struct ToolMetadata {
    /// Display name of the tool
    pub name: &'static str,
    /// Binary name (used for execution)
    pub binary: &'static str,
    /// Short description of what the tool does
    pub description: &'static str,
    /// Category for menu grouping
    pub category: ToolCategory,
}

/// All available tools in the toolbox, ordered by category.
pub const TOOLS: &[ToolMetadata] = &[
    // Recording
    ToolMetadata {
        name: "LSL Recorder",
        binary: "lsl-recorder",
        description: "Record a single LSL stream to Zarr format",
        category: ToolCategory::Recording,
    },
    ToolMetadata {
        name: "LSL Multi-Recorder",
        binary: "lsl-multi-recorder",
        description: "Record multiple LSL streams simultaneously",
        category: ToolCategory::Recording,
    },
    // Analysis
    ToolMetadata {
        name: "LSL Inspect",
        binary: "lsl-inspect",
        description: "Inspect Zarr recording contents and metadata",
        category: ToolCategory::Analysis,
    },
    ToolMetadata {
        name: "LSL Validate",
        binary: "lsl-validate",
        description: "Validate recording synchronization quality",
        category: ToolCategory::Analysis,
    },
    // Post-Processing
    ToolMetadata {
        name: "LSL Sync",
        binary: "lsl-sync",
        description: "Synchronize timestamps across streams",
        category: ToolCategory::PostProcessing,
    },
    // Development
    ToolMetadata {
        name: "LSL Replay",
        binary: "lsl-replay",
        description: "Replay recorded LSL streams",
        category: ToolCategory::Development,
    },
    ToolMetadata {
        name: "LSL Dummy Stream",
        binary: "lsl-dummy-stream",
        description: "Generate test LSL streams for development",
        category: ToolCategory::Development,
    },
];

/// State for close confirmation dialog.
pub struct CloseConfirmation {
    /// Index of tab being closed
    pub tab_index: usize,
}

/// State for tab rename dialog.
pub struct RenameState {
    /// Index of tab being renamed
    pub tab_index: usize,
    /// Current input buffer
    pub buffer: String,
    /// Cursor position in buffer
    pub cursor: usize,
}

/// Main application state with multi-tab support.
pub struct App {
    /// Currently selected tool index in the menu
    pub selected_index: usize,
    /// All open tabs
    pub tabs: Vec<TabState>,
    /// Index of the currently active tab (None = in menu)
    pub active_tab_index: Option<usize>,
    /// Confirmation dialog state
    pub close_confirmation: Option<CloseConfirmation>,
    /// File browser state (when browsing for a path)
    pub file_browser: Option<FileBrowserState>,
    /// Rename dialog state
    pub rename_state: Option<RenameState>,
    /// User preference: don't ask before closing tabs with running processes
    pub skip_close_confirmation: bool,
    /// Whether the application should quit
    pub should_quit: bool,
    /// Next tab ID (for unique identification)
    next_tab_id: usize,
}

impl App {
    /// Create a new application instance.
    pub fn new() -> Self {
        Self {
            selected_index: 0,
            tabs: Vec::new(),
            active_tab_index: None,
            close_confirmation: None,
            file_browser: None,
            rename_state: None,
            skip_close_confirmation: false,
            should_quit: false,
            next_tab_id: 0,
        }
    }

    /// Check if file browser is open.
    pub fn has_file_browser(&self) -> bool {
        self.file_browser.is_some()
    }

    /// Open file browser for a path field.
    pub fn open_file_browser(&mut self, current_value: &str, select_dir: bool, field_index: usize) {
        self.file_browser = Some(FileBrowserState::new(current_value, select_dir, field_index));
    }

    /// Close file browser without selecting.
    pub fn close_file_browser(&mut self) {
        self.file_browser = None;
    }

    /// Get the file browser mutably.
    pub fn file_browser_mut(&mut self) -> Option<&mut FileBrowserState> {
        self.file_browser.as_mut()
    }

    /// Check if rename dialog is open.
    pub fn is_renaming(&self) -> bool {
        self.rename_state.is_some()
    }

    /// Start renaming the active tab.
    pub fn start_rename(&mut self) {
        if let Some(tab_index) = self.active_tab_index {
            let current_title = self.tabs[tab_index].title.clone();
            self.rename_state = Some(RenameState {
                tab_index,
                buffer: current_title.clone(),
                cursor: current_title.len(),
            });
        }
    }

    /// Cancel rename and close dialog.
    pub fn cancel_rename(&mut self) {
        self.rename_state = None;
    }

    /// Confirm rename and apply new title.
    pub fn confirm_rename(&mut self) {
        if let Some(state) = self.rename_state.take() {
            let new_title = state.buffer.trim();
            if !new_title.is_empty() {
                self.tabs[state.tab_index].title = new_title.to_string();
            }
        }
    }

    /// Insert character in rename buffer.
    pub fn rename_insert(&mut self, c: char) {
        if let Some(ref mut state) = self.rename_state {
            if state.buffer.len() < 64 {
                state.buffer.insert(state.cursor, c);
                state.cursor += 1;
            }
        }
    }

    /// Backspace in rename buffer.
    pub fn rename_backspace(&mut self) {
        if let Some(ref mut state) = self.rename_state {
            if state.cursor > 0 {
                state.cursor -= 1;
                state.buffer.remove(state.cursor);
            }
        }
    }

    /// Delete in rename buffer.
    pub fn rename_delete(&mut self) {
        if let Some(ref mut state) = self.rename_state {
            if state.cursor < state.buffer.len() {
                state.buffer.remove(state.cursor);
            }
        }
    }

    /// Move rename cursor left.
    pub fn rename_cursor_left(&mut self) {
        if let Some(ref mut state) = self.rename_state {
            state.cursor = state.cursor.saturating_sub(1);
        }
    }

    /// Move rename cursor right.
    pub fn rename_cursor_right(&mut self) {
        if let Some(ref mut state) = self.rename_state {
            if state.cursor < state.buffer.len() {
                state.cursor += 1;
            }
        }
    }

    /// Move rename cursor to start.
    pub fn rename_cursor_home(&mut self) {
        if let Some(ref mut state) = self.rename_state {
            state.cursor = 0;
        }
    }

    /// Move rename cursor to end.
    pub fn rename_cursor_end(&mut self) {
        if let Some(ref mut state) = self.rename_state {
            state.cursor = state.buffer.len();
        }
    }

    /// Get the currently selected tool in the menu.
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

    /// Check if we're in menu mode (no active tab).
    pub fn is_in_menu(&self) -> bool {
        self.active_tab_index.is_none()
    }

    /// Get the currently active tab.
    pub fn active_tab(&self) -> Option<&TabState> {
        self.active_tab_index.and_then(|idx| self.tabs.get(idx))
    }

    /// Get the currently active tab mutably.
    pub fn active_tab_mut(&mut self) -> Option<&mut TabState> {
        self.active_tab_index.and_then(|idx| self.tabs.get_mut(idx))
    }

    /// Create a new tab for the selected tool and switch to it.
    pub fn create_tab_from_menu(&mut self) {
        let tool = self.selected_tool();
        let tool_index = self.selected_index;
        let form = tool_config::create_config_form(tool_index);

        let tab = TabState::new(self.next_tab_id, tool_index, tool.name, form);
        self.next_tab_id += 1;

        self.tabs.push(tab);
        self.active_tab_index = Some(self.tabs.len() - 1);
    }

    /// Switch to next tab or menu (Tab key).
    /// Cycles: Menu → Tab1 → Tab2 → ... → TabN → Menu
    pub fn next_tab(&mut self) {
        if self.tabs.is_empty() {
            // No tabs, stay in menu
            return;
        }
        self.active_tab_index = match self.active_tab_index {
            None => Some(0),                                      // Menu → first tab
            Some(idx) if idx + 1 < self.tabs.len() => Some(idx + 1), // Next tab
            Some(_) => None,                                      // Last tab → Menu
        };
    }

    /// Switch to previous tab or menu (Shift+Tab).
    /// Cycles: Menu → TabN → ... → Tab2 → Tab1 → Menu
    pub fn prev_tab(&mut self) {
        if self.tabs.is_empty() {
            // No tabs, stay in menu
            return;
        }
        self.active_tab_index = match self.active_tab_index {
            None => Some(self.tabs.len() - 1),  // Menu → last tab
            Some(0) => None,                     // First tab → Menu
            Some(idx) => Some(idx - 1),          // Previous tab
        };
    }

    /// Return to menu view (for future use).
    #[allow(dead_code)]
    pub fn return_to_menu(&mut self) {
        self.active_tab_index = None;
        self.close_confirmation = None;
    }

    /// Request to close the active tab (may trigger confirmation dialog).
    pub fn request_close_active_tab(&mut self) {
        let Some(idx) = self.active_tab_index else { return };
        let Some(tab) = self.tabs.get(idx) else { return };

        if tab.is_running() && !self.skip_close_confirmation {
            self.close_confirmation = Some(CloseConfirmation { tab_index: idx });
        } else {
            self.close_tab(idx);
        }
    }

    /// Close a tab by index (kills process if running).
    pub fn close_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.tabs[index].kill_process();
            self.tabs.remove(index);
            self.close_confirmation = None;

            if self.tabs.is_empty() {
                self.active_tab_index = None;
            } else if let Some(active) = self.active_tab_index {
                if index < active {
                    // Closed tab was before active - decrement to maintain same tab
                    self.active_tab_index = Some(active - 1);
                } else if index == active && active >= self.tabs.len() {
                    // Closed the active tab and it was the last one
                    self.active_tab_index = Some(self.tabs.len() - 1);
                }
                // If index > active: no adjustment needed
                // If index == active && active < tabs.len(): next tab takes over (standard behavior)
            }
        }
    }

    /// Confirm close from dialog.
    pub fn confirm_close(&mut self) {
        if let Some(conf) = self.close_confirmation.take() {
            self.close_tab(conf.tab_index);
        }
    }

    /// Confirm close and set "don't ask again" preference.
    pub fn confirm_close_dont_ask(&mut self) {
        self.skip_close_confirmation = true;
        self.confirm_close();
    }

    /// Cancel close dialog.
    pub fn cancel_close(&mut self) {
        self.close_confirmation = None;
    }

    /// Check if confirmation dialog is showing.
    pub fn has_confirmation_dialog(&self) -> bool {
        self.close_confirmation.is_some()
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

/// Find the binary path for a tool.
/// Checks next to current executable, target/release/, target/debug/, and PATH.
pub fn find_binary(binary_name: &str) -> PathBuf {
    // Get the directory of the current executable
    if let Ok(exe_path) = env::current_exe()
        && let Some(exe_dir) = exe_path.parent()
    {
        let sibling = exe_dir.join(binary_name);
        if sibling.exists() {
            return sibling;
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

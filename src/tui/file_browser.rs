//! File browser state and logic for path selection.

use std::fs;
use std::path::PathBuf;

/// Entry in the file browser listing.
#[derive(Clone, Debug)]
pub struct BrowserEntry {
    /// Display name
    pub name: String,
    /// Full path
    pub path: PathBuf,
    /// Whether this is a directory
    pub is_dir: bool,
}

/// State for the file browser modal.
pub struct FileBrowserState {
    /// Current directory being browsed
    pub current_dir: PathBuf,
    /// List of entries in current directory
    pub entries: Vec<BrowserEntry>,
    /// Currently selected index
    pub selected_index: usize,
    /// Whether we're selecting directories (true) or files (false)
    pub select_dir: bool,
    /// Scroll offset for long lists
    pub scroll_offset: usize,
    /// Error message if directory couldn't be read
    pub error: Option<String>,
    /// Index of the form field we're selecting for
    pub field_index: usize,
}

impl FileBrowserState {
    /// Create a new file browser starting at the given path.
    pub fn new(start_path: &str, select_dir: bool, field_index: usize) -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

        let start = if start_path.is_empty() {
            cwd
        } else {
            let p = PathBuf::from(start_path);
            if p.is_dir() {
                p
            } else {
                // Try parent directory, but fallback to cwd if parent is empty or doesn't exist
                p.parent()
                    .filter(|parent| !parent.as_os_str().is_empty() && parent.is_dir())
                    .map(|p| p.to_path_buf())
                    .unwrap_or(cwd)
            }
        };

        let mut browser = Self {
            current_dir: start,
            entries: Vec::new(),
            selected_index: 0,
            select_dir,
            scroll_offset: 0,
            error: None,
            field_index,
        };
        browser.refresh();
        browser
    }

    /// Refresh the directory listing.
    pub fn refresh(&mut self) {
        self.entries.clear();
        self.error = None;

        // Add parent directory entry if not at root
        if let Some(parent) = self.current_dir.parent() {
            self.entries.push(BrowserEntry {
                name: "..".to_string(),
                path: parent.to_path_buf(),
                is_dir: true,
            });
        }

        // Read directory contents
        match fs::read_dir(&self.current_dir) {
            Ok(entries) => {
                let mut dirs: Vec<BrowserEntry> = Vec::new();
                let mut files: Vec<BrowserEntry> = Vec::new();

                for entry in entries.flatten() {
                    let path = entry.path();
                    let name = entry.file_name().to_string_lossy().to_string();

                    // Skip hidden files on Unix
                    if name.starts_with('.') {
                        continue;
                    }

                    let is_dir = path.is_dir();
                    let entry = BrowserEntry { name, path, is_dir };

                    if is_dir {
                        dirs.push(entry);
                    } else {
                        files.push(entry);
                    }
                }

                // Sort alphabetically (case-insensitive)
                dirs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

                // Directories first, then files
                self.entries.extend(dirs);
                if !self.select_dir {
                    self.entries.extend(files);
                }
            }
            Err(e) => {
                self.error = Some(format!("Cannot read directory: {}", e));
            }
        }

        // Reset selection
        self.selected_index = 0;
        self.scroll_offset = 0;
    }

    /// Get the currently selected entry.
    pub fn selected_entry(&self) -> Option<&BrowserEntry> {
        self.entries.get(self.selected_index)
    }

    /// Move selection up.
    pub fn select_previous(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            self.ensure_visible();
        }
    }

    /// Move selection down.
    pub fn select_next(&mut self) {
        if self.selected_index + 1 < self.entries.len() {
            self.selected_index += 1;
            self.ensure_visible();
        }
    }

    /// Page up.
    pub fn page_up(&mut self, page_size: usize) {
        self.selected_index = self.selected_index.saturating_sub(page_size);
        self.ensure_visible();
    }

    /// Page down.
    pub fn page_down(&mut self, page_size: usize) {
        self.selected_index = (self.selected_index + page_size).min(self.entries.len().saturating_sub(1));
        self.ensure_visible();
    }

    /// Ensure the selected item is visible.
    fn ensure_visible(&mut self) {
        // This will be properly calculated during rendering
        // For now, just keep scroll_offset in reasonable bounds
        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        }
    }

    /// Update scroll offset based on visible height.
    pub fn update_scroll(&mut self, visible_height: usize) {
        if visible_height == 0 {
            return;
        }

        // Ensure selected item is visible
        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        } else if self.selected_index >= self.scroll_offset + visible_height {
            self.scroll_offset = self.selected_index - visible_height + 1;
        }
    }

    /// Navigate into the selected directory or go up.
    pub fn enter_selected(&mut self) -> Option<PathBuf> {
        if let Some(entry) = self.selected_entry() {
            if entry.is_dir {
                // Navigate into directory
                self.current_dir = entry.path.clone();
                self.refresh();
                None
            } else {
                // Return the selected file
                Some(entry.path.clone())
            }
        } else {
            None
        }
    }

    /// Select the current directory (for directory selection mode).
    pub fn select_current_dir(&self) -> PathBuf {
        self.current_dir.clone()
    }

    /// Go up one directory level.
    pub fn go_up(&mut self) {
        if let Some(parent) = self.current_dir.parent() {
            self.current_dir = parent.to_path_buf();
            self.refresh();
        }
    }

    /// Get the path to display/return.
    pub fn get_selected_path(&self) -> Option<String> {
        if self.select_dir {
            // For directory selection, return current directory
            Some(self.current_dir.to_string_lossy().to_string())
        } else {
            // For file selection, return selected file
            self.selected_entry()
                .filter(|e| !e.is_dir)
                .map(|e| e.path.to_string_lossy().to_string())
        }
    }
}

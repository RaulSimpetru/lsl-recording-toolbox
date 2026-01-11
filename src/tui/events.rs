//! Event handling for the TUI.

use std::time::Duration;

use anyhow::Result;
use crossterm::event::{
    self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};

/// Application events.
#[derive(Debug)]
pub enum Event {
    /// A key was pressed
    Key(KeyEvent),
    /// Timer tick for UI refresh
    Tick,
    /// Terminal was resized
    #[allow(dead_code)]
    Resize(u16, u16),
}

/// Event handler that polls for terminal events.
pub struct EventHandler {
    tick_rate: Duration,
}

impl EventHandler {
    /// Create a new event handler.
    pub fn new(tick_rate_ms: u64) -> Self {
        Self {
            tick_rate: Duration::from_millis(tick_rate_ms),
        }
    }

    /// Poll for the next event.
    pub fn next(&self) -> Result<Event> {
        if event::poll(self.tick_rate)? {
            match event::read()? {
                // Only handle key press events, ignore repeats and releases
                CrosstermEvent::Key(key) if key.kind == KeyEventKind::Press => {
                    Ok(Event::Key(key))
                }
                CrosstermEvent::Resize(w, h) => Ok(Event::Resize(w, h)),
                _ => Ok(Event::Tick),
            }
        } else {
            Ok(Event::Tick)
        }
    }
}

impl Default for EventHandler {
    fn default() -> Self {
        Self::new(100) // 100ms tick rate
    }
}

/// Check if a key event is Ctrl+C.
pub fn is_ctrl_c(key: &KeyEvent) -> bool {
    key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL)
}

/// Check if a key event is Escape.
pub fn is_esc(key: &KeyEvent) -> bool {
    key.code == KeyCode::Esc
}

/// Check if a key event is the enter key.
pub fn is_enter(key: &KeyEvent) -> bool {
    key.code == KeyCode::Enter
}

/// Check if a key event is Ctrl+Enter.
pub fn is_ctrl_enter(key: &KeyEvent) -> bool {
    key.code == KeyCode::Enter && key.modifiers.contains(KeyModifiers::CONTROL)
}

/// Check if a key event is the up arrow.
pub fn is_up(key: &KeyEvent) -> bool {
    key.code == KeyCode::Up
}

/// Check if a key event is the down arrow.
pub fn is_down(key: &KeyEvent) -> bool {
    key.code == KeyCode::Down
}

/// Check if a key event is page up.
pub fn is_page_up(key: &KeyEvent) -> bool {
    key.code == KeyCode::PageUp
}

/// Check if a key event is page down.
pub fn is_page_down(key: &KeyEvent) -> bool {
    key.code == KeyCode::PageDown
}

/// Check if a key event is Tab.
pub fn is_tab(key: &KeyEvent) -> bool {
    key.code == KeyCode::Tab && !key.modifiers.contains(KeyModifiers::SHIFT)
}

/// Check if a key event is Shift+Tab (back tab).
pub fn is_shift_tab(key: &KeyEvent) -> bool {
    key.code == KeyCode::BackTab
        || (key.code == KeyCode::Tab && key.modifiers.contains(KeyModifiers::SHIFT))
}

/// Check if a key event is Backspace.
pub fn is_backspace(key: &KeyEvent) -> bool {
    key.code == KeyCode::Backspace
}

/// Check if a key event is Delete.
pub fn is_delete(key: &KeyEvent) -> bool {
    key.code == KeyCode::Delete
}

/// Check if a key event is Left arrow.
pub fn is_left(key: &KeyEvent) -> bool {
    key.code == KeyCode::Left
}

/// Check if a key event is Right arrow.
pub fn is_right(key: &KeyEvent) -> bool {
    key.code == KeyCode::Right
}

/// Check if a key event is Home.
pub fn is_home(key: &KeyEvent) -> bool {
    key.code == KeyCode::Home
}

/// Check if a key event is End.
pub fn is_end(key: &KeyEvent) -> bool {
    key.code == KeyCode::End
}

/// Check if a key event is Space.
pub fn is_space(key: &KeyEvent) -> bool {
    key.code == KeyCode::Char(' ')
}

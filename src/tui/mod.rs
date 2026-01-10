//! TUI launcher module for the LSL Recording Toolbox
//!
//! This module provides a terminal user interface for selecting and running
//! the various LSL tools in the toolbox. Supports multiple concurrent tools
//! running in separate tabs.

pub mod app;
pub mod events;
pub mod form;
pub mod process;
pub mod tab;
pub mod tool_config;
pub mod ui;
pub mod ui_dialog;
pub mod ui_form;
pub mod ui_tabs;

pub use app::App;

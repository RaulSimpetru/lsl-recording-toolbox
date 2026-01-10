//! Form field and state management for tool configuration.

/// Type of form field, determining input behavior and rendering.
#[derive(Clone, Debug, PartialEq)]
pub enum FieldType {
    /// Free-form text input
    Text,
    /// Integer number (decimals truncated)
    Integer,
    /// Floating point number
    Float,
    /// Boolean toggle (Space/Enter to flip)
    Bool,
    /// Selection from a list of options (Left/Right to cycle)
    Select(Vec<String>),
    /// File or directory path (Space/Enter to open browser)
    Path {
        /// If true, select directories; if false, select files
        select_dir: bool,
    },
}

/// A single form field with label, value, and metadata.
#[derive(Clone)]
#[allow(dead_code)]
pub struct FormField {
    /// Internal name used for argument mapping
    pub name: String,
    /// Display label shown to user
    pub label: String,
    /// Current value
    pub value: String,
    /// Default value (shown as hint)
    pub default: String,
    /// Whether this field is required
    pub required: bool,
    /// Hint text describing expected format
    pub hint: String,
    /// Current cursor position within value
    pub cursor_pos: usize,
    /// Field type determining input behavior
    pub field_type: FieldType,
    /// Current selection index (for Select type)
    pub select_idx: usize,
}

#[allow(dead_code)]
impl FormField {
    /// Create a new text form field.
    pub fn new(name: &str, label: &str, default: &str, required: bool, hint: &str) -> Self {
        let value = default.to_string();
        let cursor_pos = value.len();
        Self {
            name: name.to_string(),
            label: label.to_string(),
            value,
            default: default.to_string(),
            required,
            hint: hint.to_string(),
            cursor_pos,
            field_type: FieldType::Text,
            select_idx: 0,
        }
    }

    /// Create a required text field.
    pub fn required(name: &str, label: &str, default: &str, hint: &str) -> Self {
        Self::new(name, label, default, true, hint)
    }

    /// Create an optional text field.
    pub fn optional(name: &str, label: &str, default: &str, hint: &str) -> Self {
        Self::new(name, label, default, false, hint)
    }

    /// Create a boolean toggle field.
    pub fn bool_field(name: &str, label: &str, default: bool) -> Self {
        Self {
            name: name.to_string(),
            label: label.to_string(),
            value: if default { "true" } else { "false" }.to_string(),
            default: if default { "true" } else { "false" }.to_string(),
            required: false,
            hint: "Space to toggle".to_string(),
            cursor_pos: 0,
            field_type: FieldType::Bool,
            select_idx: 0,
        }
    }

    /// Create an integer field.
    pub fn int_field(name: &str, label: &str, default: i64, required: bool, hint: &str) -> Self {
        let value = default.to_string();
        let cursor_pos = value.len();
        Self {
            name: name.to_string(),
            label: label.to_string(),
            value,
            default: default.to_string(),
            required,
            hint: hint.to_string(),
            cursor_pos,
            field_type: FieldType::Integer,
            select_idx: 0,
        }
    }

    /// Create a float field.
    pub fn float_field(name: &str, label: &str, default: f64, required: bool, hint: &str) -> Self {
        let value = default.to_string();
        let cursor_pos = value.len();
        Self {
            name: name.to_string(),
            label: label.to_string(),
            value,
            default: default.to_string(),
            required,
            hint: hint.to_string(),
            cursor_pos,
            field_type: FieldType::Float,
            select_idx: 0,
        }
    }

    /// Create a selection field with options.
    pub fn select_field(name: &str, label: &str, options: &[&str], default_idx: usize) -> Self {
        let options_vec: Vec<String> = options.iter().map(|s| s.to_string()).collect();
        let default_idx = default_idx.min(options_vec.len().saturating_sub(1));
        let value = options_vec.get(default_idx).cloned().unwrap_or_default();
        Self {
            name: name.to_string(),
            label: label.to_string(),
            value: value.clone(),
            default: value,
            required: false,
            hint: "←/→ to change".to_string(),
            cursor_pos: 0,
            field_type: FieldType::Select(options_vec),
            select_idx: default_idx,
        }
    }

    /// Create a file path field (opens file browser on Space/Enter).
    pub fn file_path(name: &str, label: &str, default: &str, required: bool, hint: &str) -> Self {
        let value = default.to_string();
        let cursor_pos = value.len();
        Self {
            name: name.to_string(),
            label: label.to_string(),
            value,
            default: default.to_string(),
            required,
            hint: hint.to_string(),
            cursor_pos,
            field_type: FieldType::Path { select_dir: false },
            select_idx: 0,
        }
    }

    /// Create a directory path field (opens directory browser on Space/Enter).
    pub fn dir_path(name: &str, label: &str, default: &str, required: bool, hint: &str) -> Self {
        let value = default.to_string();
        let cursor_pos = value.len();
        Self {
            name: name.to_string(),
            label: label.to_string(),
            value,
            default: default.to_string(),
            required,
            hint: hint.to_string(),
            cursor_pos,
            field_type: FieldType::Path { select_dir: true },
            select_idx: 0,
        }
    }

    /// Toggle boolean value.
    pub fn toggle_bool(&mut self) {
        if self.field_type == FieldType::Bool {
            self.value = if self.value == "true" {
                "false".to_string()
            } else {
                "true".to_string()
            };
        }
    }

    /// Cycle to next option (for Select fields).
    pub fn next_option(&mut self) {
        if let FieldType::Select(ref options) = self.field_type {
            if !options.is_empty() {
                self.select_idx = (self.select_idx + 1) % options.len();
                self.value = options[self.select_idx].clone();
            }
        }
    }

    /// Cycle to previous option (for Select fields).
    pub fn prev_option(&mut self) {
        if let FieldType::Select(ref options) = self.field_type {
            if !options.is_empty() {
                if self.select_idx == 0 {
                    self.select_idx = options.len() - 1;
                } else {
                    self.select_idx -= 1;
                }
                self.value = options[self.select_idx].clone();
            }
        }
    }

    /// Normalize the value based on field type.
    /// For integers, truncates decimals.
    pub fn normalize_value(&mut self) {
        match self.field_type {
            FieldType::Integer => {
                // Parse as float first, then truncate to int
                if let Ok(f) = self.value.parse::<f64>() {
                    let i = f as i64;
                    self.value = i.to_string();
                    self.cursor_pos = self.value.len();
                }
            }
            FieldType::Float => {
                // Just ensure it's a valid float
                if let Ok(f) = self.value.parse::<f64>() {
                    // Keep as-is if valid, but remove trailing garbage
                    self.value = f.to_string();
                    self.cursor_pos = self.value.len();
                }
            }
            _ => {}
        }
    }

    /// Check if field has a value (either user-entered or default).
    pub fn has_value(&self) -> bool {
        !self.value.trim().is_empty()
    }

    /// Check if field is valid (has value if required).
    pub fn is_valid(&self) -> bool {
        !self.required || self.has_value()
    }

    /// Check if this field accepts text input.
    pub fn accepts_text_input(&self) -> bool {
        matches!(
            self.field_type,
            FieldType::Text | FieldType::Integer | FieldType::Float | FieldType::Path { .. }
        )
    }

    /// Check if this field is a path field that can open a browser.
    pub fn is_path_field(&self) -> bool {
        matches!(self.field_type, FieldType::Path { .. })
    }

    /// Check if this path field selects directories (vs files).
    pub fn selects_directory(&self) -> bool {
        matches!(self.field_type, FieldType::Path { select_dir: true })
    }
}

/// State for a configuration form with multiple fields.
pub struct FormState {
    /// All fields in the form
    pub fields: Vec<FormField>,
    /// Index of currently active/focused field (fields.len() = Run button)
    pub active_field_idx: usize,
    /// Name of the tool being configured
    pub tool_name: String,
    /// Validation error message (if any)
    pub error_message: Option<String>,
    /// Scroll offset for long forms
    pub scroll_offset: usize,
}

impl FormState {
    /// Check if the Run button is currently focused.
    pub fn is_run_button_focused(&self) -> bool {
        self.active_field_idx >= self.fields.len()
    }
}

#[allow(dead_code)]
impl FormState {
    /// Create a new form state.
    pub fn new(tool_name: &str, fields: Vec<FormField>) -> Self {
        Self {
            fields,
            active_field_idx: 0,
            tool_name: tool_name.to_string(),
            error_message: None,
            scroll_offset: 0,
        }
    }

    /// Get the currently active field.
    pub fn active_field(&self) -> Option<&FormField> {
        self.fields.get(self.active_field_idx)
    }

    /// Get the currently active field mutably.
    pub fn active_field_mut(&mut self) -> Option<&mut FormField> {
        self.fields.get_mut(self.active_field_idx)
    }

    /// Insert a character at the cursor position in the active field.
    /// Only works for text-input fields.
    pub fn insert_char(&mut self, c: char) {
        if let Some(field) = self.active_field_mut() {
            if !field.accepts_text_input() {
                return;
            }
            // For numeric fields, only allow valid characters
            match field.field_type {
                FieldType::Integer => {
                    if !c.is_ascii_digit() && c != '-' {
                        return;
                    }
                }
                FieldType::Float => {
                    if !c.is_ascii_digit() && c != '-' && c != '.' {
                        return;
                    }
                }
                _ => {}
            }
            // Limit field length to prevent memory issues
            if field.value.len() < 256 {
                field.value.insert(field.cursor_pos, c);
                field.cursor_pos += 1;
            }
        }
        self.error_message = None;
    }

    /// Delete character at cursor position (Delete key).
    pub fn delete_char(&mut self) {
        if let Some(field) = self.active_field_mut() {
            if !field.accepts_text_input() {
                return;
            }
            if field.cursor_pos < field.value.len() {
                field.value.remove(field.cursor_pos);
            }
        }
        self.error_message = None;
    }

    /// Delete character before cursor position (Backspace key).
    pub fn backspace(&mut self) {
        if let Some(field) = self.active_field_mut() {
            if !field.accepts_text_input() {
                return;
            }
            if field.cursor_pos > 0 {
                field.cursor_pos -= 1;
                field.value.remove(field.cursor_pos);
            }
        }
        self.error_message = None;
    }

    /// Move cursor left within the active field.
    pub fn move_cursor_left(&mut self) {
        if let Some(field) = self.active_field_mut() {
            match &field.field_type {
                FieldType::Select(_) => field.prev_option(),
                FieldType::Bool => field.toggle_bool(),
                _ => field.cursor_pos = field.cursor_pos.saturating_sub(1),
            }
        }
    }

    /// Move cursor right within the active field.
    pub fn move_cursor_right(&mut self) {
        if let Some(field) = self.active_field_mut() {
            match &field.field_type {
                FieldType::Select(_) => field.next_option(),
                FieldType::Bool => field.toggle_bool(),
                _ => {
                    if field.cursor_pos < field.value.len() {
                        field.cursor_pos += 1;
                    }
                }
            }
        }
    }

    /// Toggle boolean or cycle option for the active field.
    pub fn toggle_or_cycle(&mut self) {
        if let Some(field) = self.active_field_mut() {
            match &field.field_type {
                FieldType::Bool => field.toggle_bool(),
                FieldType::Select(_) => field.next_option(),
                _ => {}
            }
        }
        self.error_message = None;
    }

    /// Move cursor to start of field.
    pub fn move_cursor_home(&mut self) {
        if let Some(field) = self.active_field_mut() {
            field.cursor_pos = 0;
        }
    }

    /// Move cursor to end of field.
    pub fn move_cursor_end(&mut self) {
        if let Some(field) = self.active_field_mut() {
            field.cursor_pos = field.value.len();
        }
    }

    /// Move to next field (Tab). Includes Run button at the end.
    pub fn next_field(&mut self) {
        // Normalize current field before leaving
        if let Some(field) = self.active_field_mut() {
            field.normalize_value();
        }
        if !self.fields.is_empty() {
            // Cycle through fields + 1 for Run button
            let total = self.fields.len() + 1;
            self.active_field_idx = (self.active_field_idx + 1) % total;
            // Move cursor to end of new field
            if let Some(field) = self.active_field_mut() {
                field.cursor_pos = field.value.len();
            }
        }
    }

    /// Move to previous field (Shift+Tab). Includes Run button at the end.
    pub fn prev_field(&mut self) {
        // Normalize current field before leaving
        if let Some(field) = self.active_field_mut() {
            field.normalize_value();
        }
        if !self.fields.is_empty() {
            // Cycle through fields + 1 for Run button
            let total = self.fields.len() + 1;
            if self.active_field_idx == 0 {
                self.active_field_idx = total - 1; // Go to Run button
            } else {
                self.active_field_idx -= 1;
            }
            // Move cursor to end of new field
            if let Some(field) = self.active_field_mut() {
                field.cursor_pos = field.value.len();
            }
        }
    }

    /// Get field value by name.
    pub fn get_value(&self, name: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|f| f.name == name)
            .map(|f| f.value.as_str())
    }

    /// Validate all required fields.
    pub fn validate(&mut self) -> Result<(), String> {
        let missing: Vec<&str> = self
            .fields
            .iter()
            .filter(|f| !f.is_valid())
            .map(|f| f.label.as_str())
            .collect();

        if missing.is_empty() {
            self.error_message = None;
            Ok(())
        } else {
            let msg = format!("Required fields missing: {}", missing.join(", "));
            self.error_message = Some(msg.clone());
            Err(msg)
        }
    }

    /// Clear the current field value.
    pub fn clear_field(&mut self) {
        if let Some(field) = self.active_field_mut() {
            field.value.clear();
            field.cursor_pos = 0;
        }
        self.error_message = None;
    }

    /// Number of fields in the form.
    pub fn field_count(&self) -> usize {
        self.fields.len()
    }
}

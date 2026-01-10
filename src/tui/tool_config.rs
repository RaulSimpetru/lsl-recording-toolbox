//! Tool configuration schemas and argument conversion.

use super::form::{FormField, FormState};

/// Create configuration form for a tool by index.
pub fn create_config_form(tool_idx: usize) -> FormState {
    match tool_idx {
        0 => create_recorder_form(),
        1 => create_multi_recorder_form(),
        2 => create_inspect_form(),
        3 => create_validate_form(),
        4 => create_sync_form(),
        5 => create_replay_form(),
        6 => create_dummy_stream_form(),
        _ => create_recorder_form(), // fallback
    }
}

/// Convert form state to command-line arguments.
pub fn form_to_args(form: &FormState) -> Vec<String> {
    let mut args = Vec::new();
    let mut positional_arg: Option<String> = None;

    for field in &form.fields {
        let value = field.value.trim();
        if value.is_empty() {
            continue;
        }

        // Handle special cases
        match field.name.as_str() {
            // Boolean flags - only add if "true" or similar
            "interactive" | "quiet" | "verbose" | "immediate_flush" | "memory_monitor" |
            "list" | "trim_start" | "trim_end" | "trim_both" => {
                if is_truthy(value) {
                    args.push(format!("--{}", field.name.replace('_', "-")));
                }
            }
            // Boolean flags - only add if true
            "noise" | "loop" => {
                if is_truthy(value) {
                    args.push(format!("--{}", field.name));
                }
            }
            // lsl-dummy-stream --type (field name is stream_type)
            "stream_type" => {
                args.push("--type".to_string());
                args.push(value.to_string());
            }
            // Multi-value fields - lsl-multi-recorder expects space-separated after single flag
            "source_ids" => {
                args.push("--source-ids".to_string());
                for v in value.split(',') {
                    let v = v.trim();
                    if !v.is_empty() {
                        args.push(v.to_string());
                    }
                }
            }
            "stream_names" => {
                args.push("--stream-names".to_string());
                for v in value.split(',') {
                    let v = v.trim();
                    if !v.is_empty() {
                        args.push(v.to_string());
                    }
                }
            }
            // lsl-sync --stream (can be repeated)
            "streams" => {
                for v in value.split(',') {
                    let v = v.trim();
                    if !v.is_empty() {
                        args.push("--stream".to_string());
                        args.push(v.to_string());
                    }
                }
            }
            // Positional argument (file path)
            "file_path" | "zarr_file" => {
                positional_arg = Some(value.to_string());
            }
            // Regular named arguments
            _ => {
                args.push(format!("--{}", field.name.replace('_', "-")));
                args.push(value.to_string());
            }
        }
    }

    // Prepend positional argument at the beginning
    if let Some(pos) = positional_arg {
        args.insert(0, pos);
    }

    args
}

/// Build command preview string for display.
pub fn build_command_preview(binary_name: &str, form: &FormState) -> String {
    let args = form_to_args(form);
    if args.is_empty() {
        binary_name.to_string()
    } else {
        format!("{} {}", binary_name, args.join(" "))
    }
}

/// Check if a string value represents "true".
fn is_truthy(value: &str) -> bool {
    matches!(
        value.to_lowercase().as_str(),
        "true" | "yes" | "y" | "1" | "on"
    )
}

// =============================================================================
// Tool-specific form builders
// =============================================================================

fn create_recorder_form() -> FormState {
    FormState::new("LSL Recorder", vec![
        // Required
        FormField::required("source_id", "Source ID *", "1234", "LSL stream source ID"),
        FormField::dir_path("output", "Output Path *", "experiment", true, "Space to browse"),
        // Metadata
        FormField::optional("stream_name", "Stream Name", "", "Name in Zarr (defaults to source ID)"),
        FormField::optional("subject", "Subject", "", "Subject identifier"),
        FormField::optional("session_id", "Session ID", "", "Session identifier"),
        FormField::optional("notes", "Notes", "", "Recording notes"),
        // Recording options
        FormField::float_field("duration", "Duration (s)", 0.0, false, "Max recording duration (0=unlimited)"),
        FormField::float_field("resolve_timeout", "Resolve Timeout", 5.0, false, "Stream resolution timeout (s)"),
        // Buffering
        FormField::float_field("flush_interval", "Flush Interval", 1.0, false, "Flush interval (seconds)"),
        FormField::int_field("flush_buffer_size", "Flush Buffer Size", 50, false, "Samples before flush"),
        FormField::int_field("buffer_size", "Stream Buffer", 1000, false, "LSL buffer size"),
        // Flags
        FormField::bool_field("interactive", "Interactive", false),
        FormField::bool_field("quiet", "Quiet Mode", false),
        FormField::bool_field("immediate_flush", "Immediate Flush", false),
        FormField::bool_field("memory_monitor", "Memory Monitor", false),
    ])
}

fn create_multi_recorder_form() -> FormState {
    FormState::new("LSL Multi-Recorder", vec![
        // Required
        FormField::required("source_ids", "Source IDs *", "", "Comma-separated source IDs"),
        FormField::dir_path("output", "Output Path *", "experiment", true, "Space to browse"),
        // Metadata
        FormField::optional("stream_names", "Stream Names", "", "Comma-separated names (optional)"),
        FormField::optional("subject", "Subject", "", "Subject identifier"),
        FormField::optional("session_id", "Session ID", "", "Session identifier"),
        FormField::optional("notes", "Notes", "", "Recording notes"),
        // Recording options
        FormField::float_field("duration", "Duration (s)", 0.0, false, "Max recording duration (0=unlimited)"),
        FormField::float_field("resolve_timeout", "Resolve Timeout", 5.0, false, "Stream resolution timeout (s)"),
        // Buffering
        FormField::float_field("flush_interval", "Flush Interval", 1.0, false, "Flush interval (seconds)"),
        FormField::int_field("flush_buffer_size", "Flush Buffer Size", 50, false, "Samples before flush"),
        // Flags
        FormField::bool_field("quiet", "Quiet Mode", false),
        FormField::bool_field("immediate_flush", "Immediate Flush", false),
    ])
}

fn create_inspect_form() -> FormState {
    FormState::new("LSL Inspect", vec![
        FormField::file_path("file_path", "Zarr File *", "", true, "Space to browse"),
        FormField::optional("stream", "Stream Filter", "", "Filter to specific stream"),
        FormField::bool_field("verbose", "Verbose", false),
    ])
}

fn create_validate_form() -> FormState {
    FormState::new("LSL Validate", vec![
        FormField::file_path("file_path", "Zarr File *", "", true, "Space to browse"),
    ])
}

fn create_sync_form() -> FormState {
    FormState::new("LSL Sync", vec![
        FormField::file_path("zarr_file", "Zarr File *", "", true, "Space to browse"),
        FormField::select_field("mode", "Sync Mode", &["common-start", "first-stream", "last-stream", "absolute-zero"], 0),
        FormField::optional("streams", "Stream Filter", "", "Comma-separated streams to process"),
        FormField::bool_field("trim_start", "Trim Start", false),
        FormField::bool_field("trim_end", "Trim End", false),
        FormField::bool_field("trim_both", "Trim Both", false),
        FormField::bool_field("verbose", "Verbose", false),
    ])
}

fn create_replay_form() -> FormState {
    FormState::new("LSL Replay", vec![
        FormField::file_path("file_path", "Zarr File *", "", true, "Space to browse"),
        FormField::required("stream", "Stream Name *", "", "Stream to replay"),
        FormField::optional("output_name", "Output Name", "", "Custom output stream name"),
        FormField::float_field("speed", "Speed", 1.0, false, "Playback speed (1.0 = real-time)"),
        FormField::bool_field("loop", "Loop", true),
        FormField::bool_field("list", "List Streams", false),
        FormField::bool_field("verbose", "Verbose", false),
    ])
}

fn create_dummy_stream_form() -> FormState {
    FormState::new("LSL Dummy Stream", vec![
        // Stream identity
        FormField::optional("name", "Stream Name", "TestStream", "Name of the stream"),
        FormField::optional("stream_type", "Stream Type", "EMG", "Type (EMG, EEG, etc.)"),
        FormField::optional("source_id", "Source ID", "TEST_1234", "Unique source identifier"),
        // Signal parameters
        FormField::int_field("channels", "Channels", 100, false, "Number of channels"),
        FormField::int_field("sample_rate", "Sample Rate", 10000, false, "Sampling rate (Hz)"),
        FormField::int_field("chunk_size", "Chunk Size", 18, false, "Samples per chunk"),
        FormField::optional("freq_range", "Freq Range", "1,10", "Frequency range (min,max)"),
        FormField::select_field("data_type", "Data Type", &["float32", "int16"], 0),
        // Flags
        FormField::bool_field("noise", "Noise Mode", false),
        FormField::bool_field("verbose", "Verbose", false),
    ])
}

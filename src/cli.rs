use clap::Parser;
use serde_json::json;
use std::path::PathBuf;

#[derive(Parser, Clone)]
#[command(name = "lsl-recorder")]
#[command(about = "Record LSL streams to disk with dedicated control interface")]
pub struct Args {
    #[arg(long, help = "LSL stream source ID to record", default_value = "1234")]
    pub source_id: String,

    #[arg(
        long,
        short = 'o',
        help = "Zarr experiment base path (without .zarr extension)",
        default_value = "experiment"
    )]
    pub output: PathBuf,

    #[arg(
        long,
        help = "Stream name for Zarr group (defaults to source-id if not specified)"
    )]
    pub stream_name: Option<String>,

    #[arg(
        long,
        help = "Optional suffix for Zarr store (defaults to stream name if not specified)"
    )]
    pub suffix: Option<String>,

    #[arg(
        long,
        short = 'i',
        help = "Interactive mode - accept commands via stdin"
    )]
    pub interactive: bool,

    #[arg(
        long,
        help = "Auto-start recording (default: true for non-interactive, false for interactive)"
    )]
    pub auto_start: Option<bool>,

    #[arg(long, short = 'd', help = "Maximum recording duration in seconds")]
    pub duration: Option<u64>,

    #[arg(long, default_value = "1000", help = "Stream buffer size")]
    pub buffer_size: usize,

    #[arg(long, short = 'q', help = "Minimal output mode")]
    pub quiet: bool,

    #[arg(
        long,
        default_value = "5.0",
        help = "Timeout for stream resolution in seconds"
    )]
    pub resolve_timeout: f64,

    #[arg(long, help = "Subject identifier for metadata")]
    pub subject: Option<String>,

    #[arg(long, help = "Session identifier for metadata")]
    pub session_id: Option<String>,

    #[arg(long, help = "Notes for metadata")]
    pub notes: Option<String>,

    #[arg(
        long,
        default_value = "1.0",
        help = "Flush data to disk interval in seconds"
    )]
    pub flush_interval: f64,

    #[arg(
        long,
        default_value = "50",
        help = "Buffer size before forcing flush (number of samples)"
    )]
    pub flush_buffer_size: usize,

    #[arg(
        long,
        help = "Flush immediately after every sample (maximum safety, lower performance)"
    )]
    pub immediate_flush: bool,

    #[arg(
        long,
        default_value = "3",
        help = "Maximum number of attempts to resolve LSL stream"
    )]
    pub lsl_max_retry_attempts: u32,

    #[arg(
        long,
        default_value = "50",
        help = "Base delay in milliseconds between LSL retry attempts"
    )]
    pub lsl_retry_base_delay_ms: u64,

    #[arg(
        long,
        help = "LSL pull timeout in seconds (auto-calculated from stream frequency if not set)"
    )]
    pub lsl_pull_timeout: Option<f64>,

    #[arg(long, help = "Enable memory usage monitoring and periodic reporting")]
    pub memory_monitor: bool,
}

impl Args {
    /// Get the Zarr configuration tuple from the parsed arguments
    /// Returns (store_path, stream_name, subject, session_id, notes)
    /// Note: Multiple streams can now write to the same Zarr file concurrently
    /// by using different stream_name values under /streams/{stream_name}/
    pub fn zarr_config(
        &self,
    ) -> (
        PathBuf,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    ) {
        // Single Zarr file for all streams - concurrent writes are supported
        // via stream-specific subgroups: /streams/{stream_name}/
        let zarr_store_path = PathBuf::from(format!("{}.zarr", self.output.display()));

        (
            zarr_store_path,
            self.stream_name
                .clone()
                .unwrap_or_else(|| self.source_id.clone()),
            self.subject.clone(),
            self.session_id.clone(),
            self.notes.clone(),
        )
    }

    /// Serialize recorder configuration to JSON string
    pub fn to_recorder_config_json(
        &self,
        recording_start_time: Option<String>,
    ) -> anyhow::Result<String> {
        let config_json = json!({
            "flush_interval": self.flush_interval,
            "flush_buffer_size": self.flush_buffer_size,
            "immediate_flush": self.immediate_flush,
            "lsl_max_retry_attempts": self.lsl_max_retry_attempts,
            "lsl_retry_base_delay_ms": self.lsl_retry_base_delay_ms,
            "lsl_pull_timeout": self.lsl_pull_timeout,
            "resolve_timeout": self.resolve_timeout,
            "subject": self.subject,
            "session_id": self.session_id,
            "notes": self.notes,
            "interactive": self.interactive,
            "quiet": self.quiet,
            "auto_start": self.auto_start,
            "duration": self.duration,
            "buffer_size": self.buffer_size,
            "source_id": self.source_id,
            "output": self.output.display().to_string(),
            "stream_name": self.stream_name,
            "suffix": self.suffix,
            "recorded_at": recording_start_time,
            "recorder_version": env!("CARGO_PKG_VERSION")
        });

        Ok(serde_json::to_string_pretty(&config_json)?)
    }
}

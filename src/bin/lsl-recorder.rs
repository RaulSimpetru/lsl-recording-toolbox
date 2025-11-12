//! LSL Recorder - Single-stream Lab Streaming Layer recorder to Zarr format
//!
//! This tool records a single LSL stream to disk in Zarr format with support for
//! interactive control and metadata annotation.
//!
//! # Features
//!
//! - Records LSL streams to Zarr hierarchical format
//! - Interactive mode with START/STOP/QUIT commands
//! - Direct mode with auto-start recording
//! - Configurable flush intervals and buffer sizes
//! - Memory monitoring and adaptive buffer sizing
//! - Subject, session, and notes metadata support
//!
//! # Usage
//!
//! ```bash
//! # Interactive mode (default)
//! lsl-recorder --source-id "EMG_1234" --output experiment --subject P001
//! # Then use commands: START, STOP, STOP_AFTER <seconds>, QUIT
//!
//! # Direct mode with auto-start
//! lsl-recorder --source-id "EMG_1234" --output experiment --auto-start
//!
//! # With full metadata
//! lsl-recorder --source-id "EEG_5678" \
//!   --stream-name "EEG" \
//!   --output experiment \
//!   --subject P001 \
//!   --session-id session_001 \
//!   --notes "Baseline recording"
//!
//! # Configure flushing behavior
//! lsl-recorder --source-id "1234" --output experiment \
//!   --flush-interval 2.0 \
//!   --flush-buffer-size 100
//! ```
//!
//! # Output Format
//!
//! Creates Zarr file structure:
//! ```text
//! experiment.zarr/
//! ├── streams/
//! │   └── <stream_name>/
//! │       ├── data        [N × C] float32
//! │       ├── time        [N] float64
//! │       └── zarr.json   (metadata)
//! └── meta/               (global metadata)
//! ```
//!
//! # Interactive Commands
//!
//! - `START` - Begin recording
//! - `STOP` - Stop recording
//! - `STOP_AFTER <seconds>` - Stop after specified duration
//! - `QUIT` - Exit the program

use anyhow::Result;
use clap::Parser;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;

use lsl_recording_toolbox::cli::Args;
use lsl_recording_toolbox::commands::handle_commands;
use lsl_recording_toolbox::lsl::{record_lsl_stream, RecordingConfig, RecordingParams, StreamResolutionConfig, ZarrConfig};

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if !args.quiet {
        lsl_recording_toolbox::display_license_notice("lsl-recorder");
        tracing_subscriber::fmt::init();
    }

    // Determine auto-start behavior
    let auto_start = args.auto_start.unwrap_or(!args.interactive);

    let recording = Arc::new(AtomicBool::new(auto_start));
    let quit = Arc::new(AtomicBool::new(false));

    // Prepare Zarr configuration
    let zarr_tuple = args.zarr_config();
    let zarr_config = Some(ZarrConfig {
        store_path: zarr_tuple.0,
        stream_name: zarr_tuple.1,
        subject: zarr_tuple.2,
        session_id: zarr_tuple.3,
        notes: zarr_tuple.4,
    });

    // Prepare recording configuration
    let recording_config = RecordingConfig {
        flush_interval: Duration::from_secs_f64(args.flush_interval),
        flush_buffer_size: args.flush_buffer_size,
        immediate_flush: args.immediate_flush,
    };

    // Prepare stream resolution configuration
    let resolution_config = StreamResolutionConfig {
        timeout: args.resolve_timeout,
        max_retry_attempts: args.lsl_max_retry_attempts,
        retry_base_delay_ms: args.lsl_retry_base_delay_ms,
        manual_pull_timeout: args.lsl_pull_timeout,
    };

    if args.interactive {
        // Interactive mode: spawn threads for command handling and recording
        let recording_clone = recording.clone();
        let quit_clone = quit.clone();
        let source_id = args.source_id.clone();

        // Spawn LSL recording thread
        let recording_thread = {
            let recording = recording_clone;
            let quit = quit_clone;
            let zarr_config_clone = zarr_config.clone();
            let recording_config_clone = recording_config.clone();
            let resolution_config_clone = resolution_config.clone();
            let quiet = args.quiet;

            thread::spawn(move || {
                let args_clone = args.clone();
                let params = RecordingParams {
                    source_id: &source_id,
                    recording,
                    quit,
                    quiet,
                    zarr_config: zarr_config_clone,
                    recording_config: recording_config_clone,
                    resolution_config: resolution_config_clone,
                    recorder_args: &args_clone,
                };

                if let Err(e) = record_lsl_stream(params) {
                    eprintln!("Recording error: {}", e);
                }
            })
        };

        // Handle commands on main thread
        if let Err(e) = handle_commands(recording, quit.clone()) {
            eprintln!("Command handling error: {}", e);
        }

        // Wait for recording thread to finish
        recording_thread.join().unwrap();
    } else {
        // Direct recording mode
        if !args.quiet {
            println!(
                "Starting direct recording for source ID: {}",
                args.source_id
            );
        }

        // Set up duration timer (regardless of quiet mode)
        if let Some(duration) = args.duration {
            if !args.quiet {
                println!("Recording will stop after {} seconds", duration);
            }
            let recording_clone = recording.clone();
            let quit_clone = quit.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_secs(duration));
                recording_clone.store(false, Ordering::SeqCst);
                quit_clone.store(true, Ordering::SeqCst);
            });
        }

        let params = RecordingParams {
            source_id: &args.source_id,
            recording,
            quit,
            quiet: args.quiet,
            zarr_config,
            recording_config,
            resolution_config,
            recorder_args: &args,
        };

        record_lsl_stream(params)?;
    }

    Ok(())
}

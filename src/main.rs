mod cli;
mod commands;
mod hdf5;
mod lsl;
mod merger;

use anyhow::Result;
use clap::Parser;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;

use cli::Args;
use commands::handle_commands;
use lsl::record_lsl_stream;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if !args.quiet {
        tracing_subscriber::fmt::init();
    }

    // Determine auto-start behavior
    let auto_start = args.auto_start.unwrap_or(!args.interactive);

    let recording = Arc::new(AtomicBool::new(auto_start));
    let quit = Arc::new(AtomicBool::new(false));

    // Prepare HDF5 configuration using CLI module
    let hdf5_config = Some(args.hdf5_config());

    // Prepare flush configuration
    let flush_interval = Duration::from_secs_f64(args.flush_interval);
    let flush_buffer_size = args.flush_buffer_size;
    let immediate_flush = args.immediate_flush;

    // Prepare retry configuration
    let max_retry_attempts = args.lsl_max_retry_attempts;
    let retry_base_delay_ms = args.lsl_retry_base_delay_ms;

    // Prepare LSL pull timeout configuration
    let lsl_pull_timeout = args.lsl_pull_timeout;

    if args.interactive {
        // Interactive mode: spawn threads for command handling and recording
        let recording_clone = recording.clone();
        let quit_clone = quit.clone();
        let source_id = args.source_id.clone();
        let timeout = args.resolve_timeout;
        let quiet = args.quiet;

        // Spawn LSL recording thread
        let recording_thread = {
            let recording = recording_clone;
            let quit = quit_clone;
            let hdf5_config_clone = hdf5_config.clone();
            thread::spawn(move || {
                let args_clone = args.clone();
                if let Err(e) = record_lsl_stream(
                    &source_id,
                    timeout,
                    recording,
                    quit,
                    quiet,
                    hdf5_config_clone,
                    flush_interval,
                    flush_buffer_size,
                    immediate_flush,
                    max_retry_attempts,
                    retry_base_delay_ms,
                    lsl_pull_timeout,
                    &args_clone,
                ) {
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

        record_lsl_stream(
            &args.source_id,
            args.resolve_timeout,
            recording,
            quit,
            args.quiet,
            hdf5_config,
            flush_interval,
            flush_buffer_size,
            immediate_flush,
            max_retry_attempts,
            retry_base_delay_ms,
            lsl_pull_timeout,
            &args,
        )?;
    }

    Ok(())
}

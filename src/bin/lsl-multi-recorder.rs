//! LSL Multi-Recorder - Unified controller for recording multiple LSL streams
//!
//! This tool provides synchronized control over multiple LSL stream recordings,
//! broadcasting commands to all recorders and managing their lifecycle.
//!
//! # Features
//!
//! - Record multiple LSL streams simultaneously
//! - Synchronized START/STOP/QUIT commands across all recorders
//! - Single shared Zarr file for all streams
//! - Millisecond-level synchronization of start/stop events
//! - Shared metadata (subject, session, notes) across recordings
//! - File locking prevents race conditions during concurrent writes
//! - Professional tab-delimited output formatting
//! - Labeled output from each child recorder
//! - Process lifecycle management and clean shutdown
//! - Cross-platform support (Windows/Linux/Mac)
//!
//! # Usage
//!
//! ```bash
//! # Record two streams interactively
//! lsl-multi-recorder \
//!   --source-ids "EMG_1234" "EEG_5678" \
//!   --stream-names "EMG" "EEG" \
//!   --output experiment \
//!   --subject P001
//!
//! # With full metadata
//! lsl-multi-recorder \
//!   --source-ids "EMG_1234" "EEG_5678" "Markers_9999" \
//!   --stream-names "EMG" "EEG" "Events" \
//!   --output experiment \
//!   --subject P001 \
//!   --session-id session_001 \
//!   --notes "Multi-modal recording session"
//!
//! # Custom flush settings
//! lsl-multi-recorder \
//!   --source-ids "id1" "id2" \
//!   --output experiment \
//!   --flush-interval 2.0
//! ```
//!
//! # Interactive Commands
//!
//! After starting, use these commands:
//! - `START` - Begin recording all streams
//! - `STOP` - Stop recording all streams
//! - `STOP_AFTER <seconds>` - Stop all streams after duration
//! - `QUIT` - Terminate all recorders
//!
//! # Output Format
//!
//! All streams write to a single shared Zarr file:
//! ```text
//! experiment.zarr/
//! ├── EMG/
//! │   ├── data
//! │   ├── time
//! │   └── zarr.json (stream metadata)
//! ├── EEG/
//! │   ├── data
//! │   ├── time
//! │   └── zarr.json (stream metadata)
//! ├── Events/
//! │   ├── events
//! │   ├── time
//! │   └── zarr.json (stream metadata)
//! └── zarr.json (root metadata)
//! ```

use anyhow::{Context, Result};
use clap::Parser;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

#[derive(Debug, Clone)]
enum RecorderEvent {
    FirstSample { stream_name: String, is_regular: bool },
    Stopped,
}

#[derive(Parser)]
#[command(name = "lsl-multi-recorder")]
#[command(about = "Record multiple LSL streams simultaneously with unified control")]
struct Args {
    #[arg(
        long,
        required = true,
        num_args = 1..,
        help = "LSL stream source IDs to record (space-separated)"
    )]
    source_ids: Vec<String>,

    #[arg(
        long,
        short = 'o',
        help = "Zarr experiment base path (without .zarr extension)",
        default_value = "experiment"
    )]
    output: PathBuf,

    #[arg(long, help = "Subject identifier for metadata")]
    subject: Option<String>,

    #[arg(long, help = "Session identifier for metadata")]
    session_id: Option<String>,

    #[arg(long, help = "Notes for metadata")]
    notes: Option<String>,

    #[arg(
        long,
        default_value = "5.0",
        help = "Timeout for stream resolution in seconds"
    )]
    resolve_timeout: f64,

    #[arg(
        long,
        default_value = "1.0",
        help = "Flush data to disk interval in seconds"
    )]
    flush_interval: f64,

    #[arg(
        long,
        default_value = "50",
        help = "Buffer size before forcing flush (number of samples)"
    )]
    flush_buffer_size: usize,

    #[arg(
        long,
        help = "Flush immediately after every sample (maximum safety, lower performance)"
    )]
    immediate_flush: bool,

    #[arg(long, short = 'q', help = "Minimal output mode for child recorders")]
    quiet: bool,

    #[arg(
        long,
        num_args = 0..,
        help = "Custom stream names (must match source-ids count if provided)"
    )]
    stream_names: Option<Vec<String>>,

    #[arg(
        long,
        help = "Path to lsl-recorder executable (defaults to ./target/debug/lsl-recorder[.exe])"
    )]
    recorder_path: Option<PathBuf>,

    #[arg(
        long,
        help = "Auto-stop recording after specified duration in seconds (timer starts when all regular streams ready)"
    )]
    duration: Option<u64>,
}

struct RecorderProcess {
    source_id: String,
    stream_name: String,
    child: Child,
    stdin: std::process::ChildStdin,
    is_regular: Option<bool>, // None = unknown, Some(true) = regular, Some(false) = irregular
    first_sample_received: bool,
}

fn log_with_time(message: &str, start_time: Instant) {
    let elapsed = start_time.elapsed();
    let total_millis = elapsed.as_millis();
    let seconds = (total_millis / 1000) % 60;
    let minutes = (total_millis / 60000) % 60;
    let millis = total_millis % 1000;
    println!("[+{:02}:{:02}.{:03}] {}", minutes, seconds, millis, message);
}

fn spawn_output_reader<R: BufRead + Send + 'static>(
    reader: R,
    label: String,
    stream_name: String,
    start_time: Instant,
    event_sender: mpsc::Sender<RecorderEvent>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    log_with_time(&format!("[{}] {}", label, line), start_time);

                    // Parse FIRST_SAMPLE messages
                    if line.contains("STATUS FIRST_SAMPLE") {
                        let is_regular = line.contains("(regular)");
                        let _ = event_sender.send(RecorderEvent::FirstSample {
                            stream_name: stream_name.clone(),
                            is_regular,
                        });
                    }

                    // Parse STOPPED_BY_TIMER messages
                    if line.contains("STATUS STOPPED_BY_TIMER") {
                        let _ = event_sender.send(RecorderEvent::Stopped);
                    }
                }
                Err(_) => break,
            }
        }
    })
}

fn spawn_recorder(
    source_id: &str,
    stream_name: &str,
    args: &Args,
    recorder_path: &str,
) -> Result<RecorderProcess> {
    let mut cmd_args = vec![
        "--interactive".to_string(),
        "--source-id".to_string(),
        source_id.to_string(),
        "--stream-name".to_string(),
        stream_name.to_string(),
        "-o".to_string(),
        args.output.display().to_string(),
        "--resolve-timeout".to_string(),
        args.resolve_timeout.to_string(),
        "--flush-interval".to_string(),
        args.flush_interval.to_string(),
        "--flush-buffer-size".to_string(),
        args.flush_buffer_size.to_string(),
    ];

    if args.immediate_flush {
        cmd_args.push("--immediate-flush".to_string());
    }

    if args.quiet {
        cmd_args.push("--quiet".to_string());
    }

    if let Some(ref subject) = args.subject {
        cmd_args.push("--subject".to_string());
        cmd_args.push(subject.clone());
    }

    if let Some(ref session_id) = args.session_id {
        cmd_args.push("--session-id".to_string());
        cmd_args.push(session_id.clone());
    }

    if let Some(ref notes) = args.notes {
        cmd_args.push("--notes".to_string());
        cmd_args.push(notes.clone());
    }

    if let Some(duration) = args.duration {
        cmd_args.push("--duration".to_string());
        cmd_args.push(duration.to_string());
    }

    let mut child = Command::new(recorder_path)
        .args(&cmd_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context(format!("Failed to spawn recorder for {}", source_id))?;

    let stdin = child
        .stdin
        .take()
        .context("Failed to get stdin for recorder")?;

    Ok(RecorderProcess {
        source_id: source_id.to_string(),
        stream_name: stream_name.to_string(),
        child,
        stdin,
        is_regular: None, // Will be determined from FIRST_SAMPLE message
        first_sample_received: false,
    })
}

fn broadcast_command(recorders: &mut [RecorderProcess], command: &str) -> Result<()> {
    for recorder in recorders.iter_mut() {
        writeln!(recorder.stdin, "{}", command)
            .context(format!("Failed to send {} to {}", command, recorder.source_id))?;
        recorder.stdin.flush().ok();
    }
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    let start_time = Instant::now();

    if !args.quiet {
        lsl_recording_toolbox::display_license_notice("lsl-multi-recorder");
    }

    // Validate stream names if provided
    if let Some(ref names) = args.stream_names {
        if names.len() != args.source_ids.len() {
            anyhow::bail!(
                "Number of stream names ({}) must match number of source IDs ({})",
                names.len(),
                args.source_ids.len()
            );
        }
    }

    log_with_time(
        &format!(
            "LSL Multi-Recorder - Managing {} streams",
            args.source_ids.len()
        ),
        start_time,
    );

    // Determine recorder executable path
    let recorder_path = args.recorder_path.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|| {
        if cfg!(windows) {
            ".\\target\\debug\\lsl-recorder.exe".to_string()
        } else {
            "./target/debug/lsl-recorder".to_string()
        }
    });

    log_with_time("Spawning recorder processes...", start_time);

    // Create channel for receiving events from recorder output threads
    let (event_sender, event_receiver) = mpsc::channel();

    let mut recorders: Vec<RecorderProcess> = Vec::new();
    let mut output_threads: Vec<thread::JoinHandle<()>> = Vec::new();

    for (idx, source_id) in args.source_ids.iter().enumerate() {
        let stream_name = args
            .stream_names
            .as_ref()
            .map(|names| names[idx].clone())
            .unwrap_or_else(|| source_id.clone());

        log_with_time(
            &format!(
                "\tSpawning recorder for source_id='{}' (stream_name='{}')",
                source_id, stream_name
            ),
            start_time,
        );

        let mut recorder = spawn_recorder(source_id, &stream_name, &args, &recorder_path)?;

        // Spawn output readers for this recorder
        let stdout = recorder
            .child
            .stdout
            .take()
            .context("Failed to get stdout")?;
        let stderr = recorder
            .child
            .stderr
            .take()
            .context("Failed to get stderr")?;

        let label_out = format!("{}-OUT", stream_name);
        let label_err = format!("{}-ERR", stream_name);

        output_threads.push(spawn_output_reader(
            BufReader::new(stdout),
            label_out.clone(),
            stream_name.clone(),
            start_time,
            event_sender.clone(),
        ));
        output_threads.push(spawn_output_reader(
            BufReader::new(stderr),
            label_err.clone(),
            stream_name.clone(),
            start_time,
            event_sender.clone(),
        ));

        recorders.push(recorder);
    }

    log_with_time(
        &format!("All {} recorders spawned successfully", recorders.len()),
        start_time,
    );
    println!();
    log_with_time("Interactive mode active. Available commands:", start_time);
    log_with_time("\tSTART - Begin recording on all streams", start_time);
    log_with_time("\tSTOP - Stop recording on all streams", start_time);
    log_with_time(
        "\tSTOP_AFTER <seconds> - Stop all after duration",
        start_time,
    );
    log_with_time("\tQUIT - Terminate all recorders and exit", start_time);
    if let Some(duration) = args.duration {
        log_with_time(
            &format!("\tAuto-stop enabled: {}s after all regular streams ready", duration),
            start_time,
        );
    }
    println!();

    // Spawn thread to read stdin commands
    let (cmd_sender, cmd_receiver) = mpsc::channel();
    thread::spawn(move || {
        let stdin = std::io::stdin();
        for line_res in stdin.lock().lines() {
            if let Ok(line) = line_res {
                if cmd_sender.send(line).is_err() {
                    break; // Main thread closed
                }
            }
        }
    });

    // Main event loop: handle both commands and recorder events
    let mut stop_after_pending = args.duration;
    let mut recording_started = false;

    loop {
        // Process recorder events
        while let Ok(event) = event_receiver.try_recv() {
            match event {
                RecorderEvent::FirstSample { stream_name, is_regular } => {
                    // Update recorder state
                    if let Some(recorder) = recorders.iter_mut().find(|r| r.stream_name == stream_name) {
                        recorder.is_regular = Some(is_regular);
                        recorder.first_sample_received = true;
                    }

                    // Check if all regular streams are ready
                    if stop_after_pending.is_some() && recording_started {
                        let all_regular_ready = recorders.iter()
                            .filter(|r| r.is_regular == Some(true))
                            .all(|r| r.first_sample_received);

                        if all_regular_ready {
                            let duration = stop_after_pending.unwrap();
                            log_with_time(
                                &format!("All regular streams ready! Sending STOP_AFTER {}", duration),
                                start_time,
                            );
                            broadcast_command(&mut recorders, &format!("STOP_AFTER {}", duration))?;
                            stop_after_pending = None; // Only send once
                        }
                    }
                }
                RecorderEvent::Stopped => {
                    // Stream auto-stopped, handled elsewhere
                }
            }
        }

        // Process stdin commands (non-blocking)
        if let Ok(cmd) = cmd_receiver.try_recv() {
            let cmd = cmd.trim();

            if cmd.eq_ignore_ascii_case("START") {
                log_with_time("Broadcasting START to all recorders...", start_time);
                broadcast_command(&mut recorders, "START")?;
                log_with_time("\tSTART command sent to all streams", start_time);
                recording_started = true;

                // If duration is set and there are NO regular streams (all irregular),
                // send STOP_AFTER immediately
                if let Some(duration) = stop_after_pending {
                    let has_regular_streams = recorders.iter()
                        .any(|r| r.is_regular == Some(true));

                    if !has_regular_streams {
                        // Wait a bit for stream types to be detected
                        thread::sleep(std::time::Duration::from_millis(500));

                        // Re-check after delay
                        let still_no_regular = recorders.iter()
                            .all(|r| r.is_regular != Some(true));

                        if still_no_regular {
                            log_with_time(
                                &format!("No regular streams detected, sending STOP_AFTER {} immediately", duration),
                                start_time,
                            );
                            broadcast_command(&mut recorders, &format!("STOP_AFTER {}", duration))?;
                            stop_after_pending = None;
                        }
                    }
                }
            } else if cmd.eq_ignore_ascii_case("STOP") {
                log_with_time("Broadcasting STOP to all recorders...", start_time);
                broadcast_command(&mut recorders, "STOP")?;
                log_with_time("\tSTOP command sent to all streams", start_time);
            } else if let Some(arg) = cmd.strip_prefix("STOP_AFTER ") {
                if let Ok(secs) = arg.trim().parse::<u64>() {
                    log_with_time(
                        &format!("Will stop all recorders after {} seconds (when regular streams ready)", secs),
                        start_time,
                    );
                    stop_after_pending = Some(secs);
                } else {
                    log_with_time("ERROR: Invalid STOP_AFTER argument", start_time);
                }
            } else if cmd.eq_ignore_ascii_case("QUIT") {
                log_with_time("Broadcasting QUIT to all recorders...", start_time);
                broadcast_command(&mut recorders, "QUIT")?;
                log_with_time("\tQUIT command sent to all streams", start_time);
                break;
            } else if !cmd.is_empty() {
                log_with_time(
                    &format!("ERROR: Unknown command '{}'", cmd),
                    start_time,
                );
            }
        }

        // Small sleep to avoid busy loop
        thread::sleep(std::time::Duration::from_millis(10));
    }

    // Wait for all recorder processes to finish
    log_with_time("Waiting for all recorders to finish...", start_time);
    for recorder in &mut recorders {
        let status = recorder.child.wait().context(format!(
            "Failed to wait for recorder {}",
            recorder.source_id
        ))?;
        log_with_time(
            &format!(
                "\tRecorder '{}' finished (status: {})",
                recorder.stream_name, status
            ),
            start_time,
        );
    }

    log_with_time("All recordings completed successfully", start_time);
    println!();

    // All streams are now saved to a single Zarr file
    let zarr_filename = format!("{}.zarr", args.output.display());
    log_with_time(&format!("Generated Zarr store: {}", zarr_filename), start_time);
    log_with_time("Recorded streams:", start_time);

    for recorder in &recorders {
        log_with_time(&format!("\t/{}/", recorder.stream_name), start_time);
    }

    Ok(())
}

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
//! ├── streams/
//! │   ├── EMG/
//! │   │   ├── data
//! │   │   └── time
//! │   ├── EEG/
//! │   │   ├── data
//! │   │   └── time
//! │   └── Events/
//! │       ├── events
//! │       └── time
//! └── meta/  (shared metadata)
//! ```

use anyhow::{Context, Result};
use clap::Parser;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Instant;

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
}

struct RecorderProcess {
    source_id: String,
    stream_name: String,
    child: Child,
    stdin: std::process::ChildStdin,
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
    start_time: Instant,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    log_with_time(&format!("[{}] {}", label, line), start_time);
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
            label_out,
            start_time,
        ));
        output_threads.push(spawn_output_reader(
            BufReader::new(stderr),
            label_err,
            start_time,
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
    println!();

    // Handle commands from stdin
    let stdin = std::io::stdin();
    for line_res in stdin.lock().lines() {
        match line_res {
            Ok(line) => {
                let cmd = line.trim();

                if cmd.eq_ignore_ascii_case("START") {
                    log_with_time("Broadcasting START to all recorders...", start_time);
                    broadcast_command(&mut recorders, "START")?;
                    log_with_time("\tSTART command sent to all streams", start_time);
                } else if cmd.eq_ignore_ascii_case("STOP") {
                    log_with_time("Broadcasting STOP to all recorders...", start_time);
                    broadcast_command(&mut recorders, "STOP")?;
                    log_with_time("\tSTOP command sent to all streams", start_time);
                } else if let Some(arg) = cmd.strip_prefix("STOP_AFTER ") {
                    if let Ok(secs) = arg.trim().parse::<u64>() {
                        log_with_time(
                            &format!("Will stop all recorders after {} seconds", secs),
                            start_time,
                        );
                        broadcast_command(&mut recorders, &format!("STOP_AFTER {}", secs))?;
                        log_with_time(
                            &format!("\tSTOP_AFTER {} sent to all streams", secs),
                            start_time,
                        );
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
            Err(e) => {
                eprintln!("stdin read error: {}", e);
                break;
            }
        }
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
        log_with_time(&format!("\t/streams/{}/", recorder.stream_name), start_time);
    }

    Ok(())
}

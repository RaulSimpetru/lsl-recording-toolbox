use anyhow::Result;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

fn log_with_time(message: &str, start_time: Instant) {
    let elapsed = start_time.elapsed();
    let total_millis = elapsed.as_millis();
    let seconds = (total_millis / 1000) % 60;
    let minutes = (total_millis / 60000) % 60;
    let millis = total_millis % 1000;
    println!("[+{:02}:{:02}.{:03}] {}", minutes, seconds, millis, message);
}

fn spawn_dummy_stream(
    name: &str,
    stream_type: &str,
    source_id: &str,
    channels: u32,
    sample_rate: u32,
    data_type: &str,
) -> Result<std::process::Child> {
    let mut cmd = Command::new("./target/debug/lsl-dummy-stream");
    cmd.args([
        "--name",
        name,
        "--type",
        stream_type,
        "--source-id",
        source_id,
        "--channels",
        &channels.to_string(),
        "--sample-rate",
        &sample_rate.to_string(),
        "--data-type",
        data_type,
    ])
    .stdout(Stdio::null())
    .stderr(Stdio::null());

    Ok(cmd.spawn()?)
}

fn record_with_metadata(
    source_id: &str,
    duration: u32,
    subject: &str,
    session_id: &str,
    notes: &str,
) -> Result<bool> {
    let result = Command::new("./target/debug/lsl-recorder")
        .args([
            "--source-id",
            source_id,
            "--duration",
            &duration.to_string(),
            "--subject",
            subject,
            "--session-id",
            session_id,
            "--notes",
            notes,
            "--quiet",
        ])
        .output()?;

    Ok(result.status.success())
}

fn run_inspection(file_path: &str, description: &str, start_time: Instant) -> Result<()> {
    if !std::path::Path::new(file_path).exists() {
        log_with_time(
            &format!("\t{} does not exist, skipping", file_path),
            start_time,
        );
        return Ok(());
    }

    log_with_time(&format!("Inspecting {}", file_path), start_time);

    let inspection_result = Command::new("./target/debug/lsl-inspect")
        .arg(file_path)
        .output()?;

    if inspection_result.status.success() {
        log_with_time("\tInspection completed", start_time);
        let stdout = String::from_utf8_lossy(&inspection_result.stdout);
        if !stdout.trim().is_empty() {
            println!();
            for line in stdout.lines() {
                println!("\t{}", line);
            }
            println!();
        }
    } else {
        log_with_time("\tInspection failed", start_time);
        let stderr = String::from_utf8_lossy(&inspection_result.stderr);
        if !stderr.trim().is_empty() {
            println!("\tError: {}", stderr);
        }
    }

    Ok(())
}

/// Demonstrates lsl-inspect usage for HDF5 file exploration.
///
/// Shows:
/// - Basic file inspection
/// - Metadata exploration
fn main() -> Result<()> {
    let start_time = Instant::now();

    println!("LSL Inspection Demo");
    println!("==================");
    println!("Demonstrates HDF5 file inspection and metadata exploration");
    println!();

    // Clean up existing files
    log_with_time("Cleaning up existing files", start_time);
    let _ = std::fs::remove_file("experiment_DEMO_INSPECT.h5");

    // Create and record a test stream
    log_with_time("Creating test stream", start_time);
    let mut stream = spawn_dummy_stream("InspectEMG", "EMG", "DEMO_INSPECT", 8, 1000, "float32")?;

    thread::sleep(Duration::from_millis(1500));

    let success = record_with_metadata(
        "DEMO_INSPECT",
        3,
        "DEMO_PARTICIPANT",
        "inspect_session",
        "Demo recording for inspection example",
    )?;

    stream.kill()?;
    stream.wait()?;

    if success {
        log_with_time("\tRecording completed", start_time);
        run_inspection("experiment_DEMO_INSPECT.h5", "Demo recording", start_time)?;
    }

    println!();
    log_with_time("Demo completed", start_time);

    Ok(())
}

use anyhow::Result;
use std::io::Write;
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

/// Basic lsl-recorder usage demonstration.
///
/// Shows essential recording patterns:
/// - Auto-start recording mode
/// - Interactive control mode
/// - Metadata configuration
fn main() -> Result<()> {
    let start_time = Instant::now();

    println!("LSL Basic Recording Demo");
    println!("========================");
    println!("Demonstrates auto-start, interactive, and metadata recording modes");
    println!();

    // Clean up existing files
    log_with_time("Cleaning up existing files", start_time);
    let _ = std::fs::remove_file("experiment_DEMO_EMG.h5");
    let _ = std::fs::remove_file("experiment_DEMO_EEG.h5");

    // Demo 1: Auto-start recording
    log_with_time("DEMO 1: Auto-start recording", start_time);
    log_with_time("Starting dummy EMG stream", start_time);

    let mut dummy_stream = Command::new("./target/debug/lsl-dummy-stream")
        .args([
            "--name",
            "DemoEMG",
            "--source-id",
            "DEMO_EMG",
            "--channels",
            "8",
            "--sample-rate",
            "1000",
            "--data-type",
            "float32",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    // Wait for stream to initialize
    thread::sleep(Duration::from_millis(1000));

    log_with_time("Recording for 3 seconds in auto-start mode", start_time);

    let recorder_result = Command::new("./target/debug/lsl-recorder")
        .args(["--source-id", "DEMO_EMG", "--duration", "3"])
        .output()?;

    if recorder_result.status.success() {
        log_with_time("\tAuto-start recording completed", start_time);
        let stdout = String::from_utf8_lossy(&recorder_result.stdout);
        if !stdout.trim().is_empty() {
            println!();
            for line in stdout.lines() {
                println!("\t{}", line);
            }
        }
    } else {
        log_with_time("\tAuto-start recording failed", start_time);
        let stderr = String::from_utf8_lossy(&recorder_result.stderr);
        if !stderr.is_empty() {
            println!("\tError: {}", stderr);
        }
    }

    dummy_stream.kill()?;
    dummy_stream.wait()?;
    log_with_time("\tStopped dummy stream", start_time);
    thread::sleep(Duration::from_millis(500));

    // Demo 2: Interactive recording
    log_with_time("DEMO 2: Interactive recording", start_time);
    log_with_time("Starting dummy EEG stream", start_time);

    let mut dummy_stream = Command::new("./target/debug/lsl-dummy-stream")
        .args([
            "--name",
            "DemoEEG",
            "--source-id",
            "DEMO_EEG",
            "--channels",
            "8",
            "--sample-rate",
            "500",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    thread::sleep(Duration::from_millis(1000));

    let mut recorder = Command::new("./target/debug/lsl-recorder")
        .args(["--source-id", "DEMO_EEG", "--interactive"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = recorder.stdin.take() {
        log_with_time("Recording for 2 seconds", start_time);
        writeln!(stdin, "START")?;
        thread::sleep(Duration::from_secs(2));
        writeln!(stdin, "STOP")?;
        writeln!(stdin, "QUIT")?;
    }

    let output = recorder.wait_with_output()?;
    if output.status.success() {
        log_with_time("\tInteractive recording completed", start_time);
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.trim().is_empty() {
            println!();
            for line in stdout.lines() {
                println!("\t{}", line);
            }
        }
    } else {
        log_with_time("\tInteractive recording failed", start_time);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.trim().is_empty() {
            println!("\tError: {}", stderr);
        }
    }

    dummy_stream.kill()?;
    dummy_stream.wait()?;

    // Show results
    println!();
    log_with_time("Results:", start_time);

    for file_name in &["experiment_DEMO_EMG.h5", "experiment_DEMO_EEG.h5"] {
        if std::path::Path::new(file_name).exists() {
            let metadata = std::fs::metadata(file_name)?;
            log_with_time(
                &format!("\t{} ({:.1} KB)", file_name, metadata.len() as f64 / 1024.0),
                start_time,
            );
        } else {
            log_with_time(&format!("\t{} (not created)", file_name), start_time);
        }
    }

    println!();
    log_with_time("Demo completed", start_time);

    Ok(())
}

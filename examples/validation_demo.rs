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
    source_id: &str,
    channels: u32,
    sample_rate: u32,
) -> Result<std::process::Child> {
    let mut cmd = Command::new("./target/debug/lsl-dummy-stream");
    cmd.args([
        "--name",
        name,
        "--source-id",
        source_id,
        "--channels",
        &channels.to_string(),
        "--sample-rate",
        &sample_rate.to_string(),
    ])
    .stdout(Stdio::null())
    .stderr(Stdio::null());

    Ok(cmd.spawn()?)
}

fn record_stream_delayed(
    source_id: &str,
    output_base: &str,
    duration: u32,
    delay_ms: u64,
) -> Result<bool> {
    use std::process::Stdio;
    use std::time::Instant;

    // Add a delay before starting recording to simulate timing differences
    thread::sleep(Duration::from_millis(delay_ms));

    let start_time = Instant::now();

    // Use spawn with timeout instead of output() to ensure process termination
    let mut child = Command::new("./target/debug/lsl-recorder")
        .args([
            "--source-id",
            source_id,
            "-o",
            output_base,
            "--duration",
            &duration.to_string(),
            "--subject",
            "VALIDATION_DEMO",
            "--session-id",
            "timing_analysis_demo",
            "--quiet",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    // Wait for the expected duration plus a reasonable buffer (50% extra time)
    let timeout_duration = Duration::from_secs((duration as f64 * 1.5) as u64 + 2);

    // Check if process completes within expected time
    let mut completed = false;
    let mut success = false;

    for _ in 0..(timeout_duration.as_millis() / 100) {
        match child.try_wait()? {
            Some(status) => {
                completed = true;
                success = status.success();
                break;
            }
            None => {
                thread::sleep(Duration::from_millis(100));
            }
        }
    }

    let elapsed = start_time.elapsed();

    if !completed {
        // Process is taking too long, force kill it
        let _ = child.kill();
        let _ = child.wait();
        eprintln!(
            "WARNING: Recorder for {} exceeded timeout ({:.1}s), was forcibly terminated",
            source_id,
            elapsed.as_secs_f64()
        );
        return Ok(false);
    }

    // Log completion time for debugging
    if elapsed.as_secs_f64() > (duration as f64 + 1.0) {
        eprintln!(
            "WARNING: Recorder for {} took {:.1}s (expected ~{}s)",
            source_id,
            elapsed.as_secs_f64(),
            duration
        );
    }

    Ok(success)
}

fn run_validation(file_path: &str, description: &str, start_time: Instant) -> Result<()> {
    log_with_time(&format!("Validating {}", file_path), start_time);

    let validation_result = Command::new("./target/debug/lsl-validate")
        .arg(file_path)
        .output()?;

    if validation_result.status.success() {
        log_with_time("\tValidation completed", start_time);
        let stdout = String::from_utf8_lossy(&validation_result.stdout);
        if !stdout.trim().is_empty() {
            println!();
            for line in stdout.lines() {
                println!("\t{}", line);
            }
            println!();
        }
    } else {
        log_with_time("\tValidation failed", start_time);
        let stderr = String::from_utf8_lossy(&validation_result.stderr);
        if !stderr.trim().is_empty() {
            println!("\tError: {}", stderr);
        }
    }

    Ok(())
}

/// Demonstrates lsl-validate for timing analysis.
///
/// Shows:
/// - Recording synchronized streams
/// - Validation analysis
fn main() -> Result<()> {
    let start_time = Instant::now();

    println!("LSL Validation Demo");
    println!("===================");
    println!("Demonstrates multi-stream recording and synchronization validation");
    println!();

    // Clean up existing files
    log_with_time("Cleaning up existing files", start_time);
    let _ = std::fs::remove_dir_all("validation_demo_EMG_EMG_DEMO.zarr");
    let _ = std::fs::remove_dir_all("validation_demo_EEG_EEG_DEMO.zarr");
    let _ = std::fs::remove_dir_all("validation_demo_merged.zarr");

    // Create synchronized streams
    log_with_time("Creating synchronized streams", start_time);
    let mut stream1 = spawn_dummy_stream("DemoEMG", "EMG_DEMO", 8, 1000)?;
    let mut stream2 = spawn_dummy_stream("DemoEEG", "EEG_DEMO", 16, 500)?;

    thread::sleep(Duration::from_secs(2));

    // Record simultaneously
    log_with_time("Recording streams simultaneously", start_time);
    let handle1 = thread::spawn(|| record_stream_delayed("EMG_DEMO", "validation_demo_EMG", 4, 0));
    let handle2 = thread::spawn(|| record_stream_delayed("EEG_DEMO", "validation_demo_EEG", 4, 0));

    let emg_success = handle1.join().unwrap()?;
    let eeg_success = handle2.join().unwrap()?;

    stream1.kill()?;
    stream2.kill()?;
    stream1.wait()?;
    stream2.wait()?;

    if !(emg_success && eeg_success) {
        log_with_time("\tRecording failed", start_time);
        return Ok(());
    }

    log_with_time("\tRecording completed", start_time);

    // Merge files
    log_with_time("Merging files", start_time);
    let merge_result = Command::new("./target/debug/lsl-merge")
        .args([
            "validation_demo_EMG_EMG_DEMO.zarr",
            "validation_demo_EEG_EEG_DEMO.zarr",
            "-o",
            "validation_demo_merged.zarr",
        ])
        .output()?;

    if !merge_result.status.success() {
        log_with_time("\tMerge failed", start_time);
        return Ok(());
    }

    log_with_time("\tMerge completed", start_time);

    run_validation("validation_demo_merged.zarr", "Demo validation", start_time)?;

    // Show results
    println!();
    log_with_time("Results:", start_time);

    for file in &[
        "validation_demo_EMG_EMG_DEMO.zarr",
        "validation_demo_EEG_EEG_DEMO.zarr",
        "validation_demo_merged.zarr",
    ] {
        if std::path::Path::new(file).exists() {
            let metadata = std::fs::metadata(file)?;
            log_with_time(
                &format!("\t{} ({:.1} KB)", file, metadata.len() as f64 / 1024.0),
                start_time,
            );
        }
    }

    println!();
    log_with_time("Demo completed", start_time);

    Ok(())
}

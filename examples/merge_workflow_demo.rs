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
    data_type: &str,
    freq_range: &str,
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
        "--data-type",
        data_type,
        "--freq-range",
        freq_range,
    ])
    .stdout(Stdio::null())
    .stderr(Stdio::null());

    Ok(cmd.spawn()?)
}

fn record_stream(
    source_id: &str,
    output_base: &str,
    duration: u32,
    subject: &str,
    session: &str,
    notes: &str,
) -> Result<bool> {
    let result = Command::new("./target/debug/lsl-recorder")
        .args([
            "--source-id",
            source_id,
            "--output",
            output_base,
            "--duration",
            &duration.to_string(),
            "--subject",
            subject,
            "--session-id",
            session,
            "--notes",
            notes,
        ])
        .output()?;

    if !result.status.success() {
        eprintln!(
            "Recording failed for {}: {}",
            source_id,
            String::from_utf8_lossy(&result.stderr)
        );
    }

    Ok(result.status.success())
}

/// Demonstrates basic merge workflow with multiple streams.
///
/// Shows:
/// - Recording multiple streams
/// - Merging HDF5 files
/// - Validation of results
fn main() -> Result<()> {
    let start_time = Instant::now();

    println!("LSL Merge Workflow Demo");
    println!("=======================");
    println!("Demonstrates multi-stream recording, merging, and validation");
    println!();

    // Clean up existing files
    log_with_time("Cleaning up existing files", start_time);
    let _ = std::fs::remove_file("experiment_MERGE_EMG.h5");
    let _ = std::fs::remove_file("experiment_MERGE_EEG.h5");
    let _ = std::fs::remove_file("merged_demo.h5");

    // Step 1: Create test streams
    log_with_time("STEP 1: Creating test streams", start_time);

    let mut emg_stream = spawn_dummy_stream("DemoEMG", "MERGE_EMG", 8, 1000, "float32", "20,150")?;

    let mut eeg_stream = spawn_dummy_stream("DemoEEG", "MERGE_EEG", 16, 500, "float32", "1,40")?;

    thread::sleep(Duration::from_secs(2));

    // Step 2: Record streams
    log_with_time("STEP 2: Recording streams", start_time);

    let session_id = "merge_demo_session";

    let emg_handle = thread::spawn(move || {
        record_stream(
            "MERGE_EMG",
            "experiment",
            4,
            "DEMO_SUBJECT",
            session_id,
            "EMG demo recording",
        )
    });

    let eeg_handle = thread::spawn(move || {
        record_stream(
            "MERGE_EEG",
            "experiment",
            4,
            "DEMO_SUBJECT",
            session_id,
            "EEG demo recording",
        )
    });

    let emg_success = emg_handle.join().unwrap()?;
    let eeg_success = eeg_handle.join().unwrap()?;

    emg_stream.kill()?;
    eeg_stream.kill()?;
    emg_stream.wait()?;
    eeg_stream.wait()?;

    if emg_success && eeg_success {
        log_with_time("\tRecording completed", start_time);
    } else {
        log_with_time("\tRecording failed", start_time);
        return Ok(());
    }

    // Step 3: Merge files
    log_with_time("STEP 3: Merging files", start_time);

    let merge_result = Command::new("./target/debug/lsl-merge")
        .args([
            "experiment_MERGE_EMG.h5",
            "experiment_MERGE_EEG.h5",
            "-o",
            "merged_demo.h5",
        ])
        .output()?;

    if merge_result.status.success() {
        log_with_time("\tMerge completed", start_time);
        let stdout = String::from_utf8_lossy(&merge_result.stdout);
        if !stdout.trim().is_empty() {
            println!();
            for line in stdout.lines() {
                println!("\t{}", line);
            }
        }
    } else {
        log_with_time("\tMerge failed", start_time);
        println!("\tError: {}", String::from_utf8_lossy(&merge_result.stderr));
        return Ok(());
    }

    // Step 4: Validate result
    log_with_time("STEP 4: Validating merged file", start_time);

    let validation_result = Command::new("./target/debug/lsl-validate")
        .arg("merged_demo.h5")
        .output();

    match validation_result {
        Ok(output) if output.status.success() => {
            log_with_time("\tValidation completed", start_time);
            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stdout.trim().is_empty() {
                println!();
                for line in stdout.lines() {
                    println!("\t{}", line);
                }
            }
        }
        Ok(output) => {
            log_with_time("\tValidation failed", start_time);
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.trim().is_empty() {
                println!("\tError: {}", stderr);
            }
        }
        Err(e) => {
            log_with_time(&format!("\tValidation error: {}", e), start_time);
        }
    }

    // Show results
    println!();
    log_with_time("Results:", start_time);

    for file in &[
        "experiment_MERGE_EMG.h5",
        "experiment_MERGE_EEG.h5",
        "merged_demo.h5",
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

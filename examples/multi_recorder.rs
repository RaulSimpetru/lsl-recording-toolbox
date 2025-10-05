use anyhow::Result;
use std::io::{BufRead, BufReader, Write};
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

fn spawn_output_reader<R: BufRead + Send + 'static>(
    reader: R,
    label: &'static str,
    start_time: Instant,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    log_with_time(&format!("{}: {}", label, line), start_time);
                }
                Err(_) => break,
            }
        }
    })
}

/// Example demonstrating the lsl-multi-recorder tool for synchronized multi-stream recording.
///
/// This demo shows how to use the `lsl-multi-recorder` binary to record multiple LSL streams
/// with unified START/STOP/QUIT control. The multi-recorder handles process management and
/// command broadcasting, making it much simpler than manually spawning individual recorders.
///
/// Benefits of using lsl-multi-recorder:
/// - Single command to control all recorders
/// - Synchronized start/stop timing across all streams
/// - Shared metadata (subject, session, notes) across all recordings
/// - Automatic output labeling and process management
/// - Clean shutdown of all child processes
fn main() -> Result<()> {
    let start_time = Instant::now();
    log_with_time("🚀 Multi-Stream Recording Demo", start_time);
    log_with_time("", start_time);

    // Step 1: Spawn dummy LSL stream generators for testing
    log_with_time("📡 Starting LSL dummy stream generators...", start_time);

    let mut emg_stream = Command::new("./target/debug/lsl-dummy-stream")
        .args([
            "--name",
            "TestEMG",
            "--type",
            "EMG",
            "--source-id",
            "EMG_1234",
            "--channels",
            "8",
            "--sample-rate",
            "1000",
            "--chunk-size",
            "10",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    let mut eeg_stream = Command::new("./target/debug/lsl-dummy-stream")
        .args([
            "--name",
            "TestEEG",
            "--type",
            "EEG",
            "--source-id",
            "EEG_5678",
            "--channels",
            "16",
            "--sample-rate",
            "500",
            "--chunk-size",
            "5",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    log_with_time("  ✅ EMG stream: 8 channels @ 1000 Hz", start_time);
    log_with_time("  ✅ EEG stream: 16 channels @ 500 Hz", start_time);
    log_with_time("", start_time);

    log_with_time("⏳ Waiting for streams to initialize...", start_time);
    thread::sleep(Duration::from_secs(3));

    // Step 2: Spawn the lsl-multi-recorder with both streams
    log_with_time("🎬 Spawning lsl-multi-recorder...", start_time);

    let multi_recorder_path = if cfg!(windows) {
        ".\\target\\debug\\lsl-multi-recorder.exe"
    } else {
        "./target/debug/lsl-multi-recorder"
    };

    let mut multi_recorder = Command::new(multi_recorder_path)
        .args([
            "--source-ids",
            "EMG_1234",
            "EEG_5678",
            "--stream-names",
            "EMG",
            "EEG",
            "--output",
            "demo_experiment",
            "--subject",
            "P001",
            "--session-id",
            "demo_session_001",
            "--notes",
            "Multi-stream recording demo using lsl-multi-recorder tool",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    log_with_time("  ✅ Multi-recorder spawned successfully", start_time);
    log_with_time("", start_time);

    // Get stdin handle for sending commands
    let mut stdin = multi_recorder.stdin.take().unwrap();

    // Spawn threads to read and display output from multi-recorder
    let stdout = multi_recorder.stdout.take().unwrap();
    let stderr = multi_recorder.stderr.take().unwrap();

    let _stdout_thread = spawn_output_reader(BufReader::new(stdout), "MULTI-OUT", start_time);
    let _stderr_thread = spawn_output_reader(BufReader::new(stderr), "MULTI-ERR", start_time);

    // Give the multi-recorder time to initialize both child recorders
    thread::sleep(Duration::from_secs(2));

    // Step 3: Send synchronized START command
    log_with_time("", start_time);
    log_with_time("▶️  Sending START command...", start_time);
    writeln!(stdin, "START")?;
    stdin.flush()?;

    log_with_time("⏱️  Recording for 10 seconds...", start_time);
    thread::sleep(Duration::from_secs(10));

    // Step 4: Send synchronized STOP command
    log_with_time("", start_time);
    log_with_time("⏸️  Sending STOP command...", start_time);
    writeln!(stdin, "STOP")?;
    stdin.flush()?;

    log_with_time("⏳ Waiting for final flush...", start_time);
    thread::sleep(Duration::from_secs(2));

    // Step 5: Send QUIT command to terminate all recorders
    log_with_time("", start_time);
    log_with_time("🛑 Sending QUIT command...", start_time);
    writeln!(stdin, "QUIT")?;
    stdin.flush()?;

    // Wait for multi-recorder to finish
    log_with_time("⏳ Waiting for multi-recorder to finish...", start_time);
    let status = multi_recorder.wait()?;
    log_with_time(
        &format!("  ✅ Multi-recorder finished (status: {})", status),
        start_time,
    );

    // Cleanup: Stop dummy stream generators
    log_with_time("", start_time);
    log_with_time("🧹 Cleaning up dummy streams...", start_time);
    let _ = emg_stream.kill();
    let _ = eeg_stream.kill();
    let _ = emg_stream.wait();
    let _ = eeg_stream.wait();

    log_with_time("", start_time);
    log_with_time("🎉 Demo completed successfully!", start_time);
    log_with_time("", start_time);
    log_with_time("📁 Generated HDF5 files:", start_time);
    log_with_time("  → demo_experiment_EMG.h5 (8-channel EMG data @ 1000 Hz)", start_time);
    log_with_time("  → demo_experiment_EEG.h5 (16-channel EEG data @ 500 Hz)", start_time);
    log_with_time("", start_time);
    log_with_time("🔍 Inspect files with:", start_time);
    log_with_time("  cargo run --bin lsl-inspect -- demo_experiment_EMG.h5", start_time);
    log_with_time("  cargo run --bin lsl-inspect -- demo_experiment_EEG.h5", start_time);
    log_with_time("", start_time);
    log_with_time("🔗 Merge files with:", start_time);
    log_with_time("  cargo run --bin lsl-merge -- demo_experiment_EMG.h5 demo_experiment_EEG.h5 -o merged_demo.h5", start_time);
    log_with_time("", start_time);
    log_with_time("✨ Key advantages of lsl-multi-recorder:", start_time);
    log_with_time("  • Single command controls all recorders", start_time);
    log_with_time("  • Synchronized start/stop timing", start_time);
    log_with_time("  • Shared metadata across recordings", start_time);
    log_with_time("  • Automatic process management", start_time);
    log_with_time("  • Clean shutdown handling", start_time);

    Ok(())
}

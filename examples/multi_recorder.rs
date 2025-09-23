use std::process::{Command, Stdio};
use std::io::{Write, BufRead, BufReader};
use std::thread;
use std::time::{Duration, Instant};
use anyhow::Result;

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

/// Example parent program demonstrating synchronized recording from multiple streams.
/// This demo spawns two lsl-recorder instances that start and stop recording
/// simultaneously, ensuring equal recording durations across different streams.
/// Each recorder writes to its own HDF5 file with automatic naming.
fn main() -> Result<()> {
    let start_time = Instant::now();
    log_with_time("Spawning multiple LSL recorders...", start_time);

    // Spawn first recorder for EMG stream
    let mut recorder1 = Command::new("./target/debug/lsl-recorder")
        .args([
            "--interactive",
            "--source-id", "muovi-180319",
            "--stream-name", "EMG",
            "-o", "experiment",
            "--subject", "P001",
            "--session-id", "session_001",
            "--notes", "Multi-stream recording demo"
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    // Spawn second recorder for EEG stream
    let mut recorder2 = Command::new("./target/debug/lsl-recorder")
        .args([
            "--interactive",
            "--source-id", "1234",
            "--stream-name", "EEG",
            "-o", "experiment"
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    log_with_time("Both recorders spawned successfully", start_time);

    // Get stdin handles for sending commands
    let mut stdin1 = recorder1.stdin.take().unwrap();
    let mut stdin2 = recorder2.stdin.take().unwrap();

    // Spawn threads to read and display output from both recorders
    let stdout1 = recorder1.stdout.take().unwrap();
    let stderr1 = recorder1.stderr.take().unwrap();
    let stdout2 = recorder2.stdout.take().unwrap();
    let stderr2 = recorder2.stderr.take().unwrap();

    let _stdout1_thread = spawn_output_reader(BufReader::new(stdout1), "EMG-OUT", start_time);
    let _stderr1_thread = spawn_output_reader(BufReader::new(stderr1), "EMG-ERR", start_time);
    let _stdout2_thread = spawn_output_reader(BufReader::new(stdout2), "EEG-OUT", start_time);
    let _stderr2_thread = spawn_output_reader(BufReader::new(stderr2), "EEG-ERR", start_time);

    // Synchronized control sequence - both recorders start and stop together
    log_with_time("Sending START command to both recorders simultaneously...", start_time);
    writeln!(stdin1, "START")?;
    writeln!(stdin2, "START")?;
    log_with_time("  → START sent to both recorders", start_time);

    log_with_time("Recording for 10 seconds...", start_time);
    thread::sleep(Duration::from_secs(10));

    log_with_time("Sending STOP command to both recorders simultaneously...", start_time);
    writeln!(stdin1, "STOP")?;
    writeln!(stdin2, "STOP")?;
    log_with_time("  → STOP sent to both recorders", start_time);

    log_with_time("Waiting 2 seconds before cleanup...", start_time);
    thread::sleep(Duration::from_secs(2));

    log_with_time("Sending QUIT to both recorders...", start_time);
    writeln!(stdin1, "QUIT")?;
    writeln!(stdin2, "QUIT")?;
    log_with_time("  → QUIT sent to both recorders", start_time);

    // Wait for processes to finish
    log_with_time("Waiting for processes to finish...", start_time);
    let _ = recorder1.wait()?;
    log_with_time("  → recorder1 finished", start_time);
    let _ = recorder2.wait()?;
    log_with_time("  → recorder2 finished", start_time);

    log_with_time("All recorders finished successfully", start_time);

    log_with_time("📁 Files created with JSON metadata:", start_time);
    log_with_time("  → experiment_EMG.h5 (EMG stream data + JSON metadata)", start_time);
    log_with_time("  → experiment_EEG.h5 (EEG stream data + JSON metadata)", start_time);
    log_with_time("", start_time);
    log_with_time("🔍 To inspect the JSON metadata:", start_time);
    log_with_time("  python3 examples/inspect_hdf5_metadata.py", start_time);
    log_with_time("", start_time);
    log_with_time("📊 JSON metadata includes:", start_time);
    log_with_time("  • Complete LSL stream information (channels, rates, etc.)", start_time);
    log_with_time("  • Full recorder configuration (flush settings, timeouts, etc.)", start_time);
    log_with_time("  • Exact recording timestamps and session metadata", start_time);

    Ok(())
}
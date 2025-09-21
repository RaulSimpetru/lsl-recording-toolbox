use std::process::{Command, Stdio};
use std::io::Write;
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

/// Example parent program demonstrating how to spawn and control
/// multiple lsl-recorder instances independently using anonymous pipes
fn main() -> Result<()> {
    let start_time = Instant::now();
    log_with_time("Spawning multiple LSL recorders...", start_time);

    // Spawn first recorder for stream1
    let mut recorder1 = Command::new("./target/debug/lsl-recorder")
        .args(["--interactive", "--source-id", "stream1", "--quiet"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    // Spawn second recorder for stream2
    let mut recorder2 = Command::new("./target/debug/lsl-recorder")
        .args(["--interactive", "--source-id", "stream2", "--quiet"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    log_with_time("Both recorders spawned successfully", start_time);

    // Get stdin handles for sending commands
    let mut stdin1 = recorder1.stdin.take().unwrap();
    let mut stdin2 = recorder2.stdin.take().unwrap();

    // Example control sequence
    log_with_time("Sending START command to both recorders...", start_time);
    writeln!(stdin1, "START")?;
    log_with_time("  → START sent to recorder1", start_time);
    writeln!(stdin2, "START")?;
    log_with_time("  → START sent to recorder2", start_time);

    log_with_time("Waiting 2 seconds...", start_time);
    thread::sleep(Duration::from_secs(2));

    log_with_time("Setting recorder2 to stop after 5 seconds...", start_time);
    writeln!(stdin2, "STOP_AFTER 5")?;
    log_with_time("  → STOP_AFTER 5 sent to recorder2", start_time);

    log_with_time("Waiting 3 seconds...", start_time);
    thread::sleep(Duration::from_secs(3));

    log_with_time("Stopping recorder1...", start_time);
    writeln!(stdin1, "STOP")?;
    log_with_time("  → STOP sent to recorder1", start_time);

    log_with_time("Waiting 3 seconds...", start_time);
    thread::sleep(Duration::from_secs(3));

    log_with_time("Sending QUIT to both recorders...", start_time);
    writeln!(stdin1, "QUIT")?;
    log_with_time("  → QUIT sent to recorder1", start_time);
    writeln!(stdin2, "QUIT")?;
    log_with_time("  → QUIT sent to recorder2", start_time);

    // Wait for processes to finish
    log_with_time("Waiting for processes to finish...", start_time);
    let _ = recorder1.wait()?;
    log_with_time("  → recorder1 finished", start_time);
    let _ = recorder2.wait()?;
    log_with_time("  → recorder2 finished", start_time);

    log_with_time("All recorders finished successfully", start_time);
    Ok(())
}
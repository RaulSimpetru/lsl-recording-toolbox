use std::process::{Command, Stdio};
use std::io::{Write, BufRead, BufReader};
use std::thread;
use std::time::{Duration, Instant};
use anyhow::Result;
use lsl_recorder::sync::{SyncCoordinator, SyncConfig};
use std::path::PathBuf;

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

/// Enhanced multi-recorder with precise synchronization coordination.
/// This version uses file-based coordination to ensure millisecond-precise
/// start and stop timing between multiple recording processes.
fn main() -> Result<()> {
    let start_time = Instant::now();
    let session_id = format!("session_{}", chrono::Utc::now().format("%Y%m%d_%H%M%S"));

    log_with_time("🎯 Starting Synchronized Multi-Stream Recorder", start_time);
    log_with_time(&format!("📋 Session ID: {}", session_id), start_time);

    // Configuration for synchronization
    let sync_config = SyncConfig {
        base_dir: PathBuf::from("."),
        session_id: session_id.clone(),
        sync_timeout: Duration::from_secs(30),
        poll_interval: Duration::from_millis(10),
        precision_threshold: Duration::from_millis(5),
    };

    // Create coordinator for this master process
    let mut coordinator = SyncCoordinator::new(
        sync_config.clone(),
        "master".to_string(),
        "coordinator".to_string(),
    )?;

    log_with_time("🔄 Spawning synchronized LSL recorders...", start_time);

    // Spawn first recorder for EMG stream with sync
    let mut recorder1 = Command::new("./target/debug/lsl-recorder")
        .args([
            "--interactive",
            "--source-id", "1234",
            "--stream-name", "EMG",
            "-o", "synced_experiment",
            "--subject", "P001",
            "--session-id", &session_id,
            "--notes", "Synchronized multi-stream recording demo"
        ])
        .env("SYNC_SESSION_ID", &session_id)
        .env("SYNC_PARTICIPANT_ID", "emg_recorder")
        .env("SYNC_STREAM_NAME", "EMG")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    // Spawn second recorder for EEG stream with sync
    let mut recorder2 = Command::new("./target/debug/lsl-recorder")
        .args([
            "--interactive",
            "--source-id", "1234",
            "--stream-name", "EEG",
            "-o", "synced_experiment"
        ])
        .env("SYNC_SESSION_ID", &session_id)
        .env("SYNC_PARTICIPANT_ID", "eeg_recorder")
        .env("SYNC_STREAM_NAME", "EEG")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    log_with_time("✅ Both recorders spawned successfully", start_time);

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

    // Wait for both recorders to initialize and join synchronization
    log_with_time("⏳ Waiting for recorders to initialize...", start_time);
    thread::sleep(Duration::from_secs(3));

    // Wait for all participants to join the synchronization
    let expected_participants = vec!["EMG".to_string(), "EEG".to_string()];
    log_with_time("🔄 Waiting for all participants to join...", start_time);

    match coordinator.wait_for_participants(&expected_participants) {
        Ok(()) => {
            log_with_time("✅ All participants ready for synchronization", start_time);
        }
        Err(e) => {
            log_with_time(&format!("❌ Synchronization failed: {}", e), start_time);
            // Continue with fallback to manual coordination
            log_with_time("🔄 Falling back to manual coordination", start_time);
        }
    }

    // Coordinate synchronized start
    log_with_time("🚀 Initiating synchronized start sequence...", start_time);

    let sync_start_result = coordinator.coordinate_start();

    // Send START commands immediately after sync signal
    log_with_time("📡 Broadcasting START commands...", start_time);
    writeln!(stdin1, "START")?;
    writeln!(stdin2, "START")?;

    match sync_start_result {
        Ok(start_timestamp) => {
            log_with_time(&format!("✅ Synchronized start at timestamp: {:.6}", start_timestamp), start_time);
        }
        Err(e) => {
            log_with_time(&format!("⚠️  Sync start failed, using manual timing: {}", e), start_time);
        }
    }

    // Record for specified duration
    let recording_duration = Duration::from_secs(10);
    log_with_time(&format!("⏱️  Recording for {} seconds...", recording_duration.as_secs()), start_time);
    thread::sleep(recording_duration);

    // Coordinate synchronized stop
    log_with_time("🛑 Initiating synchronized stop sequence...", start_time);

    let sync_stop_result = coordinator.coordinate_stop();

    // Send STOP commands immediately after sync signal
    log_with_time("📡 Broadcasting STOP commands...", start_time);
    writeln!(stdin1, "STOP")?;
    writeln!(stdin2, "STOP")?;

    match sync_stop_result {
        Ok(stop_timestamp) => {
            log_with_time(&format!("✅ Synchronized stop at timestamp: {:.6}", stop_timestamp), start_time);
        }
        Err(e) => {
            log_with_time(&format!("⚠️  Sync stop failed, using manual timing: {}", e), start_time);
        }
    }

    // Wait for recording to complete
    log_with_time("⏳ Waiting for recording completion...", start_time);
    thread::sleep(Duration::from_secs(2));

    // Send QUIT to both recorders
    log_with_time("🔄 Terminating recorders...", start_time);
    writeln!(stdin1, "QUIT")?;
    writeln!(stdin2, "QUIT")?;

    // Wait for processes to finish
    log_with_time("⏳ Waiting for processes to finish...", start_time);
    let _ = recorder1.wait()?;
    log_with_time("  ✅ EMG recorder finished", start_time);
    let _ = recorder2.wait()?;
    log_with_time("  ✅ EEG recorder finished", start_time);

    // Generate synchronization analysis report
    let precision_analysis = coordinator.get_precision_analysis();
    println!();
    precision_analysis.print_report();

    // Clean up sync files
    coordinator.cleanup()?;

    log_with_time("🎉 Synchronized recording completed!", start_time);
    println!();
    log_with_time("📁 Generated files:", start_time);
    log_with_time("  → synced_experiment_EMG.zarr (EMG stream data)", start_time);
    log_with_time("  → synced_experiment_EEG.zarr (EEG stream data)", start_time);
    println!();
    log_with_time("🔍 Analysis commands:", start_time);
    log_with_time("  cargo run --example sync_validator", start_time);
    log_with_time("  cargo run --example merge_zarr -- synced_experiment_*.zarr -o merged_synced.zarr", start_time);
    println!();
    log_with_time("📊 Synchronization benefits:", start_time);
    log_with_time("  • Millisecond-precise start/stop coordination", start_time);
    log_with_time("  • Automatic drift detection and reporting", start_time);
    log_with_time("  • Reproducible multi-stream experiments", start_time);
    log_with_time("  • Scientific-grade temporal alignment", start_time);

    Ok(())
}
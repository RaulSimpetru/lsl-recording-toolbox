use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Synchronization coordinator for multi-process recording
#[derive(Debug)]
pub struct SyncCoordinator {
    config: SyncConfig,
    coordinator_file: PathBuf,
    participant_id: String,
    state: SyncState,
}

#[derive(Debug, Clone)]
pub struct SyncConfig {
    pub base_dir: PathBuf,
    pub session_id: String,
    pub sync_timeout: Duration,
    pub poll_interval: Duration,
    pub precision_threshold: Duration, // Maximum acceptable time difference between processes
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            base_dir: PathBuf::from("."),
            session_id: "default_session".to_string(),
            sync_timeout: Duration::from_secs(30),
            poll_interval: Duration::from_millis(10),
            precision_threshold: Duration::from_millis(5), // 5ms precision
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncState {
    pub participants: Vec<ParticipantInfo>,
    pub coordinator_start_time: f64,
    pub global_start_signal: Option<f64>,
    pub global_stop_signal: Option<f64>,
    pub status: SyncStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticipantInfo {
    pub id: String,
    pub stream_name: String,
    pub ready_time: f64,
    pub start_confirmed: bool,
    pub stop_confirmed: bool,
    pub last_heartbeat: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SyncStatus {
    Initializing,
    WaitingForParticipants,
    ReadyToStart,
    Recording,
    Stopping,
    Completed,
    Error(String),
}

impl SyncCoordinator {
    /// Create a new synchronization coordinator
    pub fn new(config: SyncConfig, participant_id: String, stream_name: String) -> Result<Self> {
        let coordinator_file = config
            .base_dir
            .join(format!("sync_{}.json", config.session_id));

        // Initialize or load existing state
        let state = if coordinator_file.exists() {
            // Load existing state
            let mut file = File::open(&coordinator_file)?;
            let mut contents = String::new();
            file.read_to_string(&mut contents)?;

            let mut state: SyncState = serde_json::from_str(&contents)?;

            // Add or update this participant
            let participant = ParticipantInfo {
                id: participant_id.clone(),
                stream_name,
                ready_time: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64(),
                start_confirmed: false,
                stop_confirmed: false,
                last_heartbeat: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64(),
            };

            // Remove existing participant with same ID and add new one
            state.participants.retain(|p| p.id != participant_id);
            state.participants.push(participant);

            state
        } else {
            // Create new state
            let participant = ParticipantInfo {
                id: participant_id.clone(),
                stream_name,
                ready_time: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64(),
                start_confirmed: false,
                stop_confirmed: false,
                last_heartbeat: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64(),
            };

            SyncState {
                participants: vec![participant],
                coordinator_start_time: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64(),
                global_start_signal: None,
                global_stop_signal: None,
                status: SyncStatus::Initializing,
            }
        };

        let coordinator = Self {
            config,
            coordinator_file,
            participant_id,
            state,
        };

        coordinator.save_state()?;
        Ok(coordinator)
    }

    /// Wait for all expected participants to join
    pub fn wait_for_participants(&mut self, expected_participants: &[String]) -> Result<()> {
        println!("Waiting for participants: {:?}", expected_participants);

        let start_time = Instant::now();

        loop {
            self.load_state()?;
            self.update_heartbeat()?;

            // Check if all expected participants are present
            let present_participants: Vec<String> = self
                .state
                .participants
                .iter()
                .map(|p| p.stream_name.clone())
                .collect();

            let all_present = expected_participants
                .iter()
                .all(|expected| present_participants.contains(expected));

            if all_present {
                println!("All participants ready: {:?}", present_participants);
                self.state.status = SyncStatus::ReadyToStart;
                self.save_state()?;
                break;
            }

            // Check timeout
            if start_time.elapsed() > self.config.sync_timeout {
                let missing: Vec<String> = expected_participants
                    .iter()
                    .filter(|expected| !present_participants.contains(expected))
                    .cloned()
                    .collect();

                let error_msg = format!("Timeout waiting for participants. Missing: {:?}", missing);
                self.state.status = SyncStatus::Error(error_msg.clone());
                self.save_state()?;
                return Err(anyhow::anyhow!(error_msg));
            }

            std::thread::sleep(self.config.poll_interval);
        }

        Ok(())
    }

    /// Coordinate a synchronized start across all participants
    pub fn coordinate_start(&mut self) -> Result<f64> {
        println!("Coordinating synchronized start...");

        // Calculate start time in the future to give all processes time to prepare
        let preparation_time = Duration::from_millis(100); // 100ms preparation time
        let start_time = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64()
            + preparation_time.as_secs_f64();

        self.state.global_start_signal = Some(start_time);
        self.state.status = SyncStatus::Recording;
        self.save_state()?;

        println!("Start signal broadcasted for timestamp: {:.6}", start_time);

        // Wait for the coordinated start time
        let target_time = UNIX_EPOCH + Duration::from_secs_f64(start_time);
        let now = SystemTime::now();

        if target_time > now {
            let wait_duration = target_time.duration_since(now)?;
            std::thread::sleep(wait_duration);
        }

        // Confirm our participation in the start
        self.confirm_start()?;

        println!(
            "Synchronized start executed at timestamp: {:.6}",
            start_time
        );
        Ok(start_time)
    }

    /// Wait for a coordinated start signal from another coordinator
    pub fn wait_for_start_signal(&mut self) -> Result<f64> {
        println!("Waiting for start signal...");

        let start_time = Instant::now();

        loop {
            self.load_state()?;
            self.update_heartbeat()?;

            if let Some(start_signal_time) = self.state.global_start_signal {
                println!(
                    "Received start signal for timestamp: {:.6}",
                    start_signal_time
                );

                // Wait for the coordinated start time
                let target_time = UNIX_EPOCH + Duration::from_secs_f64(start_signal_time);
                let now = SystemTime::now();

                if target_time > now {
                    let wait_duration = target_time.duration_since(now)?;
                    std::thread::sleep(wait_duration);
                }

                // Confirm our participation
                self.confirm_start()?;

                println!(
                    "Synchronized start executed at timestamp: {:.6}",
                    start_signal_time
                );
                return Ok(start_signal_time);
            }

            // Check timeout
            if start_time.elapsed() > self.config.sync_timeout {
                let error_msg = "Timeout waiting for start signal".to_string();
                self.state.status = SyncStatus::Error(error_msg.clone());
                self.save_state()?;
                return Err(anyhow::anyhow!(error_msg));
            }

            std::thread::sleep(self.config.poll_interval);
        }
    }

    /// Coordinate a synchronized stop across all participants
    pub fn coordinate_stop(&mut self) -> Result<f64> {
        println!("Coordinating synchronized stop...");

        let stop_time = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64();
        self.state.global_stop_signal = Some(stop_time);
        self.state.status = SyncStatus::Stopping;
        self.save_state()?;

        self.confirm_stop()?;

        println!("Synchronized stop executed at timestamp: {:.6}", stop_time);
        Ok(stop_time)
    }

    /// Wait for a stop signal and execute synchronized stop
    pub fn wait_for_stop_signal(&mut self) -> Result<f64> {
        println!("Waiting for stop signal...");

        let start_time = Instant::now();

        loop {
            self.load_state()?;
            self.update_heartbeat()?;

            if let Some(stop_signal_time) = self.state.global_stop_signal {
                println!(
                    "Received stop signal for timestamp: {:.6}",
                    stop_signal_time
                );

                self.confirm_stop()?;

                println!(
                    "Synchronized stop executed at timestamp: {:.6}",
                    stop_signal_time
                );
                return Ok(stop_signal_time);
            }

            // Check timeout
            if start_time.elapsed() > self.config.sync_timeout {
                let error_msg = "Timeout waiting for stop signal".to_string();
                self.state.status = SyncStatus::Error(error_msg.clone());
                self.save_state()?;
                return Err(anyhow::anyhow!(error_msg));
            }

            std::thread::sleep(self.config.poll_interval);
        }
    }

    /// Confirm this participant has started recording
    fn confirm_start(&mut self) -> Result<()> {
        self.load_state()?;

        if let Some(participant) = self
            .state
            .participants
            .iter_mut()
            .find(|p| p.id == self.participant_id)
        {
            participant.start_confirmed = true;
        }

        self.save_state()
    }

    /// Confirm this participant has stopped recording
    fn confirm_stop(&mut self) -> Result<()> {
        self.load_state()?;

        if let Some(participant) = self
            .state
            .participants
            .iter_mut()
            .find(|p| p.id == self.participant_id)
        {
            participant.stop_confirmed = true;
        }

        self.save_state()
    }

    /// Update heartbeat for this participant
    fn update_heartbeat(&mut self) -> Result<()> {
        if let Some(participant) = self
            .state
            .participants
            .iter_mut()
            .find(|p| p.id == self.participant_id)
        {
            participant.last_heartbeat =
                SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64();
        }

        self.save_state()
    }

    /// Load state from file
    fn load_state(&mut self) -> Result<()> {
        if self.coordinator_file.exists() {
            let mut file = File::open(&self.coordinator_file)?;
            let mut contents = String::new();
            file.read_to_string(&mut contents)?;
            self.state = serde_json::from_str(&contents)?;
        }
        Ok(())
    }

    /// Save current state to file
    fn save_state(&self) -> Result<()> {
        let contents = serde_json::to_string_pretty(&self.state)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&self.coordinator_file)?;
        file.write_all(contents.as_bytes())?;
        file.flush()?;
        Ok(())
    }

    /// Clean up coordination files
    pub fn cleanup(&self) -> Result<()> {
        if self.coordinator_file.exists() {
            std::fs::remove_file(&self.coordinator_file)?;
            println!(
                "Cleaned up synchronization file: {}",
                self.coordinator_file.display()
            );
        }
        Ok(())
    }

    /// Get current synchronization status
    pub fn get_status(&self) -> &SyncStatus {
        &self.state.status
    }

    /// Get participant information
    pub fn get_participants(&self) -> &[ParticipantInfo] {
        &self.state.participants
    }

    /// Check if all participants have confirmed start
    pub fn all_started(&self) -> bool {
        !self.state.participants.is_empty()
            && self.state.participants.iter().all(|p| p.start_confirmed)
    }

    /// Check if all participants have confirmed stop
    pub fn all_stopped(&self) -> bool {
        !self.state.participants.is_empty()
            && self.state.participants.iter().all(|p| p.stop_confirmed)
    }

    /// Get synchronization precision analysis
    pub fn get_precision_analysis(&self) -> SyncPrecisionAnalysis {
        let mut analysis = SyncPrecisionAnalysis::default();

        if self.state.participants.len() < 2 {
            return analysis;
        }

        // Analyze ready time spread
        let ready_times: Vec<f64> = self
            .state
            .participants
            .iter()
            .map(|p| p.ready_time)
            .collect();
        analysis.ready_time_spread = ready_times
            .iter()
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap()
            - ready_times
                .iter()
                .min_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap();

        analysis.participant_count = self.state.participants.len();
        analysis.all_started = self.all_started();
        analysis.all_stopped = self.all_stopped();

        if let Some(start_time) = self.state.global_start_signal {
            analysis.start_signal_time = Some(start_time);
        }

        if let Some(stop_time) = self.state.global_stop_signal {
            analysis.stop_signal_time = Some(stop_time);

            if let Some(start_time) = analysis.start_signal_time {
                analysis.recording_duration = Some(stop_time - start_time);
            }
        }

        analysis
    }
}

#[derive(Debug, Default)]
pub struct SyncPrecisionAnalysis {
    pub participant_count: usize,
    pub ready_time_spread: f64,
    pub start_signal_time: Option<f64>,
    pub stop_signal_time: Option<f64>,
    pub recording_duration: Option<f64>,
    pub all_started: bool,
    pub all_stopped: bool,
}

impl SyncPrecisionAnalysis {
    pub fn print_report(&self) {
        println!("SYNCHRONIZATION PRECISION ANALYSIS");
        println!("===================================");
        println!("Participants:\t\t{}", self.participant_count);
        println!(
            "Ready time spread:\t{:.3} ms",
            self.ready_time_spread * 1000.0
        );

        if let Some(start_time) = self.start_signal_time {
            println!("Start signal:\t\t{:.6}", start_time);
        }

        if let Some(stop_time) = self.stop_signal_time {
            println!("Stop signal:\t\t{:.6}", stop_time);
        }

        if let Some(duration) = self.recording_duration {
            println!("Recording duration:\t{:.3} seconds", duration);
        }

        println!("All started:\t\t{}", self.all_started);
        println!("All stopped:\t\t{}", self.all_stopped);

        let precision_status = if self.ready_time_spread < 0.005 {
            // 5ms
            "EXCELLENT"
        } else if self.ready_time_spread < 0.010 {
            // 10ms
            "GOOD"
        } else if self.ready_time_spread < 0.050 {
            // 50ms
            "ACCEPTABLE"
        } else {
            "POOR"
        };

        println!("Precision rating:\t{}", precision_status);
    }
}

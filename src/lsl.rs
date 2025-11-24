use anyhow::Result;
use lsl::Pullable;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::{Duration, Instant};

use crate::cli::Args;
use crate::zarr::writer::ZarrWriter;
use crate::zarr::{open_or_create_zarr_store, setup_stream_arrays};

/// Resolve LSL stream with retry logic and random delays to avoid race conditions
pub fn resolve_lsl_stream_with_retry(
    source_id: &str,
    timeout: f64,
    quiet: bool,
    max_attempts: u32,
    base_delay_ms: u64,
) -> Result<Vec<lsl::StreamInfo>> {
    use std::time::Duration;

    if !quiet {
        println!("Resolving stream...");
    }

    for attempt in 0..max_attempts {
        // Add smart delay to reduce race conditions between multiple processes
        if attempt > 0 {
            let jitter = fastrand::u64(0..20); // Smaller jitter: 0-20ms
            let delay = Duration::from_millis(base_delay_ms + jitter);
            if !quiet {
                println!("Retrying stream resolution in {:?}...", delay);
            }
            std::thread::sleep(delay);
        }

        match lsl::resolve_byprop("source_id", source_id, 1, timeout) {
            Ok(streams) => {
                if !streams.is_empty() {
                    if !quiet && attempt > 0 {
                        println!("Successfully resolved stream on attempt {}", attempt + 1);
                    }
                    return Ok(streams);
                } else if !quiet {
                    println!("No streams found on attempt {} (will retry)", attempt + 1);
                }
            }
            Err(e) => {
                if attempt < max_attempts - 1 {
                    if !quiet {
                        println!(
                            "LSL resolution error on attempt {} (will retry): {}",
                            attempt + 1,
                            e
                        );
                    }
                } else {
                    return Err(anyhow::anyhow!(
                        "LSL error after {} attempts: {}",
                        max_attempts,
                        e
                    ));
                }
            }
        }
    }

    Err(anyhow::anyhow!(
        "No stream found with source_id={} after {} attempts",
        source_id,
        max_attempts
    ))
}

pub fn record_lsl_stream(params: RecordingParams) -> Result<()> {
    // Resolve stream with retry logic for robustness
    let res = resolve_lsl_stream_with_retry(
        params.source_id,
        params.resolution_config.timeout,
        params.quiet,
        params.resolution_config.max_retry_attempts,
        params.resolution_config.retry_base_delay_ms,
    )?;

    let inl = lsl::StreamInlet::new(&res[0], 300, 0, true)
        .map_err(|e| anyhow::anyhow!("LSL error: {}", e))?;
    let mut info = inl
        .info(lsl::FOREVER)
        .map_err(|e| anyhow::anyhow!("LSL error: {}", e))?;

    // Detect if this is an irregular stream (nominal_srate == 0)
    let is_irregular = info.nominal_srate() == 0.0;
    params.is_irregular_stream.store(is_irregular, Ordering::SeqCst);

    if !params.quiet {
        println!("Connected to stream with {} channels", info.channel_count());
        println!("Sample rate: {}", info.nominal_srate());
    }

    // Calculate optimal pull timeout based on stream frequency
    let pull_timeout = calculate_pull_timeout(
        &info,
        params.resolution_config.manual_pull_timeout,
        params.quiet,
    );

    inl.set_postprocessing(&[
        lsl::ProcessingOption::ClockSync,
        lsl::ProcessingOption::Dejitter,
        lsl::ProcessingOption::Monotonize,
        // lsl::ProcessingOption::Threadsafe,
    ])
    .map_err(|e| anyhow::anyhow!("LSL error: {}", e))?;

    // Initialize Zarr writer if config is provided
    let mut zarr_writer = if let Some(zarr_config) = params.zarr_config {
        initialize_zarr_writer(
            &zarr_config,
            &mut info,
            &inl,
            &params.recording_config,
            params.recorder_args,
            params.quiet,
        )?
    } else {
        None
    };

    // Create appropriate sample buffer based on channel format
    let mut sample_buffer = create_sample_buffer(&info)?;

    let mut sample_count: u64 = 0;
    let mut memory_monitor = MemoryMonitor::new(params.recorder_args.memory_monitor);
    let mut first_timestamp: Option<f64> = None;
    let mut last_timestamp: Option<f64> = None;

    loop {
        if params.quit.load(Ordering::SeqCst) {
            break;
        }

        if params.recording.load(Ordering::SeqCst) {
            macro_rules! pull_and_record {
                ($buf:expr, $method:ident) => {{
                    // Clear buffer and reuse capacity
                    $buf.clear();
                    let ts = inl
                        .pull_sample_buf($buf, pull_timeout)
                        .map_err(|e| anyhow::anyhow!("LSL error: {}", e))?;
                    if ts != 0.0 {
                        if let Some(ref mut writer) = zarr_writer {
                            // Pass data by slice reference to avoid full clone
                            writer.$method(&$buf, ts);
                        }
                    }
                    ts
                }};
            }

            let ts = match &mut sample_buffer {
                SampleBuffer::Float32(buf) => pull_and_record!(buf, add_sample_slice_f32),
                SampleBuffer::Float64(buf) => pull_and_record!(buf, add_sample_slice_f64),
                SampleBuffer::Int32(buf) => pull_and_record!(buf, add_sample_slice_i32),
                SampleBuffer::Int16(buf) => pull_and_record!(buf, add_sample_slice_i16),
                SampleBuffer::Int8(buf) => pull_and_record!(buf, add_sample_slice_i8),
                SampleBuffer::String(buf) => {
                    // String streams require special handling - use pull_sample() instead of pull_sample_buf()
                    // pull_sample_buf() doesn't work correctly with Vec<String>
                    match <lsl::StreamInlet as Pullable<String>>::pull_sample(&inl, pull_timeout) {
                        Ok((sample_data, ts)) => {
                            if ts != 0.0 {
                                *buf = sample_data; // Update the buffer with the pulled data
                                if let Some(ref mut writer) = zarr_writer {
                                    writer.add_sample_slice_string(&buf, ts);
                                }
                            }
                            ts
                        }
                        Err(e) => {
                            // Log error but don't fail - string streams may have no data
                            if !params.quiet {
                                eprintln!("Warning: Failed to pull string sample: {}", e);
                            }
                            0.0
                        }
                    }
                }
            };

            if ts != 0.0 {
                sample_count += 1;
                last_timestamp = Some(ts);  // Track last timestamp

                // Signal first sample pulled for STOP_AFTER timer
                if sample_count == 1 {
                    first_timestamp = Some(ts);  // Track first timestamp
                    params.first_sample_pulled.store(true, Ordering::SeqCst);

                    // Report to parent (lsl-multi-recorder) that first sample is pulled
                    let stream_type = if params.is_irregular_stream.load(Ordering::SeqCst) {
                        "irregular"
                    } else {
                        "regular"
                    };
                    if !params.quiet {
                        println!("STATUS FIRST_SAMPLE ({})", stream_type);
                        std::io::stdout().flush().ok();
                    }
                }

                // Check if we should flush (buffer size or time-based)
                if let Some(ref mut writer) = zarr_writer
                    && writer.needs_flush() {
                        writer.flush()?;
                    }

                // Memory monitoring report
                memory_monitor.maybe_report(sample_count, &zarr_writer, params.quiet);
            }
        } else {
            thread::sleep(Duration::from_millis(50));
        }
    }

    // Final flush for any remaining samples
    if let Some(ref mut writer) = zarr_writer {
        writer.flush()?;

        // Update final recording metadata with first and last timestamps
        // Note: requested duration is already in recorder_config.duration
        writer.finalize_recording_metadata(first_timestamp, last_timestamp)?;
    }

    if !params.quiet {
        println!("Recording stopped. Total samples: {}", sample_count);
    }
    Ok(())
}

/// Configuration for recording behavior (buffering and flushing)
#[derive(Debug, Clone)]
pub struct RecordingConfig {
    pub flush_interval: Duration,
    pub flush_buffer_size: usize,
    pub immediate_flush: bool,
}

impl Default for RecordingConfig {
    fn default() -> Self {
        Self {
            flush_interval: Duration::from_secs(1),
            flush_buffer_size: 50,
            immediate_flush: false,
        }
    }
}

/// Configuration for Zarr output
#[derive(Debug, Clone)]
pub struct ZarrConfig {
    pub store_path: PathBuf,
    pub stream_name: String,
    pub subject: Option<String>,
    pub session_id: Option<String>,
    pub notes: Option<String>,
}

/// Stream resolution and retry configuration
#[derive(Debug, Clone)]
pub struct StreamResolutionConfig {
    pub timeout: f64,
    pub max_retry_attempts: u32,
    pub retry_base_delay_ms: u64,
    pub manual_pull_timeout: Option<f64>,
}

impl Default for StreamResolutionConfig {
    fn default() -> Self {
        Self {
            timeout: 5.0,
            max_retry_attempts: 3,
            retry_base_delay_ms: 100,
            manual_pull_timeout: None,
        }
    }
}

/// Complete parameters for LSL stream recording
pub struct RecordingParams<'a> {
    pub source_id: &'a str,
    pub recording: Arc<AtomicBool>,
    pub quit: Arc<AtomicBool>,
    pub first_sample_pulled: Arc<AtomicBool>,
    pub is_irregular_stream: Arc<AtomicBool>,
    pub quiet: bool,
    pub zarr_config: Option<ZarrConfig>,
    pub recording_config: RecordingConfig,
    pub resolution_config: StreamResolutionConfig,
    pub recorder_args: &'a Args,
}

/// Sample buffer for different LSL channel formats
pub enum SampleBuffer {
    Float32(Vec<f32>),
    Float64(Vec<f64>),
    Int32(Vec<i32>),
    Int16(Vec<i16>),
    Int8(Vec<i8>),
    String(Vec<String>),
}

/// Calculate optimal pull timeout based on stream sample rate
fn calculate_pull_timeout(
    info: &lsl::StreamInfo,
    manual_override: Option<f64>,
    quiet: bool,
) -> f64 {
    if let Some(manual_timeout) = manual_override {
        if !quiet {
            println!("Using manual pull timeout: {:.3}s", manual_timeout);
        }
        return manual_timeout;
    }

    if info.nominal_srate() > 0.0 {
        // Wait for 2-3 sample periods to balance responsiveness vs efficiency
        // Min 5ms (for >500Hz), Max 100ms (for <25Hz)
        let calculated = (2.5 / info.nominal_srate()).clamp(0.005, 0.1);
        if !quiet {
            println!(
                "Calculated pull timeout: {:.3}s (based on {:.1}Hz)",
                calculated,
                info.nominal_srate()
            );
        }
        calculated
    } else {
        // Default for irregular/unknown rate streams
        if !quiet {
            println!("Using default pull timeout: 0.1s (irregular/unknown rate stream)");
        }
        0.1
    }
}

/// Create sample buffer appropriate for the stream's channel format
fn create_sample_buffer(info: &lsl::StreamInfo) -> Result<SampleBuffer> {
    let channel_count = info.channel_count() as usize;
    let channel_format = info.channel_format();

    let buffer = match channel_format {
        lsl::ChannelFormat::Float32 => SampleBuffer::Float32(Vec::with_capacity(channel_count)),
        lsl::ChannelFormat::Double64 => SampleBuffer::Float64(Vec::with_capacity(channel_count)),
        lsl::ChannelFormat::Int32 => SampleBuffer::Int32(Vec::with_capacity(channel_count)),
        lsl::ChannelFormat::Int16 => SampleBuffer::Int16(Vec::with_capacity(channel_count)),
        lsl::ChannelFormat::Int8 => SampleBuffer::Int8(Vec::with_capacity(channel_count)),
        lsl::ChannelFormat::String => SampleBuffer::String(Vec::with_capacity(channel_count)),
        _ => {
            return Err(anyhow::anyhow!(
                "Unsupported channel format: {:?}",
                channel_format
            ));
        }
    };

    Ok(buffer)
}

/// Helper for monitoring memory usage during recording
struct MemoryMonitor {
    last_report: Option<Instant>,
}

impl MemoryMonitor {
    fn new(enabled: bool) -> Self {
        Self {
            last_report: if enabled { Some(Instant::now()) } else { None },
        }
    }

    fn maybe_report(
        &mut self,
        sample_count: u64,
        zarr_writer: &Option<ZarrWriter>,
        quiet: bool,
    ) {
        if let Some(ref mut last_report) = self.last_report {
            if last_report.elapsed() >= Duration::from_secs(10) {
                let buffer_samples = if let Some(writer) = zarr_writer {
                    writer.buffer_sample_count()
                } else {
                    0
                };

                println!(
                    "Memory status:\t{} samples recorded, {} buffered samples, buffer usage: {:.1}%",
                    sample_count,
                    buffer_samples,
                    if let Some(writer) = zarr_writer {
                        (buffer_samples as f64 / writer.buffer_capacity() as f64) * 100.0
                    } else {
                        0.0
                    }
                );
                *last_report = Instant::now();
            }
        } else if !quiet && sample_count % 100 == 0 {
            println!("Recorded {} samples", sample_count);
        }
    }
}

/// Initialize Zarr writer with all necessary configuration
fn initialize_zarr_writer(
    config: &ZarrConfig,
    info: &mut lsl::StreamInfo,
    inl: &lsl::StreamInlet,
    recording_config: &RecordingConfig,
    recorder_args: &Args,
    quiet: bool,
) -> Result<Option<ZarrWriter>> {
    if !quiet {
        println!("Initializing Zarr store: {:?}", config.store_path);
        println!("Stream group: {}", config.stream_name);
    }

    let store = open_or_create_zarr_store(
        &config.store_path,
        config.subject.as_deref(),
        config.session_id.as_deref(),
        config.notes.as_deref(),
    )?;

    // Get LSL time correction for sync metadata
    let time_correction = inl
        .time_correction(lsl::FOREVER)
        .map_err(|e| anyhow::anyhow!("LSL error getting time correction: {}", e))?;

    let channel_format = info.channel_format();
    let recording_start_time = chrono::Utc::now().to_rfc3339();
    let recorder_config_json =
        recorder_args.to_recorder_config_json(Some(recording_start_time))?;

    let (data_array, time_array) = setup_stream_arrays(
        &store,
        &config.stream_name,
        info,
        channel_format,
        &recorder_config_json,
        time_correction,
        None, // first_timestamp will be updated after first sample
    )?;

    let buffer_size = if recording_config.immediate_flush {
        1
    } else {
        // Adaptive buffer sizing based on stream rate - aim for ~1 second of data
        let adaptive_size = if info.nominal_srate() > 0.0 {
            // Target 1 second of buffering, but clamp to reasonable bounds
            let target_buffer_time_secs = 1.0;
            let calculated_size = (info.nominal_srate() * target_buffer_time_secs) as usize;
            // Clamp between 10 samples (very low rate) and 2000 samples (very high rate)
            calculated_size.clamp(10, 2000)
        } else {
            recording_config.flush_buffer_size // Unknown rate, use default
        };

        if !quiet {
            println!(
                "Using adaptive buffer size: {} samples for {:.1}Hz stream",
                adaptive_size,
                info.nominal_srate()
            );
        }
        adaptive_size
    };

    Ok(Some(ZarrWriter::new(
        data_array,
        time_array,
        buffer_size,
        channel_format,
        recording_config.flush_interval,
        config.store_path.clone(),
        store,
        config.stream_name.clone(),
    )?))
}

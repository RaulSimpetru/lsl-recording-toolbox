use anyhow::Result;
use lsl::Pullable;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::{Duration, Instant};

use crate::cli::Args;
use crate::hdf5::writer::Hdf5Writer;
use crate::hdf5::{open_or_create_hdf5_file, setup_stream_group};

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

        match lsl::resolve_byprop("source_id", &source_id, 1, timeout) {
            Ok(streams) => {
                if !streams.is_empty() {
                    if !quiet && attempt > 0 {
                        println!("Successfully resolved stream on attempt {}", attempt + 1);
                    }
                    return Ok(streams);
                } else {
                    if !quiet {
                        println!("No streams found on attempt {} (will retry)", attempt + 1);
                    }
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

pub fn record_lsl_stream(
    source_id: &str,
    timeout: f64,
    recording: Arc<AtomicBool>,
    quit: Arc<AtomicBool>,
    quiet: bool,
    hdf5_config: Option<(
        PathBuf,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    )>,
    flush_interval: Duration,
    flush_buffer_size: usize,
    immediate_flush: bool,
    max_retry_attempts: u32,
    retry_base_delay_ms: u64,
    manual_pull_timeout: Option<f64>,
    recorder_args: &Args,
) -> Result<()> {
    // Resolve stream with retry logic for robustness
    let res = resolve_lsl_stream_with_retry(
        source_id,
        timeout,
        quiet,
        max_retry_attempts,
        retry_base_delay_ms,
    )?;

    let inl = lsl::StreamInlet::new(&res[0], 300, 0, true)
        .map_err(|e| anyhow::anyhow!("LSL error: {}", e))?;
    let info = inl
        .info(lsl::FOREVER)
        .map_err(|e| anyhow::anyhow!("LSL error: {}", e))?;

    if !quiet {
        println!("Connected to stream with {} channels", info.channel_count());
        println!("Sample rate: {}", info.nominal_srate());
    }

    // Calculate optimal pull timeout based on stream frequency
    let pull_timeout = if let Some(manual_timeout) = manual_pull_timeout {
        // User override
        if !quiet {
            println!("Using manual pull timeout: {:.3}s", manual_timeout);
        }
        manual_timeout
    } else if info.nominal_srate() > 0.0 {
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
    };

    inl.set_postprocessing(&[
        lsl::ProcessingOption::ClockSync,
        lsl::ProcessingOption::Dejitter,
        lsl::ProcessingOption::Monotonize,
        // lsl::ProcessingOption::Threadsafe,
    ])
    .map_err(|e| anyhow::anyhow!("LSL error: {}", e))?;

    // Initialize HDF5 writer if config is provided
    let mut hdf5_writer =
        if let Some((file_path, stream_name, subject, session_id, notes)) = hdf5_config {
            if !quiet {
                println!("Initializing HDF5 file: {:?}", file_path);
                println!("Stream group: {}", stream_name);
            }

            let file = open_or_create_hdf5_file(
                &file_path,
                subject.as_deref(),
                session_id.as_deref(),
                notes.as_deref(),
            )?;

            let channel_format = info.channel_format();
            let recording_start_time = chrono::Utc::now().to_rfc3339();
            let recorder_config_json =
                recorder_args.to_recorder_config_json(Some(recording_start_time))?;
            let (_group, data_dataset, time_dataset) = setup_stream_group(
                &file,
                &stream_name,
                &info,
                channel_format,
                &recorder_config_json,
            )?;

            let buffer_size = if immediate_flush {
                1
            } else {
                // Adaptive buffer sizing based on stream rate - aim for ~1 second of data
                let adaptive_size = if info.nominal_srate() > 0.0 {
                    // Target 1 second of buffering, but clamp to reasonable bounds
                    // This ensures consistent memory usage regardless of sample rate
                    let target_buffer_time_secs = 1.0;
                    let calculated_size = (info.nominal_srate() * target_buffer_time_secs) as usize;

                    // Clamp between 10 samples (very low rate) and 2000 samples (very high rate)
                    calculated_size.clamp(10, 2000)
                } else {
                    flush_buffer_size // Unknown rate, use default
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
            Some(Hdf5Writer::new(
                data_dataset,
                time_dataset,
                buffer_size,
                channel_format,
                flush_interval,
            )?)
        } else {
            None
        };

    // Create appropriate sample buffer based on channel format
    let channel_format = info.channel_format();

    // Create single sample buffer for the detected type - pre-allocated with channel capacity
    enum SampleBuffer {
        Float32(Vec<f32>),
        Float64(Vec<f64>),
        Int32(Vec<i32>),
        Int16(Vec<i16>),
        Int8(Vec<i8>),
        String(Vec<String>),
    }

    let channel_count = info.channel_count() as usize;
    let mut sample_buffer = match channel_format {
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

    let mut sample_count: u64 = 0;
    let mut last_memory_report = if recorder_args.memory_monitor {
        Some(Instant::now())
    } else {
        None
    };

    loop {
        if quit.load(Ordering::SeqCst) {
            break;
        }

        if recording.load(Ordering::SeqCst) {
            macro_rules! pull_and_record {
                ($buf:expr, $method:ident) => {{
                    // Clear buffer and reuse capacity
                    $buf.clear();
                    let ts = inl
                        .pull_sample_buf($buf, pull_timeout)
                        .map_err(|e| anyhow::anyhow!("LSL error: {}", e))?;
                    if ts != 0.0 {
                        if let Some(ref mut writer) = hdf5_writer {
                            // Pass data by slice reference to avoid full clone
                            writer.$method(&$buf, ts);
                        }
                    }
                    ts
                }};
            }

            let ts = match &mut sample_buffer {
                SampleBuffer::Float32(ref mut buf) => pull_and_record!(buf, add_sample_slice_f32),
                SampleBuffer::Float64(ref mut buf) => pull_and_record!(buf, add_sample_slice_f64),
                SampleBuffer::Int32(ref mut buf) => pull_and_record!(buf, add_sample_slice_i32),
                SampleBuffer::Int16(ref mut buf) => pull_and_record!(buf, add_sample_slice_i16),
                SampleBuffer::Int8(ref mut buf) => pull_and_record!(buf, add_sample_slice_i8),
                SampleBuffer::String(ref mut buf) => pull_and_record!(buf, add_sample_slice_string),
            };

            if ts != 0.0 {
                sample_count += 1;

                // Check if we should flush (buffer size or time-based)
                if let Some(ref mut writer) = hdf5_writer {
                    if writer.needs_flush() {
                        writer.flush()?;
                    }
                }

                // Memory monitoring report every 10 seconds
                if let Some(ref mut last_report) = last_memory_report {
                    if last_report.elapsed() >= Duration::from_secs(10) {
                        let buffer_samples = if let Some(ref writer) = hdf5_writer {
                            writer.buffer_sample_count()
                        } else {
                            0
                        };

                        println!(
                            "Memory status:\t{} samples recorded, {} buffered samples, buffer usage: {:.1}%",
                            sample_count,
                            buffer_samples,
                            if let Some(ref writer) = hdf5_writer {
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
        } else {
            thread::sleep(Duration::from_millis(50));
        }
    }

    // Final flush for any remaining samples
    if let Some(ref mut writer) = hdf5_writer {
        writer.flush()?;
    }

    if !quiet {
        println!("Recording stopped. Total samples: {}", sample_count);
    }
    Ok(())
}

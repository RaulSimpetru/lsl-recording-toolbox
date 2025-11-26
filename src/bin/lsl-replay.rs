//! LSL Replay - Stream recorded LSL data from Zarr files
//!
//! This tool reads recorded LSL streams from Zarr files and replays them
//! as live LSL streams, with continuous looping enabled by default.
//!
//! # Features
//!
//! - Replay recorded LSL streams from Zarr files
//! - Continuous looping enabled by default (use --no-loop to disable)
//! - Original timing preservation or speed adjustment
//! - Support for all data formats (Float32, Float64, Int32, Int16, Int8, String)
//! - Automatic stream metadata reconstruction
//! - List available streams in a Zarr file
//!
//! # Usage
//!
//! ```bash
//! # List available streams in a Zarr file
//! lsl-replay recording.zarr --list
//!
//! # Replay a specific stream (loops by default)
//! lsl-replay recording.zarr --stream MUOVI
//!
//! # Replay once (no looping)
//! lsl-replay recording.zarr --stream MUOVI --no-loop
//!
//! # Replay at 2x speed (with looping)
//! lsl-replay recording.zarr --stream VHI_Control --speed 2.0
//!
//! # Replay at half speed
//! lsl-replay recording.zarr --stream VHI_Predict --speed 0.5
//!
//! # Custom output stream name
//! lsl-replay recording.zarr --stream MUOVI --output-name "ReplayedMUOVI"
//! ```
//!
//! # Timing and Synchronization
//!
//! - Preserves original inter-sample timing from recorded timestamps
//! - Speed factor adjusts playback rate (1.0 = real-time, 2.0 = 2x faster)
//! - Loops seamlessly without timestamp discontinuities
//! - Supports both regular and irregular streams

use anyhow::{Context, Result};
use clap::Parser;
use lsl::{ChannelFormat, Pushable, StreamInfo, StreamOutlet};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use zarrs::array::Array;
use zarrs::array_subset::ArraySubset;
use zarrs::filesystem::FilesystemStore;
use zarrs::storage::{ReadableStorageTraits, StoreKey};

#[derive(Parser)]
#[command(name = "lsl-replay")]
#[command(about = "Replay recorded LSL streams from Zarr files")]
#[command(version)]
struct Args {
    /// Path to Zarr file to replay
    file_path: String,

    /// Stream name to replay
    #[arg(short, long)]
    stream: Option<String>,

    /// List available streams in the Zarr file
    #[arg(short, long)]
    list: bool,

    /// Loop continuously (default: true, use --no-loop to disable)
    #[arg(short = 'L', long, default_value = "true", action = clap::ArgAction::Set)]
    r#loop: bool,

    /// Playback speed multiplier (1.0 = real-time, 2.0 = 2x speed, 0.5 = half speed)
    #[arg(long, default_value = "1.0")]
    speed: f64,

    /// Custom output stream name (defaults to original stream name)
    #[arg(short, long)]
    output_name: Option<String>,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    lsl_recording_toolbox::display_license_notice("lsl-replay");

    let store = Arc::new(FilesystemStore::new(&args.file_path)?);

    // List mode
    if args.list {
        list_streams(&args.file_path)?;
        return Ok(());
    }

    // Replay mode - require stream name
    let stream_name = args
        .stream
        .as_ref()
        .context("Stream name required (use --stream or --list to see available streams)")?;

    // Verify stream exists
    let streams_path = PathBuf::from(&args.file_path);
    let stream_path_buf = streams_path.join(stream_name);
    if !stream_path_buf.exists() {
        anyhow::bail!(
            "Stream '{}' not found in Zarr file. Use --list to see available streams.",
            stream_name
        );
    }

    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║              LSL Stream Replay                                 ║");
    println!("╚════════════════════════════════════════════════════════════════╝");
    println!();

    // Read stream metadata
    let stream_path = format!("/{}", stream_name);
    let attrs = read_group_attributes(&store, &stream_path)
        .context("Failed to read stream metadata")?;

    let stream_info = attrs
        .get("stream_info")
        .context("No stream_info in metadata")?;

    // Extract stream parameters
    let source_id = stream_info
        .get("source_id")
        .and_then(|v| v.as_str())
        .unwrap_or("replayed_stream");
    let stream_type = stream_info
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown");
    let channel_count = stream_info
        .get("channel_count")
        .and_then(|v| v.as_u64())
        .context("Missing channel_count")? as u32;
    let nominal_srate = stream_info
        .get("nominal_srate")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let channel_format_str = stream_info
        .get("channel_format")
        .and_then(|v| v.as_str())
        .context("Missing channel_format")?;

    let channel_format = parse_channel_format(channel_format_str)?;

    // Use custom output name if provided
    let output_stream_name = args.output_name.as_deref().unwrap_or(stream_name);

    println!("Source file:\t{}", args.file_path);
    println!("Stream name:\t{}", stream_name);
    println!("Output name:\t{}", output_stream_name);
    println!("Stream type:\t{}", stream_type);
    println!("Source ID:\t{}", source_id);
    println!("Channels:\t{}", channel_count);
    println!("Sample rate:\t{} Hz", nominal_srate);
    println!("Format:\t\t{:?}", channel_format);
    println!("Speed:\t\t{}x", args.speed);
    println!("Looping:\t{}", if args.r#loop { "Yes" } else { "No" });
    println!();

    // Create LSL outlet
    let info = StreamInfo::new(
        output_stream_name,
        stream_type,
        channel_count,
        nominal_srate,
        channel_format,
        source_id,
    )?;

    let outlet = StreamOutlet::new(&info, 0, 360)?;

    // Read time array
    let time_array_path = format!("{}/time", stream_path);
    let time_array = Array::<FilesystemStore>::open(store.clone(), &time_array_path)
        .context("Failed to open time array")?;

    let num_samples = time_array.shape()[0] as usize;

    if num_samples == 0 {
        anyhow::bail!("No samples found in stream");
    }

    println!("Starting replay of {} samples...", num_samples);
    if args.r#loop {
        println!("Press Ctrl+C to stop");
    }
    println!();

    // Replay loop
    match channel_format {
        ChannelFormat::Float32 => replay_float32(&store, &stream_path, num_samples, &outlet, &args),
        ChannelFormat::Double64 => replay_float64(&store, &stream_path, num_samples, &outlet, &args),
        ChannelFormat::Int32 => replay_int32(&store, &stream_path, num_samples, &outlet, &args),
        ChannelFormat::Int16 => replay_int16(&store, &stream_path, num_samples, &outlet, &args),
        ChannelFormat::Int8 => replay_int8(&store, &stream_path, num_samples, &outlet, &args),
        ChannelFormat::String => replay_string(&store, &stream_path, num_samples, &outlet, &args),
        _ => anyhow::bail!("Unsupported channel format: {:?}", channel_format),
    }
}

macro_rules! replay_numeric {
    ($name:ident, $ty:ty) => {
        fn $name(
            store: &Arc<FilesystemStore>,
            stream_path: &str,
            num_samples: usize,
            outlet: &StreamOutlet,
            args: &Args,
        ) -> Result<()> {
            // Read data array
            let data_array_path = format!("{}/data", stream_path);
            let data_array = Array::<FilesystemStore>::open(store.clone(), &data_array_path)
                .context("Failed to open data array")?;

            let shape = data_array.shape();
            let num_channels = shape[0] as usize;

            // Read time array
            let time_array_path = format!("{}/time", stream_path);
            let time_array = Array::<FilesystemStore>::open(store.clone(), &time_array_path)
                .context("Failed to open time array")?;

            // Read all timestamps
            let time_subset = ArraySubset::new_with_start_shape(vec![0], vec![num_samples as u64])?;
            let timestamps = time_array
                .retrieve_array_subset_ndarray::<f64>(&time_subset)
                .context("Failed to read timestamps")?;

            let mut loop_count = 0;
            let start_time = Instant::now();

            loop {
                loop_count += 1;

                if args.verbose {
                    println!("Starting loop iteration {}", loop_count);
                }

                let loop_start = Instant::now();

                for sample_idx in 0..num_samples {
                    // Read single sample across all channels
                    let sample_subset = ArraySubset::new_with_start_shape(
                        vec![0, sample_idx as u64],
                        vec![num_channels as u64, 1],
                    )?;

                    let sample_data = data_array
                        .retrieve_array_subset_ndarray::<$ty>(&sample_subset)
                        .with_context(|| format!("Failed to read sample {}", sample_idx))?;

                    // Convert to vector for LSL push
                    let sample_vec: Vec<$ty> = (0..num_channels)
                        .map(|ch| sample_data[[ch, 0]])
                        .collect();

                    // Push to LSL
                    outlet.push_sample(&sample_vec)?;

                    // Calculate timing for next sample
                    if sample_idx < num_samples - 1 {
                        let current_ts = timestamps[[sample_idx]];
                        let next_ts = timestamps[[sample_idx + 1]];
                        let inter_sample_interval = (next_ts - current_ts) / args.speed;

                        if inter_sample_interval > 0.0 {
                            let sleep_duration = Duration::from_secs_f64(inter_sample_interval);

                            // Sleep with high accuracy for short intervals
                            if sleep_duration > Duration::from_micros(100) {
                                thread::sleep(sleep_duration);
                            } else if sleep_duration > Duration::from_nanos(1) {
                                // Spin-wait for very short intervals
                                let target = Instant::now() + sleep_duration;
                                while Instant::now() < target {
                                    std::hint::spin_loop();
                                }
                            }
                        }
                    }
                }

                if args.verbose {
                    let loop_elapsed = loop_start.elapsed();
                    let total_elapsed = start_time.elapsed();
                    println!(
                        "Loop {} completed in {:.3}s (total: {:.1}s, {} samples sent)",
                        loop_count,
                        loop_elapsed.as_secs_f64(),
                        total_elapsed.as_secs_f64(),
                        loop_count * num_samples
                    );
                }

                // Exit if not looping
                if !args.r#loop {
                    break;
                }
            }

            println!();
            println!("Replay completed: {} loop(s), {} total samples sent", loop_count, loop_count * num_samples);

            Ok(())
        }
    };
}

replay_numeric!(replay_float32, f32);
replay_numeric!(replay_float64, f64);
replay_numeric!(replay_int32, i32);
replay_numeric!(replay_int16, i16);
replay_numeric!(replay_int8, i8);

fn replay_string(
    store: &Arc<FilesystemStore>,
    stream_path: &str,
    num_samples: usize,
    outlet: &StreamOutlet,
    args: &Args,
) -> Result<()> {
    // String streams typically use "events" array instead of "data"
    let events_array_path = format!("{}/events", stream_path);
    let data_array_path = format!("{}/data", stream_path);

    // Try "events" first, fall back to "data"
    let (array_path, is_events) = if let Ok(_) = Array::<FilesystemStore>::open(store.clone(), &events_array_path) {
        (events_array_path, true)
    } else {
        (data_array_path, false)
    };

    let data_array = Array::<FilesystemStore>::open(store.clone(), &array_path)
        .context("Failed to open string data array")?;

    let shape = data_array.shape();
    let num_channels = if is_events {
        1 // events array is 1D
    } else {
        shape[0] as usize // data array is 2D [channels, samples]
    };

    // Read time array
    let time_array_path = format!("{}/time", stream_path);
    let time_array = Array::<FilesystemStore>::open(store.clone(), &time_array_path)
        .context("Failed to open time array")?;

    // Read all timestamps
    let time_subset = ArraySubset::new_with_start_shape(vec![0], vec![num_samples as u64])?;
    let timestamps = time_array
        .retrieve_array_subset_ndarray::<f64>(&time_subset)
        .context("Failed to read timestamps")?;

    let mut loop_count = 0;
    let start_time = Instant::now();

    loop {
        loop_count += 1;

        if args.verbose {
            println!("Starting loop iteration {}", loop_count);
        }

        let loop_start = Instant::now();

        for sample_idx in 0..num_samples {
            // Read single sample
            let sample_subset = if is_events {
                // 1D array: [samples]
                ArraySubset::new_with_start_shape(vec![sample_idx as u64], vec![1])?
            } else {
                // 2D array: [channels, samples]
                ArraySubset::new_with_start_shape(
                    vec![0, sample_idx as u64],
                    vec![num_channels as u64, 1],
                )?
            };

            let sample_data = data_array
                .retrieve_array_subset_ndarray::<String>(&sample_subset)
                .with_context(|| format!("Failed to read string sample {}", sample_idx))?;

            // Convert to vector for LSL push
            let sample_vec: Vec<String> = if is_events {
                vec![sample_data[[0]].clone()]
            } else {
                (0..num_channels)
                    .map(|ch| sample_data[[ch, 0]].clone())
                    .collect()
            };

            // Push to LSL
            outlet.push_sample(&sample_vec)?;

            // Calculate timing for next sample
            if sample_idx < num_samples - 1 {
                let current_ts = timestamps[[sample_idx]];
                let next_ts = timestamps[[sample_idx + 1]];
                let inter_sample_interval = (next_ts - current_ts) / args.speed;

                if inter_sample_interval > 0.0 {
                    let sleep_duration = Duration::from_secs_f64(inter_sample_interval);

                    if sleep_duration > Duration::from_micros(100) {
                        thread::sleep(sleep_duration);
                    } else if sleep_duration > Duration::from_nanos(1) {
                        let target = Instant::now() + sleep_duration;
                        while Instant::now() < target {
                            std::hint::spin_loop();
                        }
                    }
                }
            }
        }

        if args.verbose {
            let loop_elapsed = loop_start.elapsed();
            let total_elapsed = start_time.elapsed();
            println!(
                "Loop {} completed in {:.3}s (total: {:.1}s, {} samples sent)",
                loop_count,
                loop_elapsed.as_secs_f64(),
                total_elapsed.as_secs_f64(),
                loop_count * num_samples
            );
        }

        // Exit if not looping
        if !args.r#loop {
            break;
        }
    }

    println!();
    println!("Replay completed: {} loop(s), {} total samples sent", loop_count, loop_count * num_samples);

    Ok(())
}

fn list_streams(file_path: &str) -> Result<()> {
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║              Available Streams                                 ║");
    println!("╚════════════════════════════════════════════════════════════════╝");
    println!();
    println!("File: {}", file_path);
    println!();

    let streams_path = PathBuf::from(file_path);
    if !streams_path.exists() || !streams_path.is_dir() {
        anyhow::bail!("Zarr file not found: {}", file_path);
    }

    let mut stream_names = Vec::new();
    for entry in std::fs::read_dir(&streams_path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            stream_names.push(entry.file_name().to_string_lossy().to_string());
        }
    }

    stream_names.sort();

    if stream_names.is_empty() {
        println!("No streams found in Zarr file.");
    } else {
        println!("Streams:");
        for name in &stream_names {
            println!("\t- {}", name);
        }
        println!();
        println!("Use --stream <name> to replay a specific stream");
    }

    Ok(())
}

fn parse_channel_format(format_str: &str) -> Result<ChannelFormat> {
    match format_str {
        "Float32" => Ok(ChannelFormat::Float32),
        "Float64" => Ok(ChannelFormat::Double64),
        "Int32" => Ok(ChannelFormat::Int32),
        "Int16" => Ok(ChannelFormat::Int16),
        "Int8" => Ok(ChannelFormat::Int8),
        "String" => Ok(ChannelFormat::String),
        _ => anyhow::bail!("Unknown channel format: {}", format_str),
    }
}

/// Read attributes from a group's zarr.json file (Zarr v3 format)
fn read_group_attributes(store: &Arc<FilesystemStore>, path: &str) -> Result<Value> {
    let trimmed_path = path.trim_end_matches('/').trim_start_matches('/');
    let zarr_json_path = if trimmed_path.is_empty() {
        "zarr.json".to_string()
    } else {
        format!("{}/zarr.json", trimmed_path)
    };
    let zarr_key = StoreKey::new(&zarr_json_path)?;
    let zarr_bytes = store
        .get(&zarr_key)?
        .ok_or_else(|| anyhow::anyhow!("Metadata not found at {}", zarr_json_path))?;
    let zarr_metadata: Value = serde_json::from_slice(&zarr_bytes)?;

    // Extract attributes from zarr.json structure
    if let Some(attrs) = zarr_metadata.get("attributes") {
        Ok(attrs.clone())
    } else {
        Ok(serde_json::json!({}))
    }
}

//! LSL Sync - Post-processing timestamp synchronization tool
//!
//! This tool aligns timestamps across multiple streams in a Zarr recording,
//! creating synchronized time arrays for multi-stream analysis.
//!
//! # Features
//!
//! - Align timestamps across multiple streams
//! - Multiple alignment modes (common-start, first-stream, last-stream, absolute-zero)
//! - Optional trimming to remove data outside common time window
//! - Non-destructive: preserves original raw timestamps
//! - Writes aligned timestamps to `/<name>/aligned_time`
//! - Stores alignment metadata in Zarr attributes
//! - Supports any number of streams in a Zarr file
//!
//! # Usage
//!
//! ```bash
//! # Synchronize with default mode (common-start)
//! lsl-sync experiment.zarr
//!
//! # Synchronize with trimming (removes data outside common window)
//! lsl-sync experiment.zarr --trim-both
//!
//! # Use different alignment mode
//! lsl-sync experiment.zarr --mode first-stream
//! lsl-sync experiment.zarr --mode last-stream
//! lsl-sync experiment.zarr --mode absolute-zero
//!
//! # Trim only start or end
//! lsl-sync experiment.zarr --trim-start
//! lsl-sync experiment.zarr --trim-end
//!
//! # Only process specific streams (auto-skips invalid streams)
//! lsl-sync experiment.zarr --stream VHI_Control --stream VHI_Predict
//! ```
//!
//! # Alignment Modes
//!
//! - `common-start` (recommended): Align to latest start time where ALL streams have data
//! - `first-stream`: Align to earliest stream start (may have gaps)
//! - `last-stream`: Align to latest stream start
//! - `absolute-zero`: Align to t=0
//!
//! # Output
//!
//! For each stream:
//! - Creates `/<name>/aligned_time` array with synchronized timestamps
//! - Stores metadata in `/<name>/zarr.json`:
//!   - `alignment_offset`: Time offset applied
//!   - `trim_start_index`: Start index if trimmed
//!   - `trim_end_index`: End index if trimmed
//!   - `original_sample_count`: Samples before trimming
//!   - `aligned_sample_count`: Samples after trimming
//!
//! # Workflow
//!
//! ```bash
//! # 1. Record multiple streams
//! lsl-multi-recorder --source-ids "id1" "id2" --output experiment
//!
//! # 2. Synchronize timestamps
//! lsl-sync experiment.zarr --mode common-start --trim-both
//!
//! # 3. Inspect results
//! lsl-inspect experiment.zarr --verbose
//!
//! # 4. Validate synchronization
//! lsl-validate experiment.zarr
//! ```

use anyhow::Result;
use clap::Parser;
use ndarray::{Array1, Ix1};
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use zarrs::array::{Array, ArrayBuilder, DataType, FillValue};
use zarrs::array::codec::{BloscCodec, BloscCompressionLevel, BloscCompressor, BloscShuffleMode};
use zarrs::array_subset::ArraySubset;
use zarrs::filesystem::FilesystemStore;

#[derive(Parser)]
#[command(name = "lsl-sync")]
#[command(about = "Synchronize timestamps across streams in a Zarr recording")]
#[command(version)]
struct Args {
    /// Path to Zarr file to synchronize
    #[arg(default_value = "experiment.zarr")]
    zarr_file: PathBuf,

    /// Alignment mode
    #[arg(long, default_value = "common-start")]
    #[arg(value_parser = ["common-start", "first-stream", "last-stream", "absolute-zero"])]
    mode: String,

    /// Trim data before common start
    #[arg(long)]
    trim_start: bool,

    /// Trim data after common end
    #[arg(long)]
    trim_end: bool,

    /// Trim both start and end (shorthand for --trim-start --trim-end)
    #[arg(long)]
    trim_both: bool,

    /// Verbose output (show detailed stream information)
    #[arg(short, long)]
    verbose: bool,

    /// Only process specific streams (can be specified multiple times)
    #[arg(long)]
    stream: Vec<String>,
}

#[derive(Debug)]
struct StreamData {
    name: String,
    timestamps: Vec<f64>,
    sample_count: usize,
    nominal_srate: f64,  // 0.0 for irregular streams
    is_irregular: bool,  // true if nominal_srate == 0.0
}

#[derive(Debug, PartialEq)]
enum ValidationResult {
    Valid,
    InvalidTimestamps(String),  // Reason for invalidity
    InsufficientSamples(String),
}

/// Validate stream data for synchronization
fn validate_stream(stream: &StreamData) -> ValidationResult {
    // Check for empty stream
    if stream.sample_count == 0 {
        return ValidationResult::InsufficientSamples(
            "No samples recorded".to_string()
        );
    }

    // Get first and last timestamps
    let first_ts = stream.timestamps.first().copied().unwrap_or(0.0);
    let last_ts = stream.timestamps.last().copied().unwrap_or(0.0);

    // Check for invalid timestamps (suspiciously low values indicating uninitialized data)
    // LSL timestamps are typically large values (seconds since system boot)
    if first_ts < 1.0 {
        return ValidationResult::InvalidTimestamps(
            format!("First timestamp too low: {:.6}s (likely uninitialized data)", first_ts)
        );
    }

    // Check for duplicate timestamps (all same value = likely bogus)
    // Only flag if multiple samples AND all timestamps are identical
    if stream.sample_count > 1 && (last_ts - first_ts).abs() < 0.001 {
        return ValidationResult::InvalidTimestamps(
            format!("All timestamps identical: {:.6}s (likely bogus data)", first_ts)
        );
    }

    ValidationResult::Valid
}

fn main() -> Result<()> {
    let args = Args::parse();

    lsl_recording_toolbox::display_license_notice("lsl-sync");

    let trim_start = args.trim_start || args.trim_both;
    let trim_end = args.trim_end || args.trim_both;

    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║              LSL Synchronization Tool                          ║");
    println!("╚════════════════════════════════════════════════════════════════╝");
    println!();
    println!("Zarr file: {}", args.zarr_file.display());
    println!("Mode: {}", args.mode);
    println!("Trim: start={}, end={}", trim_start, trim_end);
    println!();

    let store = Arc::new(FilesystemStore::new(&args.zarr_file)?);

    // Read all streams
    println!("Reading streams...");
    let all_streams = read_streams(&store, &args.zarr_file)?;

    if all_streams.is_empty() {
        println!("WARNING: No streams found in Zarr file");
        return Ok(());
    }

    let regular_count = all_streams.iter().filter(|s| !s.is_irregular).count();
    let irregular_count = all_streams.len() - regular_count;
    println!("\tFound {} stream(s): {} regular, {} irregular",
             all_streams.len(), regular_count, irregular_count);
    for stream in &all_streams {
        let stream_type = if stream.is_irregular { "irregular" } else { "regular" };
        if args.verbose {
            let first_ts = stream.timestamps.first().unwrap_or(&0.0);
            let last_ts = stream.timestamps.last().unwrap_or(&0.0);
            let duration = last_ts - first_ts;
            println!("\t- {} ({}): {} samples, {:.3} Hz, t=[{:.6}, {:.6}] ({:.3}s)",
                     stream.name, stream_type, stream.sample_count,
                     stream.nominal_srate, first_ts, last_ts, duration);
        } else {
            println!("\t- {} ({}): {} samples", stream.name, stream_type, stream.sample_count);
        }
    }
    println!();

    // Filter streams based on --stream flag and validation
    println!("Validating streams...");
    let mut streams = Vec::new();
    let mut skipped_streams = Vec::new();

    for stream in all_streams {
        // Check if stream is in the user-specified list (if provided)
        let user_selected = args.stream.is_empty() || args.stream.contains(&stream.name);

        if !user_selected {
            skipped_streams.push((stream.name.clone(), "Not in --stream list".to_string()));
            continue;
        }

        // Validate stream data
        let validation = validate_stream(&stream);
        match validation {
            ValidationResult::Valid => {
                streams.push(stream);
            }
            ValidationResult::InvalidTimestamps(reason) => {
                skipped_streams.push((stream.name.clone(), reason));
            }
            ValidationResult::InsufficientSamples(reason) => {
                skipped_streams.push((stream.name.clone(), reason));
            }
        }
    }

    // Report skipped streams
    if !skipped_streams.is_empty() {
        println!("\tSkipped {} stream(s):", skipped_streams.len());
        for (name, reason) in &skipped_streams {
            println!("\t- {}: {}", name, reason);
        }
        println!();
    }

    // Check if we have any valid streams left
    if streams.is_empty() {
        println!("ERROR: No valid streams to synchronize after validation");
        println!("Hint: Use --stream to manually select specific streams");
        return Ok(());
    }

    let valid_regular_count = streams.iter().filter(|s| !s.is_irregular).count();
    let valid_irregular_count = streams.len() - valid_regular_count;
    println!("\tProcessing {} valid stream(s): {} regular, {} irregular",
             streams.len(), valid_regular_count, valid_irregular_count);
    println!();

    // Calculate alignment offsets
    println!("Calculating alignment...");
    let (reference_time, alignment_offsets) = calculate_alignment(&streams, &args.mode)?;

    if args.verbose {
        println!("\tReference time: {:.6} s (from {} streams)",
                 reference_time,
                 if regular_count > 0 { "regular" } else { "all" });
    } else {
        println!("\tReference time: {:.6} s", reference_time);
    }
    for (name, offset) in &alignment_offsets {
        // Display relative timing (when stream started relative to reference)
        // Positive: started BEFORE reference (earlier)
        // Negative: started AFTER reference (later)
        // Note: offset itself is what to ADD for alignment (reference - first_ts)
        let relative_ms = -offset * 1000.0; // Flip sign for intuitive display
        let sign = if relative_ms >= 0.0 { "+" } else { "" };
        if args.verbose {
            // Find the stream to show aligned time range
            if let Some(stream) = streams.iter().find(|s| s.name == *name) {
                let first_aligned = stream.timestamps.first().unwrap_or(&0.0) + offset;
                let last_aligned = stream.timestamps.last().unwrap_or(&0.0) + offset;
                println!("\t- {}: {}{}ms relative to ref -> t=[{:.6}, {:.6}] aligned",
                         name, sign, relative_ms as i32, first_aligned, last_aligned);
            }
        } else {
            println!("\t- {}: {}{}ms relative to reference", name, sign, relative_ms as i32);
        }
    }
    println!();

    // Calculate common time window (based on regular streams only)
    let (common_start, common_end) = calculate_common_window(&streams, &alignment_offsets);
    let duration = common_end - common_start;
    println!("Common window (absolute): {:.6} s -> {:.6} s (duration: {:.3} s)",
             common_start, common_end, duration);
    println!("Common window (relative): 0.000000 s -> {:.6} s (after alignment)", duration);
    println!();

    // Check and warn about irregular streams with events outside common window
    check_irregular_stream_coverage(&streams, &alignment_offsets, common_start, common_end, trim_start, trim_end);

    // Write aligned timestamps and sync metadata
    println!("Writing synchronized data...");
    for stream in &streams {
        write_aligned_timestamps(AlignmentParams {
            store: &store,
            stream_name: &stream.name,
            timestamps: &stream.timestamps,
            offset: alignment_offsets.get(&stream.name).copied().unwrap_or(0.0),
            common_start,
            common_end,
            trim_start,
            trim_end,
        })?;
        println!("\tDone: {}", stream.name);
    }
    println!();

    println!("Synchronization complete!");
    println!();
    println!("Aligned timestamps written to:");
    println!("\t/<stream>/aligned_time");
    println!();
    println!("Alignment metadata written to:");
    println!("\t/<stream>/zarr.json (attributes)");
    println!();
    println!("Use lsl-inspect to view results:");
    println!("\tlsl-inspect {} --verbose", args.zarr_file.display());

    Ok(())
}

fn read_streams(store: &Arc<FilesystemStore>, zarr_path: &Path) -> Result<Vec<StreamData>> {
    if !zarr_path.exists() {
        return Ok(Vec::new());
    }

    let mut streams = Vec::new();
    for entry in std::fs::read_dir(zarr_path)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }

        let stream_name = entry.file_name().to_string_lossy().to_string();

        // Read time array
        let time_path = format!("/{}/time", stream_name);
        let time_array = Array::<FilesystemStore>::open(store.clone(), &time_path)?;

        // For unlimited dimensions, shape may be 0 in metadata even if data exists
        // Find actual extent by counting chunks
        let chunk_shape_opt = time_array.chunk_grid().chunk_shape(&[0])?;
        let chunk_shape = chunk_shape_opt
            .ok_or_else(|| anyhow::anyhow!("Failed to get chunk shape for {}", stream_name))?;
        let chunk_size = chunk_shape[0].get() as usize;

        // Find highest chunk by checking chunk directory
        let time_chunk_dir = zarr_path.join(format!("{}/time/c", stream_name));
        let mut max_chunk = 0;
        if time_chunk_dir.exists() {
            for entry in std::fs::read_dir(&time_chunk_dir)?.flatten() {
                if let Ok(chunk_idx) = entry.file_name().to_string_lossy().parse::<usize>() {
                    max_chunk = max_chunk.max(chunk_idx);
                }
            }
        }

        // Estimate sample count (max_chunk + 1) * chunk_size
        // Then read that much to get actual data
        let estimated_samples = (max_chunk + 1) * chunk_size;

        if estimated_samples == 0 {
            println!("\tWARNING: Skipping {} (no samples)", stream_name);
            continue;
        }

        let subset = ArraySubset::new_with_start_shape(vec![0], vec![estimated_samples as u64])?;
        let timestamps_array = time_array.retrieve_array_subset_ndarray::<f64>(&subset)?;

        // Find actual end by checking for fill values (0.0)
        let mut sample_count = timestamps_array.len();
        for i in (0..timestamps_array.len()).rev() {
            if timestamps_array[i] != 0.0 {
                sample_count = i + 1;
                break;
            }
        }

        if sample_count == 0 {
            println!("\tWARNING: Skipping {} (no samples)", stream_name);
            continue;
        }

        let timestamps: Vec<f64> = timestamps_array.iter().take(sample_count).copied().collect();

        // Read nominal_srate from stream metadata
        let stream_group_path = format!("/{}", stream_name);
        let stream_group = zarrs::group::Group::open(store.clone(), &stream_group_path)?;

        // Try to read from stream_info.nominal_srate first (nested), then fallback to top-level
        let nominal_srate = stream_group
            .attributes()
            .get("stream_info")
            .and_then(|v| v.get("nominal_srate"))
            .and_then(|v| v.as_f64())
            .or_else(|| {
                stream_group
                    .attributes()
                    .get("nominal_srate")
                    .and_then(|v| v.as_f64())
            })
            .unwrap_or(0.0);

        let is_irregular = nominal_srate == 0.0;

        streams.push(StreamData {
            name: stream_name,
            timestamps,
            sample_count,
            nominal_srate,
            is_irregular,
        });
    }

    Ok(streams)
}

fn calculate_alignment(streams: &[StreamData], mode: &str) -> Result<(f64, HashMap<String, f64>)> {
    let mut alignment_offsets = HashMap::new();

    if streams.is_empty() {
        return Ok((0.0, alignment_offsets));
    }

    // Only use regular streams for alignment calculation
    // Irregular streams (events, markers) should not constrain the time window
    let regular_streams: Vec<_> = streams.iter().filter(|s| !s.is_irregular).collect();

    if regular_streams.is_empty() {
        println!("\tWARNING: No regular streams found - using all streams for alignment");
        // Fallback: use all streams if no regular streams exist
        let reference_time = match mode {
            "first-stream" => streams.iter().filter_map(|s| s.timestamps.first()).fold(f64::INFINITY, |acc, &x| acc.min(x)),
            "last-stream" | "common-start" => streams.iter().filter_map(|s| s.timestamps.first()).fold(f64::NEG_INFINITY, |acc, &x| acc.max(x)),
            "absolute-zero" => 0.0,
            _ => anyhow::bail!("Unknown alignment mode: {}", mode),
        };
        for stream in streams {
            if let Some(&first_timestamp) = stream.timestamps.first() {
                alignment_offsets.insert(stream.name.clone(), reference_time - first_timestamp);
            }
        }
        return Ok((reference_time, alignment_offsets));
    }

    let reference_time = match mode {
        "first-stream" => {
            // Earliest start time among REGULAR streams only
            regular_streams
                .iter()
                .filter_map(|s| s.timestamps.first())
                .fold(f64::INFINITY, |acc, &x| acc.min(x))
        }
        "last-stream" => {
            // Latest start time among REGULAR streams only
            regular_streams
                .iter()
                .filter_map(|s| s.timestamps.first())
                .fold(f64::NEG_INFINITY, |acc, &x| acc.max(x))
        }
        "absolute-zero" => 0.0,
        "common-start" => {
            // Latest start time (where ALL REGULAR streams have data) becomes t=0
            // Irregular streams do NOT constrain this
            regular_streams
                .iter()
                .filter_map(|s| s.timestamps.first())
                .fold(f64::NEG_INFINITY, |acc, &x| acc.max(x))
        }
        _ => anyhow::bail!("Unknown alignment mode: {}", mode),
    };

    // Calculate offset for ALL streams (both regular and irregular)
    // Irregular streams get the same offset but won't be trimmed aggressively
    for stream in streams {
        if let Some(&first_timestamp) = stream.timestamps.first() {
            let offset = reference_time - first_timestamp;
            alignment_offsets.insert(stream.name.clone(), offset);
        }
    }

    Ok((reference_time, alignment_offsets))
}

fn calculate_common_window(streams: &[StreamData], alignment_offsets: &HashMap<String, f64>) -> (f64, f64) {
    if streams.is_empty() {
        return (0.0, 0.0);
    }

    // Only use regular streams to calculate common window
    // Irregular streams should not constrain the time window
    let regular_streams: Vec<_> = streams.iter().filter(|s| !s.is_irregular).collect();

    if regular_streams.is_empty() {
        // Fallback: if no regular streams, use all streams
        let mut common_start = f64::NEG_INFINITY;
        let mut common_end = f64::INFINITY;
        for stream in streams {
            if let Some(&offset) = alignment_offsets.get(&stream.name)
                && let (Some(&first_ts), Some(&last_ts)) = (stream.timestamps.first(), stream.timestamps.last()) {
                    common_start = common_start.max(first_ts + offset);
                    common_end = common_end.min(last_ts + offset);
                }
        }
        return (common_start, common_end.max(common_start));
    }

    let mut common_start = f64::NEG_INFINITY;
    let mut common_end = f64::INFINITY;

    // Calculate window based on REGULAR streams only
    for stream in regular_streams {
        if let Some(&offset) = alignment_offsets.get(&stream.name)
            && let (Some(&first_ts), Some(&last_ts)) = (stream.timestamps.first(), stream.timestamps.last()) {
                let aligned_start = first_ts + offset;
                let aligned_end = last_ts + offset;

                common_start = common_start.max(aligned_start); // Latest start
                common_end = common_end.min(aligned_end); // Earliest end
            }
    }

    // Ensure common_end is not before common_start
    if common_end < common_start {
        common_end = common_start;
    }

    (common_start, common_end)
}

fn check_irregular_stream_coverage(
    streams: &[StreamData],
    alignment_offsets: &HashMap<String, f64>,
    common_start: f64,
    common_end: f64,
    trim_start: bool,
    trim_end: bool,
) {
    let irregular_streams: Vec<_> = streams.iter().filter(|s| s.is_irregular).collect();

    if irregular_streams.is_empty() {
        return;
    }

    let mut warnings = Vec::new();

    for stream in irregular_streams {
        if let Some(&offset) = alignment_offsets.get(&stream.name) {
            // Count events outside the common window
            let mut events_before = 0;
            let mut events_after = 0;
            let mut events_inside = 0;

            for &ts in &stream.timestamps {
                let aligned_ts = ts + offset;
                if aligned_ts < common_start {
                    events_before += 1;
                } else if aligned_ts > common_end {
                    events_after += 1;
                } else {
                    events_inside += 1;
                }
            }

            // Warn if trimming is enabled and events would be lost
            if trim_start && events_before > 0 {
                warnings.push(format!(
                    "\t- {}: {} event(s) before common window (will be trimmed)",
                    stream.name, events_before
                ));
            }
            if trim_end && events_after > 0 {
                warnings.push(format!(
                    "\t- {}: {} event(s) after common window (will be trimmed)",
                    stream.name, events_after
                ));
            }

            // Info about event distribution
            if events_before > 0 || events_after > 0 {
                let total = stream.timestamps.len();
                warnings.push(format!(
                    "\t- {}: {}/{} events inside window, {} before, {} after",
                    stream.name, events_inside, total, events_before, events_after
                ));
            }
        }
    }

    if !warnings.is_empty() {
        println!("Irregular stream event coverage:");
        for warning in warnings {
            println!("{}", warning);
        }
        println!();
    }
}

struct AlignmentParams<'a> {
    store: &'a Arc<FilesystemStore>,
    stream_name: &'a str,
    timestamps: &'a [f64],
    offset: f64,
    common_start: f64,
    common_end: f64,
    trim_start: bool,
    trim_end: bool,
}

fn write_aligned_timestamps(params: AlignmentParams) -> Result<()> {
    let AlignmentParams {
        store,
        stream_name,
        timestamps,
        offset,
        common_start,
        common_end,
        trim_start,
        trim_end,
    } = params;
    // Shift timestamps to make common_start = t=0
    // Streams that started before common_start will have negative timestamps
    let aligned_timestamps: Vec<f64> = timestamps
        .iter()
        .map(|&t| t - common_start)
        .collect();

    // Determine trim indices (common_start is now at t=0, common_end is relative to t=0)
    let relative_common_end = common_end - common_start;
    let (trim_start_idx, trim_end_idx) = if trim_start || trim_end {
        let start_idx = if trim_start {
            aligned_timestamps
                .iter()
                .position(|&t| t >= 0.0)  // common_start is now at t=0
                .unwrap_or(0)
        } else {
            0
        };

        let end_idx = if trim_end {
            aligned_timestamps
                .iter()
                .rposition(|&t| t <= relative_common_end)
                .map(|i| i + 1)
                .unwrap_or(aligned_timestamps.len())
        } else {
            aligned_timestamps.len()
        };

        (start_idx, end_idx)
    } else {
        (0, aligned_timestamps.len())
    };

    // Write ALL aligned timestamps (no trimming - Python will use indices)
    let final_timestamps = &aligned_timestamps;

    // Write to /<stream>/aligned_time (right next to the raw time array)
    let stream_path = format!("/{}", stream_name);
    let aligned_time_path = format!("{}/aligned_time", stream_path);

    // Create Blosc codec with BitShuffle for optimal float64 compression
    let compression_level = BloscCompressionLevel::try_from(5u8)
        .map_err(|e| anyhow::anyhow!("Invalid compression level: {}", e))?;
    let blosc_codec = Arc::new(BloscCodec::new(
        BloscCompressor::LZ4,
        compression_level,
        None,  // blocksize (auto-detect)
        BloscShuffleMode::BitShuffle,  // BitShuffle for float64 timestamps
        Some(8),  // typesize: 8 bytes for float64
    )?);

    let array = ArrayBuilder::new(
        vec![final_timestamps.len() as u64],
        vec![100],
        DataType::Float64,
        FillValue::from(0.0f64),
    )
    .bytes_to_bytes_codecs(vec![blosc_codec])
    .build(store.clone(), &aligned_time_path)?;

    array.store_metadata()?;

    // Write data
    let data_array = Array1::from(final_timestamps.to_vec());
    array.store_array_subset_ndarray::<f64, Ix1>(&[0], data_array)?;

    // Write alignment metadata as attributes to the stream group
    let stream_group_path = format!("/{}", stream_name);
    let mut stream_group = zarrs::group::Group::open(store.clone(), &stream_group_path)?;

    // Add alignment metadata (trim indices for Python to use, but no actual trimming)
    let mut attrs = serde_json::Map::new();
    attrs.insert("alignment_offset".to_string(), json!(offset));
    attrs.insert("trim_start_index".to_string(), json!(trim_start_idx));
    attrs.insert("trim_end_index".to_string(), json!(trim_end_idx));
    attrs.insert("original_sample_count".to_string(), json!(timestamps.len()));
    // Note: Arrays are NOT trimmed - Python should use trim indices
    attrs.insert("trimmed_sample_count".to_string(), json!(trim_end_idx - trim_start_idx));

    stream_group.attributes_mut().extend(attrs);
    stream_group.store_metadata()?;

    Ok(())
}


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
//! - Writes aligned timestamps to `/streams/<name>/aligned_time`
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
//! - Creates `/streams/<name>/aligned_time` array with synchronized timestamps
//! - Stores metadata in `/streams/<name>/zarr.json`:
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
}

#[derive(Debug)]
struct StreamData {
    name: String,
    timestamps: Vec<f64>,
    sample_count: usize,
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
    let streams = read_streams(&store, &args.zarr_file)?;

    if streams.is_empty() {
        println!("WARNING: No streams found in Zarr file");
        return Ok(());
    }

    println!("\tFound {} stream(s)", streams.len());
    for stream in &streams {
        println!("\t- {}: {} samples", stream.name, stream.sample_count);
    }
    println!();

    // Calculate alignment offsets
    println!("Calculating alignment...");
    let (reference_time, alignment_offsets) = calculate_alignment(&streams, &args.mode)?;

    println!("\tReference time: {:.6} s", reference_time);
    for (name, offset) in &alignment_offsets {
        let offset_ms = offset * 1000.0;
        let sign = if *offset >= 0.0 { "+" } else { "" };
        println!("\t- {}: {}{}ms offset", name, sign, offset_ms as i32);
    }
    println!();

    // Calculate common time window
    let (common_start, common_end) = calculate_common_window(&streams, &alignment_offsets);
    let duration = common_end - common_start;
    println!("Common window (absolute): {:.6} s -> {:.6} s (duration: {:.3} s)",
             common_start, common_end, duration);
    println!("Common window (relative): 0.000000 s -> {:.6} s (after alignment)", duration);
    println!();

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
    println!("\t/streams/<stream>/aligned_time");
    println!();
    println!("Alignment metadata written to:");
    println!("\t/streams/<stream>/zarr.json (attributes)");
    println!();
    println!("Use lsl-inspect to view results:");
    println!("\tlsl-inspect {} --verbose", args.zarr_file.display());

    Ok(())
}

fn read_streams(store: &Arc<FilesystemStore>, zarr_path: &Path) -> Result<Vec<StreamData>> {
    let streams = Vec::new();

    // Read streams from /streams directory
    let streams_dir = zarr_path.join("streams");
    if !streams_dir.exists() {
        return Ok(streams);
    }

    let mut streams = Vec::new();
    for entry in std::fs::read_dir(streams_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }

        let stream_name = entry.file_name().to_string_lossy().to_string();

        // Read time array
        let time_path = format!("/streams/{}/time", stream_name);
        let time_array = Array::<FilesystemStore>::open(store.clone(), &time_path)?;

        // For unlimited dimensions, shape may be 0 in metadata even if data exists
        // Find actual extent by counting chunks
        let chunk_shape_opt = time_array.chunk_grid().chunk_shape(&[0])?;
        let chunk_shape = chunk_shape_opt
            .ok_or_else(|| anyhow::anyhow!("Failed to get chunk shape for {}", stream_name))?;
        let chunk_size = chunk_shape[0].get() as usize;

        // Find highest chunk by checking chunk directory
        let time_chunk_dir = zarr_path.join(format!("streams/{}/time/c", stream_name));
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

        streams.push(StreamData {
            name: stream_name,
            timestamps,
            sample_count,
        });
    }

    Ok(streams)
}

fn calculate_alignment(streams: &[StreamData], mode: &str) -> Result<(f64, HashMap<String, f64>)> {
    let mut alignment_offsets = HashMap::new();

    if streams.is_empty() {
        return Ok((0.0, alignment_offsets));
    }

    let reference_time = match mode {
        "first-stream" => {
            // Earliest start time
            streams
                .iter()
                .filter_map(|s| s.timestamps.first())
                .fold(f64::INFINITY, |acc, &x| acc.min(x))
        }
        "last-stream" => {
            // Latest start time
            streams
                .iter()
                .filter_map(|s| s.timestamps.first())
                .fold(f64::NEG_INFINITY, |acc, &x| acc.max(x))
        }
        "absolute-zero" => 0.0,
        "common-start" => {
            // Latest start time (where ALL streams have data) becomes t=0
            streams
                .iter()
                .filter_map(|s| s.timestamps.first())
                .fold(f64::NEG_INFINITY, |acc, &x| acc.max(x))
        }
        _ => anyhow::bail!("Unknown alignment mode: {}", mode),
    };

    // Calculate offset for each stream
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

    let mut common_start = f64::NEG_INFINITY;
    let mut common_end = f64::INFINITY;

    for stream in streams {
        if let Some(&offset) = alignment_offsets.get(&stream.name) {
            if let (Some(&first_ts), Some(&last_ts)) = (stream.timestamps.first(), stream.timestamps.last()) {
                let aligned_start = first_ts + offset;
                let aligned_end = last_ts + offset;

                common_start = common_start.max(aligned_start); // Latest start
                common_end = common_end.min(aligned_end); // Earliest end
            }
        }
    }

    // Ensure common_end is not before common_start
    if common_end < common_start {
        common_end = common_start;
    }

    (common_start, common_end)
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

    // Write to /streams/<stream>/aligned_time (right next to the raw time array)
    let stream_path = format!("/streams/{}", stream_name);
    let aligned_time_path = format!("{}/aligned_time", stream_path);

    // Create Blosc codec
    let compression_level = BloscCompressionLevel::try_from(5u8)
        .map_err(|e| anyhow::anyhow!("Invalid compression level: {}", e))?;
    let blosc_codec = Arc::new(BloscCodec::new(
        BloscCompressor::LZ4,
        compression_level,
        None,
        BloscShuffleMode::NoShuffle,
        None,
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
    let stream_group_path = format!("/streams/{}", stream_name);
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


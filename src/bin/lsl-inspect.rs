//! LSL Inspect - Zarr file inspection and metadata viewer
//!
//! This tool inspects Zarr files created by lsl-recorder and displays file structure,
//! metadata, stream information, and recording duration.
//!
//! # Features
//!
//! - Display Zarr file structure and hierarchy
//! - Show global metadata (subject, session, notes)
//! - List all streams within a Zarr file
//! - Display stream information (channels, sample rate, format, duration)
//! - Filter by specific stream name(s)
//! - Verbose mode for additional details
//! - Clean hierarchical output with Unicode box drawing
//!
//! # Usage
//!
//! ```bash
//! # Inspect default file (experiment.zarr)
//! lsl-inspect
//!
//! # Inspect specific file
//! lsl-inspect recording.zarr
//!
//! # Verbose mode with additional metadata
//! lsl-inspect experiment.zarr --verbose
//!
//! # Filter to specific stream(s)
//! lsl-inspect experiment.zarr --stream EMG
//! lsl-inspect experiment.zarr --stream EMG --stream EEG
//! ```
//!
//! # Output Format
//!
//! Displays:
//! - Global metadata (subject, session ID, start time, notes)
//! - Stream list with names and key information
//! - For each stream:
//!   - Channel count and format
//!   - Sample rate (nominal and actual)
//!   - Recording duration
//!   - Sample count
//!   - Timestamp range
//!   - (Verbose) Full stream info and recorder config

use anyhow::Result;
use clap::Parser;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use zarrs::array::Array;
use zarrs::array_subset::ArraySubset;
use zarrs::filesystem::FilesystemStore;
use zarrs::storage::{ReadableStorageTraits, StoreKey};

#[derive(Parser)]
#[command(name = "lsl-inspect")]
#[command(about = "Inspect Zarr recordings created by lsl-recorder")]
#[command(version)]
struct Args {
    /// Path to Zarr file to inspect
    #[arg(default_value = "experiment.zarr")]
    file_path: String,

    /// Show detailed stream information
    #[arg(short, long)]
    verbose: bool,

    /// Filter to specific stream name(s)
    #[arg(short, long)]
    stream: Option<Vec<String>>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    lsl_recording_toolbox::display_license_notice("lsl-inspect");

    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║              LSL Zarr File Inspector                           ║");
    println!("╚════════════════════════════════════════════════════════════════╝");
    println!();
    println!("📁 Store: {}", args.file_path);
    println!();

    let store = Arc::new(FilesystemStore::new(&args.file_path)?);

    // Inspect global metadata from /meta/.zattrs
    match read_group_attributes(&store, "/meta") {
        Ok(meta_attrs) => {
            println!("🌐 GLOBAL METADATA");
            for (key, value) in meta_attrs.as_object().unwrap_or(&serde_json::Map::new()) {
                let value_str = if value.is_string() {
                    value.as_str().unwrap_or("").to_string()
                } else {
                    value.to_string()
                };

                println!(
                    "  ├─ {}: {}",
                    key,
                    if value_str.len() > 100 {
                        format!("{}...", &value_str[..100])
                    } else {
                        value_str
                    }
                );
            }
            println!();
        }
        Err(e) => {
            if args.verbose {
                println!("⚠️  No global metadata found: {}", e);
                println!();
            }
        }
    }

    // Inspect streams
    let streams_path = PathBuf::from(&args.file_path).join("streams");
    let mut stream_count = 0;
    let mut total_samples = 0;

    if streams_path.exists() && streams_path.is_dir() {
        // Count streams first
        for entry in std::fs::read_dir(&streams_path)? {
            if entry?.file_type()?.is_dir() {
                stream_count += 1;
            }
        }

        println!("📊 STREAMS ({} found)", stream_count);
        println!();

        let mut stream_idx = 0;
        let streams_dir = std::fs::read_dir(&streams_path)?;
        let mut entries: Vec<_> = streams_dir.collect();
        entries.sort_by_key(|e| e.as_ref().map(|e| e.file_name()).unwrap_or_default());

        for entry in entries {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let stream_name = entry.file_name().to_string_lossy().to_string();

                // Filter by stream name if specified
                if let Some(ref filter_streams) = args.stream {
                    if !filter_streams.contains(&stream_name) {
                        continue;
                    }
                }

                stream_idx += 1;
                let is_last = stream_idx == stream_count;
                let prefix = if is_last { "  └─" } else { "  ├─" };
                let indent = if is_last { "     " } else { "  │  " };

                println!("{} {}", prefix, stream_name);

                let stream_path = format!("/streams/{}", stream_name);

                // Show data array info
                let data_array_path = format!("{}/data", stream_path);
                match Array::<FilesystemStore>::open(store.clone(), &data_array_path) {
                    Ok(data_array) => {
                        let shape = data_array.shape();
                        if shape.len() >= 2 {
                            let num_channels = shape[0] as usize;
                            println!("{}├─ Channels: {}", indent, num_channels);
                        }
                    }
                    Err(e) if args.verbose => {
                        println!("{}├─ ⚠️  Could not open data array at '{}': {}", indent, data_array_path, e);
                    }
                    _ => {}
                }

                // Show time array info and calculate duration
                let time_array_path = format!("{}/time", stream_path);
                match Array::<FilesystemStore>::open(store.clone(), &time_array_path) {
                    Ok(time_array) => {
                    let shape = time_array.shape();

                    // Read time data to calculate duration
                    if shape[0] > 0 {
                        let num_samples = shape[0] as usize;
                        total_samples += num_samples;
                        println!("{}├─ Samples: {}", indent, num_samples);

                        if num_samples >= 2 {
                            // Read first timestamp
                            let first_subset = ArraySubset::new_with_start_shape(vec![0], vec![1])?;
                            let first_arr = time_array.retrieve_array_subset_ndarray::<f64>(&first_subset)?;
                            let first_time = first_arr[[0]];

                            // Read last timestamp
                            let last_subset = ArraySubset::new_with_start_shape(
                                vec![num_samples as u64 - 1],
                                vec![1],
                            )?;
                            let last_arr = time_array.retrieve_array_subset_ndarray::<f64>(&last_subset)?;
                            let last_time = last_arr[[0]];

                            let duration = last_time - first_time;
                            println!("{}├─ Duration: {:.3} s", indent, duration);
                            println!("{}├─ Time Range: {:.6} → {:.6}", indent, first_time, last_time);
                        } else if num_samples == 1 {
                            println!("{}├─ Duration: single sample", indent);
                        } else {
                            println!("{}├─ Duration: no samples", indent);
                        }
                    }
                    }
                    Err(e) if args.verbose => {
                        println!("{}├─ ⚠️  Could not open time array at '{}': {}", indent, time_array_path, e);
                    }
                    _ => {}
                }

                // Show attributes from /streams/<stream_name>/zarr.json (stream group attributes)
                if let Ok(attrs) = read_group_attributes(&store, &stream_path) {
                    for (attr_name, parsed) in attrs.as_object().unwrap_or(&serde_json::Map::new()) {
                        if parsed.is_object() {
                            if attr_name == "stream_info" {
                                // Show key stream info fields
                                if let Some(source_id) = parsed.get("source_id") {
                                    println!("{}├─ Source ID: {}", indent, source_id.as_str().unwrap_or(""));
                                }
                                if let Some(nominal_srate) = parsed.get("nominal_srate") {
                                    println!("{}├─ Nominal rate: {} Hz", indent, nominal_srate);
                                }
                                if let Some(channel_format) = parsed.get("channel_format") {
                                    println!("{}├─ Format: {}", indent, channel_format.as_str().unwrap_or(""));
                                }

                                // Show additional fields in verbose mode
                                if args.verbose {
                                    if let Some(hostname) = parsed.get("hostname") {
                                        println!("{}├─ Hostname: {}", indent, hostname.as_str().unwrap_or(""));
                                    }
                                    if let Some(stream_type) = parsed.get("type") {
                                        println!("{}├─ Type: {}", indent, stream_type.as_str().unwrap_or(""));
                                    }
                                }
                            } else if attr_name == "recorder_config" {
                                // Show recorder version
                                if let Some(recorder_version) = parsed.get("recorder_version") {
                                    println!("{}└─ Recorder: v{}", indent, recorder_version.as_str().unwrap_or("unknown"));
                                }

                                // Show additional fields in verbose mode
                                if args.verbose {
                                    if let Some(recorded_at) = parsed.get("recorded_at") {
                                        println!("{}   Recorded at: {}", indent, recorded_at.as_str().unwrap_or(""));
                                    }
                                }
                            }
                        }
                    }
                }
                println!();
            }
        }

        // Show summary
        println!("✅ Summary: {} stream{}, {} total samples",
                 stream_count,
                 if stream_count == 1 { "" } else { "s" },
                 total_samples);
        println!();
    }

    Ok(())
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
        Ok(json!({}))
    }
}

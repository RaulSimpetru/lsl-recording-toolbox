use anyhow::Result;
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use zarrs::array::Array;
use zarrs::array_subset::ArraySubset;
use zarrs::filesystem::FilesystemStore;
use zarrs::storage::{ReadableStorageTraits, StoreKey};

#[derive(Debug, Clone)]
struct StreamData {
    name: String,
    store_path: String,
    timestamps: Vec<f64>,
    data_shape: (usize, usize), // (channels, samples)
    stream_info: Value,
    recorder_config: Value,
    start_time: f64,
    end_time: f64,
    duration: f64,
    sample_count: usize,
    nominal_sample_rate: f64,
    actual_sample_rate: f64,
    channel_count: usize,
    channel_format: String,
}

impl StreamData {
    fn new(name: String, store_path: String) -> Self {
        Self {
            name,
            store_path,
            timestamps: Vec::new(),
            data_shape: (0, 0),
            stream_info: serde_json::json!({}),
            recorder_config: serde_json::json!({}),
            start_time: 0.0,
            end_time: 0.0,
            duration: 0.0,
            sample_count: 0,
            nominal_sample_rate: 0.0,
            actual_sample_rate: 0.0,
            channel_count: 0,
            channel_format: String::new(),
        }
    }
}

#[derive(Debug)]
struct SyncAnalysis {
    streams: Vec<StreamData>,
    start_time_diff: f64,     // Max - Min start times
    end_time_diff: f64,       // Max - Min end times
    duration_diff: f64,       // Max - Min durations
    max_timestamp_drift: f64, // Maximum drift between streams
    is_synchronized: bool,
    sync_threshold: f64, // Threshold for considering streams synchronized
}

fn load_zarr_stream_data(store_path: &str) -> Result<Vec<StreamData>> {
    let path = Path::new(store_path);
    if !path.exists() {
        return Err(anyhow::anyhow!("Store not found: {}", store_path));
    }

    let store = Arc::new(FilesystemStore::new(store_path)?);
    let mut streams = Vec::new();

    // Find all streams in /streams/
    let streams_dir = path.join("streams");
    if !streams_dir.exists() || !streams_dir.is_dir() {
        return Err(anyhow::anyhow!("No streams found in store"));
    }

    for entry in std::fs::read_dir(streams_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }

        let stream_name = entry.file_name().to_string_lossy().to_string();
        let mut stream_data = StreamData::new(stream_name.clone(), store_path.to_string());

        let stream_path = format!("/streams/{}", stream_name);

        // Load timestamps
        let time_array_path = format!("{}/time", stream_path);
        if let Ok(time_array) = Array::<FilesystemStore>::open(store.clone(), &time_array_path) {
            let shape = time_array.shape();
            stream_data.sample_count = shape[0] as usize;

            if stream_data.sample_count > 0 {
                // Read all timestamps
                let time_subset = ArraySubset::new_with_ranges(&[0..shape[0]]);
                let timestamps_ndarray = time_array.retrieve_array_subset_ndarray::<f64>(&time_subset)?;
                stream_data.timestamps = timestamps_ndarray.into_raw_vec_and_offset().0;

                stream_data.start_time = stream_data.timestamps[0];
                stream_data.end_time = stream_data.timestamps[stream_data.timestamps.len() - 1];
                stream_data.duration = stream_data.end_time - stream_data.start_time;

                // Calculate actual sample rate
                if stream_data.sample_count > 1 {
                    stream_data.actual_sample_rate =
                        (stream_data.sample_count - 1) as f64 / stream_data.duration;
                }
            }
        }

        // Load data shape
        let data_array_path = format!("{}/data", stream_path);
        if let Ok(data_array) = Array::<FilesystemStore>::open(store.clone(), &data_array_path) {
            let shape = data_array.shape();
            stream_data.data_shape = (shape[0] as usize, shape[1] as usize); // (channels, samples)
            stream_data.channel_count = shape[0] as usize;

            // Load attributes from data array
            if let Ok(attrs) = read_attributes(&store, &data_array_path) {
                if let Some(obj) = attrs.as_object() {
                    // Extract stream_info
                    if let Some(stream_info) = obj.get("stream_info") {
                        stream_data.stream_info = stream_info.clone();

                        // Extract key information
                        if let Some(nominal_srate) =
                            stream_info.get("nominal_srate").and_then(|v| v.as_f64())
                        {
                            stream_data.nominal_sample_rate = nominal_srate;
                        }
                        if let Some(channel_format) =
                            stream_info.get("channel_format").and_then(|v| v.as_str())
                        {
                            stream_data.channel_format = channel_format.to_string();
                        }
                    }

                    // Extract recorder_config
                    if let Some(recorder_config) = obj.get("recorder_config") {
                        stream_data.recorder_config = recorder_config.clone();
                    }
                }
            }
        }

        streams.push(stream_data);
    }

    Ok(streams)
}

fn analyze_synchronization(streams: &[StreamData]) -> SyncAnalysis {
    let sync_threshold = 0.200; // 200ms threshold for synchronization (accounts for system timing variations)

    if streams.is_empty() {
        return SyncAnalysis {
            streams: streams.to_vec(),
            start_time_diff: 0.0,
            end_time_diff: 0.0,
            duration_diff: 0.0,
            max_timestamp_drift: 0.0,
            is_synchronized: false,
            sync_threshold,
        };
    }

    // Calculate timing differences
    let start_times: Vec<f64> = streams.iter().map(|s| s.start_time).collect();
    let end_times: Vec<f64> = streams.iter().map(|s| s.end_time).collect();
    let durations: Vec<f64> = streams.iter().map(|s| s.duration).collect();

    let start_time_diff = start_times.iter().fold(f64::NAN, |a, &b| a.max(b))
        - start_times.iter().fold(f64::INFINITY, |a, &b| a.min(b));
    let end_time_diff = end_times.iter().fold(f64::NAN, |a, &b| a.max(b))
        - end_times.iter().fold(f64::INFINITY, |a, &b| a.min(b));
    let duration_diff = durations.iter().fold(f64::NAN, |a, &b| a.max(b))
        - durations.iter().fold(f64::INFINITY, |a, &b| a.min(b));

    // Calculate maximum timestamp drift between streams
    let mut max_drift: f64 = 0.0;
    if streams.len() > 1 {
        let min_length = streams
            .iter()
            .map(|s| s.timestamps.len())
            .min()
            .unwrap_or(0);
        for i in 0..min_length.min(100) {
            // Check first 100 samples for drift
            let mut sample_times = Vec::new();
            for stream in streams {
                if i < stream.timestamps.len() {
                    sample_times.push(stream.timestamps[i]);
                }
            }
            if sample_times.len() > 1 {
                let max_time = sample_times.iter().fold(f64::NAN, |a, &b| a.max(b));
                let min_time = sample_times.iter().fold(f64::INFINITY, |a, &b| a.min(b));
                max_drift = max_drift.max(max_time - min_time);
            }
        }
    }

    let is_synchronized = start_time_diff < sync_threshold
        && end_time_diff < sync_threshold
        && max_drift < sync_threshold;

    SyncAnalysis {
        streams: streams.to_vec(),
        start_time_diff,
        end_time_diff,
        duration_diff,
        max_timestamp_drift: max_drift,
        is_synchronized,
        sync_threshold,
    }
}

fn print_stream_info(stream: &StreamData) {
    println!("Stream: {}", stream.name);
    println!("\tStore:\t\t{}", stream.store_path);
    println!(
        "\tData shape:\t{:?} (channels × samples)",
        stream.data_shape
    );
    println!("\tChannels:\t{}", stream.channel_count);
    println!("\tSample count:\t{}", stream.sample_count);
    println!("\tDuration:\t{:.3} seconds", stream.duration);
    println!("\tNominal rate:\t{:.1} Hz", stream.nominal_sample_rate);
    println!("\tActual rate:\t{:.1} Hz", stream.actual_sample_rate);

    let rate_accuracy = if stream.nominal_sample_rate > 0.0 {
        (stream.actual_sample_rate / stream.nominal_sample_rate) * 100.0
    } else {
        0.0
    };
    println!("\tRate accuracy:\t{:.2}%", rate_accuracy);
    println!("\tChannel format:\t{}", stream.channel_format);

    // Timing information
    println!("\tStart time:\t{:.6}", stream.start_time);
    println!("\tEnd time:\t{:.6}", stream.end_time);

    // Extract some key metadata if available
    if let Some(source_id) = stream.stream_info.get("source_id").and_then(|v| v.as_str()) {
        println!("\tSource ID:\t{}", source_id);
    }
    if let Some(hostname) = stream.stream_info.get("hostname").and_then(|v| v.as_str()) {
        println!("\tHostname:\t{}", hostname);
    }

    println!();
}

fn print_sync_analysis(analysis: &SyncAnalysis) {
    println!("SYNCHRONIZATION ANALYSIS");
    println!("========================");

    if analysis.is_synchronized {
        println!("Status:\t\t\tSYNCHRONIZED");
    } else {
        println!("Status:\t\t\tNOT SYNCHRONIZED");
    }

    println!(
        "Sync threshold:\t\t{:.1} ms",
        analysis.sync_threshold * 1000.0
    );
    println!();

    println!("TIMING ANALYSIS:");
    println!(
        "\tStart time difference:\t{:.1} ms",
        analysis.start_time_diff * 1000.0
    );
    println!(
        "\tEnd time difference:\t{:.1} ms",
        analysis.end_time_diff * 1000.0
    );
    println!(
        "\tDuration difference:\t{:.1} ms",
        analysis.duration_diff * 1000.0
    );
    println!(
        "\tMax timestamp drift:\t{:.1} ms",
        analysis.max_timestamp_drift * 1000.0
    );

    // Show why synchronization failed/passed
    if analysis.is_synchronized {
        println!(
            "\tAll timing differences are within {:.0}ms threshold",
            analysis.sync_threshold * 1000.0
        );
    } else {
        println!("\tSynchronization failed because:");
        if analysis.start_time_diff >= analysis.sync_threshold {
            println!(
                "\t\t• Start time difference ({:.1}ms) exceeds {:.0}ms threshold",
                analysis.start_time_diff * 1000.0,
                analysis.sync_threshold * 1000.0
            );
        }
        if analysis.end_time_diff >= analysis.sync_threshold {
            println!(
                "\t\t• End time difference ({:.1}ms) exceeds {:.0}ms threshold",
                analysis.end_time_diff * 1000.0,
                analysis.sync_threshold * 1000.0
            );
        }
        if analysis.max_timestamp_drift >= analysis.sync_threshold {
            println!(
                "\t\t• Timestamp drift ({:.1}ms) exceeds {:.0}ms threshold",
                analysis.max_timestamp_drift * 1000.0,
                analysis.sync_threshold * 1000.0
            );
        }
    }
    println!();

    // Detailed timing information
    if !analysis.streams.is_empty() {
        println!("DETAILED TIMING:");
        let min_start = analysis
            .streams
            .iter()
            .map(|s| s.start_time)
            .fold(f64::INFINITY, |a, b| a.min(b));

        // Show precise start and end times for each stream
        for stream in &analysis.streams {
            let start_offset_ms = (stream.start_time - min_start) * 1000.0;
            println!("\t{}:", stream.name);
            println!("\t\tStart offset:\t+{:.1} ms", start_offset_ms);
            println!("\t\tDuration:\t{:.3} seconds", stream.duration);
            println!("\t\tEnd time:\t{:.6} s", stream.end_time);
        }
        println!();

        // Simple timeline (note: limited resolution)
        println!("RECORDING TIMELINE (second-level resolution):");
        let max_end = analysis
            .streams
            .iter()
            .map(|s| s.end_time)
            .fold(f64::NAN, |a, b| a.max(b));
        let total_duration = max_end - min_start;
        let max_name_len = analysis
            .streams
            .iter()
            .map(|s| s.name.len())
            .max()
            .unwrap_or(10);
        let name_width = max_name_len.max(10);

        for stream in &analysis.streams {
            let start_offset = ((stream.start_time - min_start) / total_duration * 50.0) as usize;
            let duration_bars = ((stream.duration / total_duration * 50.0) as usize).max(1);

            let mut timeline = vec![' '; 50];
            for i in start_offset..(start_offset + duration_bars).min(50) {
                timeline[i] = '█';
            }
            let timeline_str: String = timeline.iter().collect();
            println!(
                "\t{:width$}: |{}|",
                stream.name,
                timeline_str,
                width = name_width
            );
        }

        // Timeline scale
        let scale_padding = format!("\t{}", " ".repeat(name_width + 3));
        let end_time_str = format!("{:.1}s", total_duration);
        let start_str = "0.0";
        let total_scale_width = 50;
        let padding_needed = total_scale_width - start_str.len() - end_time_str.len();
        let middle_padding = " ".repeat(padding_needed);
        println!(
            "{}|{}{}{}|",
            scale_padding, start_str, middle_padding, end_time_str
        );

        // Resolution note
        let resolution_ms = total_duration * 1000.0 / 50.0;
        println!(
            "\tNote: Timeline resolution ~{:.0}ms per character",
            resolution_ms
        );
        println!("\t      Millisecond differences may not be visible above");
        println!();
    }
}

fn print_summary(analysis: &SyncAnalysis) {
    println!("SUMMARY");
    println!("=======");
    println!("Total streams analyzed:\t{}", analysis.streams.len());

    if !analysis.streams.is_empty() {
        let total_samples: usize = analysis.streams.iter().map(|s| s.sample_count).sum();
        let avg_duration = analysis.streams.iter().map(|s| s.duration).sum::<f64>()
            / analysis.streams.len() as f64;

        println!("Total samples:\t\t{}", total_samples);
        println!("Average duration:\t{:.3} seconds", avg_duration);

        if analysis.is_synchronized {
            println!("All streams appear to be properly synchronized");
            println!("The recordings should be suitable for multi-stream analysis");
        } else {
            println!("Synchronization issues detected!");
            println!("Small timing differences (<200ms) are common due to:");
            println!("\t• System thread scheduling variations");
            println!("\t• Network timing in LSL data transmission");
            println!("\t• Recording start/stop coordination delays");
            println!();
            println!("These differences may not affect most analyses, but consider:");
            println!("\t• For event-related analysis: Use LSL timestamps for alignment");
            println!("\t• For spectral analysis: Usually not problematic");
            println!("\t• For cross-correlation: May need timestamp-based alignment");
        }

        // Add context about sync thresholds
        println!();
        println!("SYNCHRONIZATION CONTEXT:");
        println!(
            "\tThreshold used: {:.0}ms",
            analysis.sync_threshold * 1000.0
        );
        println!("\tThis accounts for normal system timing variations");
        println!("\tStreams recorded simultaneously can still have small offsets")
    }

    println!();
    println!("Run 'cargo run --example multi_recorder' to generate test stores");
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    println!("LSL Multi-Stream Synchronization Validator");
    println!("==========================================");
    println!();

    let test_stores = if args.len() > 1 {
        // Use command line arguments as store paths
        args[1..].to_vec()
    } else {
        // Default to standard multi-recorder stores
        vec![
            "experiment_EMG.zarr".to_string(),
            "experiment_EEG.zarr".to_string(),
        ]
    };

    let mut all_streams = Vec::new();
    let mut _found_stores = 0;

    // Load data from all available stores
    for store_path in &test_stores {
        match load_zarr_stream_data(store_path) {
            Ok(mut streams) => {
                _found_stores += 1;
                println!("Loaded {} stream(s) from {}", streams.len(), store_path);
                all_streams.append(&mut streams);
            }
            Err(e) => {
                println!("Could not load {}: {}", store_path, e);
            }
        }
    }

    if all_streams.is_empty() {
        println!("No valid Zarr stores found!");
        println!("Make sure to run 'cargo run --example multi_recorder' first");
        return Ok(());
    }

    println!();

    // Display individual stream information
    println!("STREAM INFORMATION");
    println!("==================");
    for stream in &all_streams {
        print_stream_info(stream);
    }

    // Perform synchronization analysis
    let analysis = analyze_synchronization(&all_streams);
    print_sync_analysis(&analysis);

    // Print summary
    print_summary(&analysis);

    Ok(())
}

/// Read attributes from a path's .zattrs file
fn read_attributes(store: &Arc<FilesystemStore>, path: &str) -> Result<Value> {
    let attrs_path = format!("{}/.zattrs", path.trim_end_matches('/'));
    let attrs_key = StoreKey::new(&attrs_path)?;
    let attrs_bytes = store
        .get(&attrs_key)?
        .ok_or_else(|| anyhow::anyhow!("Attributes not found at {}", attrs_path))?;
    let attrs: Value = serde_json::from_slice(&attrs_bytes)?;
    Ok(attrs)
}

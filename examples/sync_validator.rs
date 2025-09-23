use anyhow::Result;
use hdf5::{File, types::VarLenUnicode};
use serde_json::{json, Value};
use std::path::Path;
use ndarray::Array1;

#[derive(Debug, Clone)]
struct StreamData {
    name: String,
    file_path: String,
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
    fn new(name: String, file_path: String) -> Self {
        Self {
            name,
            file_path,
            timestamps: Vec::new(),
            data_shape: (0, 0),
            stream_info: json!({}),
            recorder_config: json!({}),
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
    start_time_diff: f64,    // Max - Min start times
    end_time_diff: f64,      // Max - Min end times
    duration_diff: f64,      // Max - Min durations
    max_timestamp_drift: f64, // Maximum drift between streams
    is_synchronized: bool,
    sync_threshold: f64,     // Threshold for considering streams synchronized
}

fn load_hdf5_stream_data(file_path: &str) -> Result<Vec<StreamData>> {
    let path = Path::new(file_path);
    if !path.exists() {
        return Err(anyhow::anyhow!("File not found: {}", file_path));
    }

    let file = File::open(file_path)?;
    let mut streams = Vec::new();

    if let Ok(streams_group) = file.group("streams") {
        for stream_name in streams_group.member_names()? {
            let mut stream_data = StreamData::new(stream_name.clone(), file_path.to_string());

            let stream_group = streams_group.group(&stream_name)?;

            // Load timestamps
            if let Ok(time_dataset) = stream_group.dataset("time") {
                let timestamps_array: Array1<f64> = time_dataset.read_1d()?;
                stream_data.timestamps = timestamps_array.to_vec();
                stream_data.sample_count = stream_data.timestamps.len();

                if !stream_data.timestamps.is_empty() {
                    stream_data.start_time = stream_data.timestamps[0];
                    stream_data.end_time = stream_data.timestamps[stream_data.timestamps.len() - 1];
                    stream_data.duration = stream_data.end_time - stream_data.start_time;

                    // Calculate actual sample rate
                    if stream_data.sample_count > 1 {
                        stream_data.actual_sample_rate = (stream_data.sample_count - 1) as f64 / stream_data.duration;
                    }
                }
            }

            // Load data shape
            if let Ok(data_dataset) = stream_group.dataset("data") {
                let shape = data_dataset.shape();
                stream_data.data_shape = (shape[0], shape[1]); // (channels, samples)
                stream_data.channel_count = shape[0];
            }

            // Parse JSON metadata
            if let Ok(stream_info_raw) = stream_group.attr("stream_info_json") {
                let stream_info_unicode: VarLenUnicode = stream_info_raw.read_scalar()?;
                let stream_info_str = stream_info_unicode.to_string();
                if let Ok(parsed) = serde_json::from_str::<Value>(&stream_info_str) {
                    stream_data.stream_info = parsed.clone();

                    // Extract key information
                    if let Some(nominal_srate) = parsed.get("nominal_srate").and_then(|v| v.as_f64()) {
                        stream_data.nominal_sample_rate = nominal_srate;
                    }
                    if let Some(channel_format) = parsed.get("channel_format").and_then(|v| v.as_str()) {
                        stream_data.channel_format = channel_format.to_string();
                    }
                }
            }

            if let Ok(recorder_config_raw) = stream_group.attr("recorder_config_json") {
                let recorder_config_unicode: VarLenUnicode = recorder_config_raw.read_scalar()?;
                let recorder_config_str = recorder_config_unicode.to_string();
                if let Ok(parsed) = serde_json::from_str::<Value>(&recorder_config_str) {
                    stream_data.recorder_config = parsed;
                }
            }

            streams.push(stream_data);
        }
    }

    Ok(streams)
}

fn analyze_synchronization(streams: &[StreamData]) -> SyncAnalysis {
    let sync_threshold = 0.100; // 100ms threshold for synchronization

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

    let start_time_diff = start_times.iter().fold(f64::NAN, |a, &b| a.max(b)) -
                         start_times.iter().fold(f64::INFINITY, |a, &b| a.min(b));
    let end_time_diff = end_times.iter().fold(f64::NAN, |a, &b| a.max(b)) -
                       end_times.iter().fold(f64::INFINITY, |a, &b| a.min(b));
    let duration_diff = durations.iter().fold(f64::NAN, |a, &b| a.max(b)) -
                       durations.iter().fold(f64::INFINITY, |a, &b| a.min(b));

    // Calculate maximum timestamp drift between streams
    let mut max_drift: f64 = 0.0;
    if streams.len() > 1 {
        let min_length = streams.iter().map(|s| s.timestamps.len()).min().unwrap_or(0);
        for i in 0..min_length.min(100) { // Check first 100 samples for drift
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

    let is_synchronized = start_time_diff < sync_threshold &&
                         end_time_diff < sync_threshold &&
                         max_drift < sync_threshold;

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
    println!("📊 Stream: {}", stream.name);
    println!("   📁 File: {}", stream.file_path);
    println!("   📏 Data shape: {:?} (channels × samples)", stream.data_shape);
    println!("   🎵 Channels: {}", stream.channel_count);
    println!("   📈 Sample count: {}", stream.sample_count);
    println!("   ⏱️  Duration: {:.3} seconds", stream.duration);
    println!("   📡 Nominal sample rate: {:.1} Hz", stream.nominal_sample_rate);
    println!("   📊 Actual sample rate: {:.1} Hz", stream.actual_sample_rate);

    let rate_accuracy = if stream.nominal_sample_rate > 0.0 {
        (stream.actual_sample_rate / stream.nominal_sample_rate) * 100.0
    } else {
        0.0
    };
    println!("   ✅ Rate accuracy: {:.2}%", rate_accuracy);
    println!("   🔧 Channel format: {}", stream.channel_format);

    // Timing information
    println!("   🕐 Start time: {:.6}", stream.start_time);
    println!("   🕕 End time: {:.6}", stream.end_time);

    // Extract some key metadata if available
    if let Some(source_id) = stream.stream_info.get("source_id").and_then(|v| v.as_str()) {
        println!("   🆔 Source ID: {}", source_id);
    }
    if let Some(hostname) = stream.stream_info.get("hostname").and_then(|v| v.as_str()) {
        println!("   🖥️  Hostname: {}", hostname);
    }

    println!();
}

fn print_sync_analysis(analysis: &SyncAnalysis) {
    println!("🔄 SYNCHRONIZATION ANALYSIS");
    println!("================================");

    if analysis.is_synchronized {
        println!("✅ Status: SYNCHRONIZED");
    } else {
        println!("❌ Status: NOT SYNCHRONIZED");
    }

    println!("📏 Sync threshold: {:.1} ms", analysis.sync_threshold * 1000.0);
    println!();

    println!("⏱️  TIMING ANALYSIS:");
    println!("   Start time difference: {:.3} ms", analysis.start_time_diff * 1000.0);
    println!("   End time difference: {:.3} ms", analysis.end_time_diff * 1000.0);
    println!("   Duration difference: {:.3} ms", analysis.duration_diff * 1000.0);
    println!("   Maximum timestamp drift: {:.3} ms", analysis.max_timestamp_drift * 1000.0);
    println!();

    // Timeline visualization
    if !analysis.streams.is_empty() {
        println!("📅 RECORDING TIMELINE:");
        let min_start = analysis.streams.iter().map(|s| s.start_time).fold(f64::INFINITY, |a, b| a.min(b));
        let max_end = analysis.streams.iter().map(|s| s.end_time).fold(f64::NAN, |a, b| a.max(b));
        let total_duration = max_end - min_start;

        for stream in &analysis.streams {
            let start_offset = ((stream.start_time - min_start) / total_duration * 50.0) as usize;
            let duration_bars = ((stream.duration / total_duration * 50.0) as usize).max(1);

            let mut timeline = vec![' '; 52];
            for i in start_offset..(start_offset + duration_bars).min(50) {
                timeline[i] = '█';
            }
            let timeline_str: String = timeline.iter().collect();

            println!("   {:>10}: |{}|", stream.name, timeline_str);
        }
        println!("              |{:50}|", format!("{:.2}s", total_duration));
        println!();
    }
}

fn print_summary(analysis: &SyncAnalysis) {
    println!("📋 SUMMARY");
    println!("===========");
    println!("🗃️  Total streams analyzed: {}", analysis.streams.len());

    if analysis.streams.len() > 0 {
        let total_samples: usize = analysis.streams.iter().map(|s| s.sample_count).sum();
        let avg_duration = analysis.streams.iter().map(|s| s.duration).sum::<f64>() / analysis.streams.len() as f64;

        println!("📊 Total samples across all streams: {}", total_samples);
        println!("⏱️  Average recording duration: {:.3} seconds", avg_duration);

        if analysis.is_synchronized {
            println!("✅ All streams appear to be properly synchronized");
            println!("💡 The recordings should be suitable for multi-stream analysis");
        } else {
            println!("⚠️  Synchronization issues detected!");
            println!("💡 Consider checking:");
            println!("   • LSL clock synchronization");
            println!("   • Network timing issues");
            println!("   • Recording start/stop coordination");
        }
    }

    println!();
    println!("🔧 Run 'cargo run --example multi_recorder' to generate test files");
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    println!("🎯 LSL Multi-Stream Synchronization Validator");
    println!("=============================================");
    println!();

    let test_files = if args.len() > 1 {
        // Use command line arguments as file paths
        args[1..].to_vec()
    } else {
        // Default to standard multi-recorder files
        vec![
            "experiment_EMG.h5".to_string(),
            "experiment_EEG.h5".to_string(),
        ]
    };

    let mut all_streams = Vec::new();
    let mut _found_files = 0;

    // Load data from all available files
    for file_path in &test_files {
        match load_hdf5_stream_data(file_path) {
            Ok(mut streams) => {
                _found_files += 1;
                println!("✅ Loaded {} stream(s) from {}", streams.len(), file_path);
                all_streams.append(&mut streams);
            }
            Err(e) => {
                println!("⚠️  Could not load {}: {}", file_path, e);
            }
        }
    }

    if all_streams.is_empty() {
        println!("❌ No valid HDF5 files found!");
        println!("💡 Make sure to run 'cargo run --example multi_recorder' first");
        return Ok(());
    }

    println!();

    // Display individual stream information
    println!("📊 STREAM INFORMATION");
    println!("=====================");
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
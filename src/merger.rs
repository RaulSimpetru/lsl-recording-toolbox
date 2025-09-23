use anyhow::Result;
use hdf5::{types::VarLenUnicode, Dataset, File, Group};
use ndarray::{Array1, Array2};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct StreamInfo {
    pub name: String,
    pub source_file: String,
    pub data_shape: (usize, usize), // (channels, samples)
    pub channel_format: String,
    pub timestamps: Vec<f64>,
    pub stream_metadata: Value,
    pub recorder_config: Value,
    pub data_dataset: Option<Dataset>,
}

#[derive(Debug)]
pub struct MergerConfig {
    pub output_file: String,
    pub time_reference: TimeReference,
    pub conflict_resolution: ConflictResolution,
    pub preserve_provenance: bool,
    pub trim_start: bool,
    pub trim_end: bool,
}

#[derive(Debug)]
pub enum TimeReference {
    FirstStream,      // Use the earliest start time as reference
    LastStream,       // Use the latest start time as reference
    AbsoluteZero,     // Align all timestamps to start from 0
    KeepOriginal,     // Keep original timestamps
    CommonStart,      // Set 0.0 as the first timestamp where ALL streams have data
}

#[derive(Debug)]
pub enum ConflictResolution {
    Error,            // Fail on conflicts
    UseFirst,         // Use metadata from first encountered stream
    UseLast,          // Use metadata from last encountered stream
    Merge,            // Attempt to merge metadata intelligently
}

impl Default for MergerConfig {
    fn default() -> Self {
        Self {
            output_file: "merged_experiment.h5".to_string(),
            time_reference: TimeReference::FirstStream,
            conflict_resolution: ConflictResolution::Merge,
            preserve_provenance: true,
            trim_start: false,
            trim_end: false,
        }
    }
}

pub struct Hdf5Merger {
    config: MergerConfig,
    streams: Vec<StreamInfo>,
    global_metadata: HashMap<String, Value>,
}

impl Hdf5Merger {
    pub fn new(config: MergerConfig) -> Self {
        Self {
            config,
            streams: Vec::new(),
            global_metadata: HashMap::new(),
        }
    }

    /// Add an HDF5 file to be merged
    pub fn add_file<P: AsRef<Path>>(&mut self, file_path: P) -> Result<()> {
        let path = file_path.as_ref();
        let file_path_str = path.to_string_lossy().to_string();

        println!("📂 Loading: {}", file_path_str);

        let file = File::open(path)?;

        // Extract global metadata
        if let Ok(meta_group) = file.group("meta") {
            for attr_name in meta_group.attr_names()? {
                if let Ok(attr) = meta_group.attr(&attr_name) {
                    // Try to read as VarLenUnicode first, then as different types
                    let value = if let Ok(unicode_val) = attr.read_scalar::<VarLenUnicode>() {
                        Value::String(unicode_val.to_string())
                    } else if let Ok(f64_val) = attr.read_scalar::<f64>() {
                        Value::Number(serde_json::Number::from_f64(f64_val).unwrap_or_else(|| serde_json::Number::from(0)))
                    } else if let Ok(i64_val) = attr.read_scalar::<i64>() {
                        Value::Number(serde_json::Number::from(i64_val))
                    } else {
                        Value::String("unknown".to_string())
                    };

                    self.merge_global_metadata(attr_name, value);
                }
            }
        }

        // Extract streams
        if let Ok(streams_group) = file.group("streams") {
            for stream_name in streams_group.member_names()? {
                let stream_group = streams_group.group(&stream_name)?;

                let mut stream_info = StreamInfo {
                    name: stream_name.clone(),
                    source_file: file_path_str.clone(),
                    data_shape: (0, 0),
                    channel_format: String::new(),
                    timestamps: Vec::new(),
                    stream_metadata: json!({}),
                    recorder_config: json!({}),
                    data_dataset: None,
                };

                // Load timestamps
                if let Ok(time_dataset) = stream_group.dataset("time") {
                    let timestamps_array: Array1<f64> = time_dataset.read_1d()?;
                    stream_info.timestamps = timestamps_array.to_vec();
                }

                // Load data shape and keep reference to dataset
                if let Ok(data_dataset) = stream_group.dataset("data") {
                    let shape = data_dataset.shape();
                    stream_info.data_shape = (shape[0], shape[1]);
                    // Note: We can't store the dataset reference because it would keep the file open
                }

                // Load metadata
                if let Ok(stream_info_attr) = stream_group.attr("stream_info_json") {
                    if let Ok(unicode_val) = stream_info_attr.read_scalar::<VarLenUnicode>() {
                        if let Ok(parsed) = serde_json::from_str::<Value>(&unicode_val.to_string()) {
                            stream_info.stream_metadata = parsed.clone();
                            if let Some(format) = parsed.get("channel_format").and_then(|v| v.as_str()) {
                                stream_info.channel_format = format.to_string();
                            }
                        }
                    }
                }

                if let Ok(recorder_config_attr) = stream_group.attr("recorder_config_json") {
                    if let Ok(unicode_val) = recorder_config_attr.read_scalar::<VarLenUnicode>() {
                        if let Ok(parsed) = serde_json::from_str::<Value>(&unicode_val.to_string()) {
                            stream_info.recorder_config = parsed;
                        }
                    }
                }

                println!("  ✅ Stream: {} ({} samples, {} channels)",
                         stream_name, stream_info.timestamps.len(), stream_info.data_shape.0);

                self.streams.push(stream_info);
            }
        }

        Ok(())
    }

    fn merge_global_metadata(&mut self, key: String, value: Value) {
        match self.config.conflict_resolution {
            ConflictResolution::Error => {
                if self.global_metadata.contains_key(&key) {
                    eprintln!("⚠️  Metadata conflict for key '{}' - using error resolution", key);
                }
                self.global_metadata.insert(key, value);
            }
            ConflictResolution::UseFirst => {
                self.global_metadata.entry(key).or_insert(value);
            }
            ConflictResolution::UseLast => {
                self.global_metadata.insert(key, value);
            }
            ConflictResolution::Merge => {
                if let Some(existing) = self.global_metadata.get(&key) {
                    if existing != &value {
                        println!("🔀 Merging metadata for key '{}': {:?} + {:?}", key, existing, value);
                        // For now, create an array of values
                        let merged = json!([existing, value]);
                        self.global_metadata.insert(key, merged);
                    }
                } else {
                    self.global_metadata.insert(key, value);
                }
            }
        }
    }

    /// Calculate the common time window where all streams have data
    fn calculate_common_time_window(&self, alignment_offsets: &HashMap<String, f64>) -> (f64, f64) {
        if self.streams.is_empty() {
            return (0.0, 0.0);
        }

        let mut common_start = f64::NEG_INFINITY;
        let mut common_end = f64::INFINITY;

        for stream in &self.streams {
            if let Some(&offset) = alignment_offsets.get(&stream.name) {
                if let (Some(&first_ts), Some(&last_ts)) = (stream.timestamps.first(), stream.timestamps.last()) {
                    let aligned_start = first_ts + offset;
                    let aligned_end = last_ts + offset;

                    common_start = common_start.max(aligned_start);  // Latest start
                    common_end = common_end.min(aligned_end);        // Earliest end
                }
            }
        }

        // Ensure common_end is not before common_start
        if common_end < common_start {
            common_end = common_start;
        }

        (common_start, common_end)
    }

    /// Calculate time alignment based on the configured time reference
    fn calculate_time_alignment(&self) -> HashMap<String, f64> {
        let mut alignment_offsets = HashMap::new();

        if self.streams.is_empty() {
            return alignment_offsets;
        }

        let reference_time = match self.config.time_reference {
            TimeReference::FirstStream => {
                self.streams.iter()
                    .filter_map(|s| s.timestamps.first())
                    .fold(f64::INFINITY, |acc, &x| acc.min(x))
            }
            TimeReference::LastStream => {
                self.streams.iter()
                    .filter_map(|s| s.timestamps.first())
                    .fold(f64::NEG_INFINITY, |acc, &x| acc.max(x))
            }
            TimeReference::AbsoluteZero => 0.0,
            TimeReference::CommonStart => {
                // Find the latest start time among all streams
                // This ensures all streams have data from this point forward
                let common_start = self.streams.iter()
                    .filter_map(|s| s.timestamps.first())
                    .fold(f64::NEG_INFINITY, |acc, &x| acc.max(x));
                println!("🎯 Common start time: {:.6}s (latest stream start)", common_start);
                common_start
            }
            TimeReference::KeepOriginal => return alignment_offsets, // No offsets needed
        };

        for stream in &self.streams {
            if let Some(&first_timestamp) = stream.timestamps.first() {
                let offset = reference_time - first_timestamp;
                alignment_offsets.insert(stream.name.clone(), offset);
                println!("⏰ Stream '{}': offset = {:.6}s", stream.name, offset);
            }
        }

        alignment_offsets
    }

    /// Merge all loaded streams into a single HDF5 file
    pub fn merge(&self) -> Result<()> {
        println!("🔄 Starting merge process...");
        println!("📁 Output file: {}", self.config.output_file);

        let output_path = Path::new(&self.config.output_file);

        // Remove existing file if it exists
        if output_path.exists() {
            std::fs::remove_file(output_path)?;
            println!("🗑️  Removed existing output file");
        }

        // Calculate time alignment
        let alignment_offsets = self.calculate_time_alignment();

        // Create output file
        let output_file = File::create(output_path)?;

        // Create base structure
        let streams_group = output_file.create_group("streams")?;
        let sync_group = output_file.create_group("sync")?;
        let meta_group = output_file.create_group("meta")?;

        // Write global metadata
        for (key, value) in &self.global_metadata {
            match value {
                Value::String(s) => {
                    if let Ok(unicode_val) = VarLenUnicode::from_str(s) {
                        let _ = meta_group.new_attr::<VarLenUnicode>()
                            .create(key.as_str())
                            .and_then(|attr| attr.write_scalar(&unicode_val));
                    }
                }
                Value::Number(n) => {
                    if let Some(f_val) = n.as_f64() {
                        let _ = meta_group.new_attr::<f64>()
                            .create(key.as_str())
                            .and_then(|attr| attr.write_scalar(&f_val));
                    } else if let Some(i_val) = n.as_i64() {
                        let _ = meta_group.new_attr::<i64>()
                            .create(key.as_str())
                            .and_then(|attr| attr.write_scalar(&i_val));
                    }
                }
                _ => {
                    // For complex types, convert to JSON string
                    let json_str = serde_json::to_string(value)?;
                    if let Ok(unicode_val) = VarLenUnicode::from_str(&json_str) {
                        let _ = meta_group.new_attr::<VarLenUnicode>()
                            .create(key.as_str())
                            .and_then(|attr| attr.write_scalar(&unicode_val));
                    }
                }
            }
        }

        // Add merge metadata
        if self.config.preserve_provenance {
            let merge_time = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64();
            let _ = meta_group.new_attr::<f64>()
                .create("merged_at")
                .and_then(|attr| attr.write_scalar(&merge_time));

            let source_files: Vec<String> = self.streams.iter()
                .map(|s| s.source_file.clone())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();

            let sources_json = json!(source_files);
            if let Ok(unicode_val) = VarLenUnicode::from_str(&serde_json::to_string(&sources_json)?) {
                let _ = meta_group.new_attr::<VarLenUnicode>()
                    .create("source_files")
                    .and_then(|attr| attr.write_scalar(&unicode_val));
            }
        }

        // Merge each stream
        for stream in &self.streams {
            println!("🔀 Merging stream: {}", stream.name);
            self.merge_stream(&streams_group, stream, &alignment_offsets)?;
        }

        // Write synchronization information
        if !alignment_offsets.is_empty() {
            let offsets_json = json!(alignment_offsets);
            if let Ok(unicode_val) = VarLenUnicode::from_str(&serde_json::to_string(&offsets_json)?) {
                let _ = sync_group.new_attr::<VarLenUnicode>()
                    .create("time_alignment_offsets")
                    .and_then(|attr| attr.write_scalar(&unicode_val));
            }
        }

        println!("✅ Merge completed successfully!");
        println!("📊 Merged {} streams from {} files", self.streams.len(),
                 self.streams.iter().map(|s| &s.source_file).collect::<std::collections::HashSet<_>>().len());

        Ok(())
    }

    fn merge_stream(&self, streams_group: &Group, stream: &StreamInfo, alignment_offsets: &HashMap<String, f64>) -> Result<()> {
        // Create stream group
        let stream_group = streams_group.create_group(&stream.name)?;

        // Copy metadata
        if let Ok(unicode_val) = VarLenUnicode::from_str(&serde_json::to_string(&stream.stream_metadata)?) {
            stream_group.new_attr::<VarLenUnicode>()
                .create("stream_info_json")?
                .write_scalar(&unicode_val)?;
        }

        if let Ok(unicode_val) = VarLenUnicode::from_str(&serde_json::to_string(&stream.recorder_config)?) {
            stream_group.new_attr::<VarLenUnicode>()
                .create("recorder_config_json")?
                .write_scalar(&unicode_val)?;
        }

        // Apply time alignment
        let aligned_timestamps: Vec<f64> = if let Some(&offset) = alignment_offsets.get(&stream.name) {
            stream.timestamps.iter().map(|&t| t + offset).collect()
        } else {
            stream.timestamps.clone()
        };

        // Apply trimming if enabled
        let (final_timestamps, trim_start_index, trim_end_count) = if self.config.trim_start && self.config.trim_end {
            // Calculate common time window for dual trimming
            let (common_start, common_end) = self.calculate_common_time_window(alignment_offsets);

            println!("  🎯 Common time window: {:.6}s to {:.6}s ({:.3}s duration)",
                     common_start, common_end, common_end - common_start);

            // Find start index - trim to common start
            let start_idx = aligned_timestamps.iter().position(|&t| t >= common_start).unwrap_or(0);

            // Find end index - trim to common end
            let end_idx = aligned_timestamps.iter().rposition(|&t| t <= common_end)
                .map(|i| i + 1)  // +1 to make it exclusive end
                .unwrap_or(aligned_timestamps.len());

            let start_trimmed = start_idx;
            let end_trimmed = aligned_timestamps.len() - end_idx;

            if start_trimmed > 0 || end_trimmed > 0 {
                println!("  ✂️  Trimming {} samples at start, {} samples at end for stream '{}'",
                         start_trimmed, end_trimmed, stream.name);
            }

            (aligned_timestamps[start_idx..end_idx].to_vec(), start_idx, end_trimmed)
        } else if self.config.trim_start {
            // Trim samples before t=0.0 or common start
            let trim_index = aligned_timestamps.iter()
                .position(|&t| t >= 0.0)
                .unwrap_or(0);

            if trim_index > 0 {
                println!("  ✂️  Trimming {} samples before t=0.0 for stream '{}'", trim_index, stream.name);
            }

            (aligned_timestamps[trim_index..].to_vec(), trim_index, 0)
        } else if self.config.trim_end {
            // Trim samples after common end time
            let (_, common_end) = self.calculate_common_time_window(alignment_offsets);
            let end_idx = aligned_timestamps.iter().rposition(|&t| t <= common_end)
                .map(|i| i + 1)  // +1 to make it exclusive end
                .unwrap_or(aligned_timestamps.len());

            let end_trimmed = aligned_timestamps.len() - end_idx;
            if end_trimmed > 0 {
                println!("  ✂️  Trimming {} samples at end for stream '{}'", end_trimmed, stream.name);
            }

            (aligned_timestamps[..end_idx].to_vec(), 0, end_trimmed)
        } else {
            (aligned_timestamps, 0, 0)
        };

        // Create and write time dataset
        let time_array = Array1::from_vec(final_timestamps);
        let time_dataset = stream_group.new_dataset::<f64>()
            .shape(time_array.len())
            .create("time")?;
        time_dataset.write(&time_array)?;

        // Copy data from source file - we need to reopen the source file
        let source_file = File::open(&stream.source_file)?;
        let source_streams_group = source_file.group("streams")?;
        let source_stream_group = source_streams_group.group(&stream.name)?;
        let source_data_dataset = source_stream_group.dataset("data")?;

        // Calculate final dimensions after trimming
        let (channels, original_samples) = stream.data_shape;
        let final_samples = original_samples - trim_start_index - trim_end_count;

        macro_rules! copy_data {
            ($type:ty) => {{
                let source_data: Array2<$type> = source_data_dataset.read_2d()?;

                // Apply dual trimming if needed
                let final_data = if trim_start_index > 0 || trim_end_count > 0 {
                    // Calculate end index for trimming
                    let end_index = original_samples - trim_end_count;
                    // Slice the data to remove samples at both ends
                    // Data is stored as (channels, samples), so we trim along the sample dimension
                    source_data.slice(ndarray::s![.., trim_start_index..end_index]).to_owned()
                } else {
                    source_data
                };

                let data_dataset = stream_group.new_dataset::<$type>()
                    .shape((channels, final_samples))
                    .create("data")?;
                data_dataset.write(&final_data)?;
            }};
        }

        // Determine data type and copy accordingly
        match stream.channel_format.as_str() {
            "Float32" => copy_data!(f32),
            "Double64" => copy_data!(f64),
            "Int32" => copy_data!(i32),
            "Int16" => copy_data!(i16),
            "Int8" => copy_data!(i8),
            _ => {
                // Default to Float32 if unknown
                copy_data!(f32);
            }
        }

        if trim_start_index > 0 || trim_end_count > 0 {
            println!("  ✅ Copied {} samples ({} channels, {} start + {} end trimmed) for stream '{}'",
                     final_samples, channels, trim_start_index, trim_end_count, stream.name);
        } else {
            println!("  ✅ Copied {} samples ({} channels) for stream '{}'",
                     final_samples, channels, stream.name);
        }

        Ok(())
    }

    /// Get summary of loaded streams
    pub fn summary(&self) -> String {
        let mut summary = format!("📋 Merger Summary\n");
        summary.push_str(&format!("==================\n"));
        summary.push_str(&format!("🗃️  Streams loaded: {}\n", self.streams.len()));
        summary.push_str(&format!("📁 Output file: {}\n", self.config.output_file));
        summary.push_str(&format!("⏰ Time reference: {:?}\n", self.config.time_reference));
        summary.push_str(&format!("🔀 Conflict resolution: {:?}\n", self.config.conflict_resolution));
        summary.push_str(&format!("✂️  Trim start: {}\n", self.config.trim_start));
        summary.push_str(&format!("🎯 Trim end: {}\n", self.config.trim_end));
        summary.push_str("\n📊 Stream Details:\n");

        for stream in &self.streams {
            summary.push_str(&format!("  • {} ({} samples, {} channels) from {}\n",
                                     stream.name, stream.timestamps.len(), stream.data_shape.0, stream.source_file));
        }

        summary
    }
}
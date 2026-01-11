pub mod writer;

use anyhow::Result;
use fs2::FileExt;
use serde_json::json;
use std::fs::OpenOptions;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use zarrs::array::{Array, ArrayBuilder, DataType, FillValue};
use zarrs::array::codec::{BloscCodec, BloscCompressionLevel, BloscCompressor, BloscShuffleMode};
use zarrs::filesystem::FilesystemStore;
use zarrs::group::GroupBuilder;
use zarrs::storage::{StoreKey, ReadableStorageTraits};

/// Initialize or open Zarr store with base structure, handling concurrent access
pub fn open_or_create_zarr_store(
    store_path: &Path,
    _subject: Option<&str>,
    _session_id: Option<&str>,
    _notes: Option<&str>,
) -> Result<Arc<FilesystemStore>> {
    println!("Writing to Zarr store: {:?}", store_path);

    // Create the store directory if it doesn't exist
    std::fs::create_dir_all(store_path)?;

    // Create filesystem store
    let store = Arc::new(FilesystemStore::new(store_path)?);

    // Use file locking to coordinate concurrent access during initialization
    let lock_path = store_path.join(".zarr_init.lock");
    let lock_file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)?;

    // Acquire exclusive lock for initialization
    lock_file.lock_exclusive()?;

    // Initialize base structure if needed (protected by lock)
    let mut last_error = None;
    for attempt in 0..2 {
        match initialize_store_structure(&store) {
            Ok(_) => {
                lock_file.unlock()?;
                return Ok(store);
            }
            Err(e) => {
                eprintln!(
                    "Warning: Failed to initialize Zarr store (attempt {}): {}",
                    attempt + 1,
                    e
                );
                last_error = Some(e);
                std::thread::sleep(Duration::from_millis(10 + fastrand::u64(0..20)));
            }
        }
    }

    lock_file.unlock()?;
    Err(anyhow::anyhow!(
        "Failed to initialize Zarr store after 2 attempts: {}",
        last_error.unwrap()
    ))
}

/// Initialize Zarr store with base group structure
fn initialize_store_structure(
    store: &Arc<FilesystemStore>,
) -> Result<()> {
    // Create root group if it doesn't exist
    if !group_exists(store, "/")? {
        let root_group = GroupBuilder::new().build(store.clone(), "/")?;
        root_group.store_metadata()?;
    }

    Ok(())
}

/// Check if a Zarr group exists (Zarr v3 uses zarr.json with node_type)
fn group_exists(store: &Arc<FilesystemStore>, path: &str) -> Result<bool> {
    let trimmed_path = path.trim_end_matches('/');
    let metadata_path = if trimmed_path.is_empty() || trimmed_path == "/" {
        "zarr.json".to_string()  // Root group
    } else {
        format!("{}/zarr.json", trimmed_path.trim_start_matches('/'))
    };
    let metadata_key = StoreKey::new(&metadata_path)?;

    match store.get(&metadata_key) {
        Ok(Some(data)) => {
            // Parse JSON and check node_type
            let json: serde_json::Value = serde_json::from_slice(&data)?;
            Ok(json.get("node_type").and_then(|v| v.as_str()) == Some("group"))
        }
        _ => Ok(false),
    }
}

/// Create a Zarr group if it doesn't exist
fn create_group_if_not_exists(store: &Arc<FilesystemStore>, path: &str) -> Result<()> {
    if !group_exists(store, path)? {
        let group = GroupBuilder::new().build(store.clone(), path)?;
        group.store_metadata()?;
    }
    Ok(())
}


/// Serialize LSL StreamInfo to JSON value
fn serialize_stream_info(info: &mut lsl::StreamInfo) -> Result<serde_json::Value> {
    // Get full XML representation and extract just the <desc> element
    let full_xml = info.to_xml()
        .map_err(|e| anyhow::anyhow!("Failed to serialize stream info XML: {}", e))?;

    // Parse <desc>...</desc> content to JSON to avoid duplicating basic stream info
    let description_json = parse_desc_to_json(&full_xml);

    let stream_info_json = json!({
        "type": info.stream_type(),
        "source_id": info.source_id(),
        "hostname": info.hostname(),
        "channel_count": info.channel_count(),
        "nominal_srate": info.nominal_srate(),
        "channel_format": format!("{:?}", info.channel_format()),
        "created_at": info.created_at(),
        "uid": info.uid(),
        "session_id": info.session_id(),
        "version": info.version(),
        "description": description_json
    });

    Ok(stream_info_json)
}

/// Parse the <desc> element from LSL XML to JSON using quick-xml
fn parse_desc_to_json(xml: &str) -> serde_json::Value {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut in_desc = false;
    let mut depth = 0;
    let mut desc_xml = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) if e.name().as_ref() == b"desc" => {
                in_desc = true;
                depth = 1;
            }
            Ok(Event::Start(e)) if in_desc => {
                depth += 1;
                desc_xml.extend_from_slice(b"<");
                desc_xml.extend_from_slice(e.name().as_ref());
                desc_xml.extend_from_slice(b">");
            }
            Ok(Event::End(e)) if in_desc => {
                depth -= 1;
                if depth == 0 {
                    // Finished reading desc element
                    let desc_content = String::from_utf8_lossy(&desc_xml).to_string();
                    return parse_xml_to_json(&desc_content);
                }
                desc_xml.extend_from_slice(b"</");
                desc_xml.extend_from_slice(e.name().as_ref());
                desc_xml.extend_from_slice(b">");
            }
            Ok(Event::Text(e)) if in_desc => {
                desc_xml.extend_from_slice(&e);
            }
            Ok(Event::Empty(e)) if e.name().as_ref() == b"desc" => {
                // Empty desc element
                return serde_json::Value::Object(serde_json::Map::new());
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                tracing::warn!("Error parsing LSL XML: {}", e);
                break;
            }
            _ => {}
        }
    }

    serde_json::Value::Object(serde_json::Map::new())
}

/// Parse XML string to JSON recursively using quick-xml
fn parse_xml_to_json(xml: &str) -> serde_json::Value {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut result = serde_json::Map::new();
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut current_tag = String::new();
    let mut current_text = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                current_tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                current_text.clear();
            }
            Ok(Event::Text(e)) => {
                if let Ok(text) = e.unescape() {
                    current_text.push_str(&text);
                }
            }
            Ok(Event::End(_)) => {
                if !current_tag.is_empty() {
                    result.insert(current_tag.clone(), serde_json::Value::String(current_text.clone()));
                    current_tag.clear();
                    current_text.clear();
                }
            }
            Ok(Event::Empty(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                result.insert(tag, serde_json::Value::String(String::new()));
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                tracing::warn!("Error parsing XML element: {}", e);
                break;
            }
            _ => {}
        }
    }

    serde_json::Value::Object(result)
}

/// Parse recorder config JSON string to serde_json::Value
fn parse_recorder_config(recorder_config_json: &str) -> Result<serde_json::Value> {
    let config: serde_json::Value = serde_json::from_str(recorder_config_json)?;
    Ok(config)
}

/// Get dtype for Zarr array based on LSL channel format
fn get_zarr_dtype(channel_format: lsl::ChannelFormat) -> Result<DataType> {
    match channel_format {
        lsl::ChannelFormat::Float32 => Ok(DataType::Float32),
        lsl::ChannelFormat::Double64 => Ok(DataType::Float64),
        lsl::ChannelFormat::Int32 => Ok(DataType::Int32),
        lsl::ChannelFormat::Int16 => Ok(DataType::Int16),
        lsl::ChannelFormat::Int8 => Ok(DataType::Int8),
        lsl::ChannelFormat::String => Ok(DataType::String),
        _ => Err(anyhow::anyhow!(
            "Unsupported channel format for Zarr: {:?}",
            channel_format
        )),
    }
}

/// Get typesize for Blosc compression based on LSL channel format
fn get_blosc_typesize(channel_format: lsl::ChannelFormat) -> Option<usize> {
    match channel_format {
        lsl::ChannelFormat::Float32 => Some(4),  // 4 bytes
        lsl::ChannelFormat::Double64 => Some(8),  // 8 bytes
        lsl::ChannelFormat::Int32 => Some(4),  // 4 bytes
        lsl::ChannelFormat::Int16 => Some(2),  // 2 bytes
        lsl::ChannelFormat::Int8 => Some(1),   // 1 byte
        _ => None,  // String or unsupported
    }
}

/// Setup stream arrays (data and time) in the Zarr store
pub fn setup_stream_arrays(
    store: &Arc<FilesystemStore>,
    stream_name: &str,
    info: &mut lsl::StreamInfo,
    channel_format: lsl::ChannelFormat,
    recorder_config_json: &str,
    time_correction: f64,
    first_timestamp: Option<f64>,
) -> Result<(Array<FilesystemStore>, Array<FilesystemStore>)> {
    // Create stream group (use absolute path with /)
    let stream_path = format!("/{}", stream_name);
    create_group_if_not_exists(store, &stream_path)?;

    // Prepare sync metadata (will be added to stream group attributes)
    let mut sync_attrs = serde_json::Map::new();
    sync_attrs.insert("lsl_clock_offset".to_string(), json!(time_correction));
    sync_attrs.insert("recorded_at".to_string(), json!(chrono::Utc::now().to_rfc3339()));
    if let Some(first_ts) = first_timestamp {
        sync_attrs.insert("first_timestamp".to_string(), json!(first_ts));
    }

    // Create or get data array (use absolute path with /)
    let data_path = format!("{}/data", stream_path);
    let data_array = if array_exists(store, &data_path)? {
        Array::open(store.clone(), &data_path)?
    } else {
        let channels = info.channel_count() as usize;
        let dtype = get_zarr_dtype(channel_format)?;

        // Select shuffle mode based on data type for optimal compression
        // BitShuffle: best for floating-point (EMG/EEG signals)
        // Shuffle: best for integers
        let shuffle_mode = match channel_format {
            lsl::ChannelFormat::Float32 | lsl::ChannelFormat::Double64 => BloscShuffleMode::BitShuffle,
            lsl::ChannelFormat::Int32 | lsl::ChannelFormat::Int16 | lsl::ChannelFormat::Int8 => BloscShuffleMode::Shuffle,
            _ => BloscShuffleMode::NoShuffle, // String (not compressed anyway)
        };

        // Get typesize for Blosc (required when shuffling is enabled)
        let typesize = get_blosc_typesize(channel_format);

        // Create Blosc codec with LZ4 compression (not used for String type)
        let compression_level = BloscCompressionLevel::try_from(5u8)
            .map_err(|e| anyhow::anyhow!("Invalid compression level: {}", e))?;
        let blosc_codec = Arc::new(BloscCodec::new(
            BloscCompressor::LZ4,
            compression_level,
            None,  // blocksize (auto-detect)
            shuffle_mode,
            typesize,  // typesize required for shuffling
        )?);

        // Select appropriate fill value and build array based on data type
        let array = if matches!(channel_format, lsl::ChannelFormat::String) {
            // String arrays: no compression, empty string fill value
            ArrayBuilder::new(
                vec![channels as u64, 0], // [channels, samples] - samples dimension is unlimited
                vec![channels as u64, 100], // chunk size: [channels, 100 samples]
                dtype,
                FillValue::from(""),
            )
            .dimension_names(Some(vec![
                Some("channels".to_string()),
                Some("samples".to_string()),
            ]))
            .build(store.clone(), &data_path)?
        } else {
            // Numeric arrays: with Blosc compression
            ArrayBuilder::new(
                vec![channels as u64, 0], // [channels, samples] - samples dimension is unlimited
                vec![channels as u64, 100], // chunk size: [channels, 100 samples]
                dtype,
                FillValue::from(0.0f32),
            )
            .dimension_names(Some(vec![
                Some("channels".to_string()),
                Some("samples".to_string()),
            ]))
            .bytes_to_bytes_codecs(vec![blosc_codec])
            .build(store.clone(), &data_path)?
        };

        array.store_metadata()?;

        // Store metadata in the stream group instead of on the array
        let mut stream_group = zarrs::group::Group::open(store.clone(), &stream_path)?;
        let mut stream_attrs = serde_json::Map::new();
        stream_attrs.insert("stream_info".to_string(), serialize_stream_info(info)?);
        stream_attrs.insert("recorder_config".to_string(), parse_recorder_config(recorder_config_json)?);
        // Add sync metadata to stream attributes
        stream_attrs.extend(sync_attrs);
        stream_group.attributes_mut().extend(stream_attrs);
        stream_group.store_metadata()?;

        array
    };

    // Create or get time array
    let time_path = format!("{}/time", stream_path);
    let time_array = if array_exists(store, &time_path)? {
        Array::open(store.clone(), &time_path)?
    } else {
        // Create Blosc codec with BitShuffle for optimal float64 timestamp compression
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
            vec![0], // unlimited dimension
            vec![100], // chunk size: 100 samples
            DataType::Float64,
            FillValue::from(0.0f64),
        )
        .dimension_names(Some(vec![Some("samples".to_string())]))
        .bytes_to_bytes_codecs(vec![blosc_codec])
        .build(store.clone(), &time_path)?;

        array.store_metadata()?;

        // Note: Array-level attributes are not set via API in zarr-rs
        // Time array description is self-evident from the array name

        array
    };

    Ok((data_array, time_array))
}

/// Read attributes from a group's zarr.json file (Zarr v3 format)
pub fn read_group_attributes(store: &Arc<FilesystemStore>, path: &str) -> Result<serde_json::Value> {
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
    let zarr_metadata: serde_json::Value = serde_json::from_slice(&zarr_bytes)?;

    Ok(zarr_metadata
        .get("attributes")
        .cloned()
        .unwrap_or_else(|| json!({})))
}

/// Check if a Zarr array exists (Zarr v3 uses zarr.json with node_type)
fn array_exists(store: &Arc<FilesystemStore>, path: &str) -> Result<bool> {
    let trimmed_path = path.trim_end_matches('/').trim_start_matches('/');
    let metadata_path = format!("{}/zarr.json", trimmed_path);
    let metadata_key = StoreKey::new(&metadata_path)?;

    match store.get(&metadata_key) {
        Ok(Some(data)) => {
            // Parse JSON and check node_type
            let json: serde_json::Value = serde_json::from_slice(&data)?;
            Ok(json.get("node_type").and_then(|v| v.as_str()) == Some("array"))
        }
        _ => Ok(false),
    }
}


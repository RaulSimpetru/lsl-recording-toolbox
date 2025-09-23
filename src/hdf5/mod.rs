pub mod writer;

use anyhow::Result;
use hdf5::types::VarLenUnicode;
use hdf5::{Dataset, File, Group};
use serde_json::json;
use std::path::Path;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

/// Initialize or open HDF5 file with base structure, handling concurrent access
pub fn open_or_create_hdf5_file(
    file_path: &Path,
    subject: Option<&str>,
    session_id: Option<&str>,
    notes: Option<&str>,
) -> Result<File> {
    use std::time::Duration;

    println!("Writing to file: {:?}", file_path);

    // Try to open existing file first
    if file_path.exists() {
        // Try to open existing file with fast retries
        for attempt in 0..2 {
            match File::open_rw(file_path) {
                Ok(file) => return Ok(file),
                Err(e) => {
                    if attempt < 1 {
                        eprintln!(
                            "Warning: Failed to open existing HDF5 file (attempt {}): {}",
                            attempt + 1,
                            e
                        );
                        std::thread::sleep(Duration::from_millis(10 + fastrand::u64(0..20))); // 10-30ms with jitter
                    } else {
                        return Err(anyhow::anyhow!(
                            "Failed to open existing HDF5 file after 2 attempts: {}",
                            e
                        ));
                    }
                }
            }
        }
    }

    // File doesn't exist, try to create it
    for attempt in 0..2 {
        match create_hdf5_file_with_structure(file_path, subject, session_id, notes) {
            Ok(file) => return Ok(file),
            Err(e) => {
                // Check if file was created by another process while we were trying
                if file_path.exists() {
                    // Another process created it, try to open it
                    match File::open_rw(file_path) {
                        Ok(file) => return Ok(file),
                        Err(open_err) => {
                            eprintln!(
                                "Warning: File exists but cannot open (attempt {}): {}",
                                attempt + 1,
                                open_err
                            );
                        }
                    }
                }

                if attempt < 1 {
                    eprintln!(
                        "Warning: Failed to create HDF5 file (attempt {}): {}",
                        attempt + 1,
                        e
                    );
                    std::thread::sleep(Duration::from_millis(5 + fastrand::u64(0..15))); // 5-20ms with jitter
                } else {
                    return Err(anyhow::anyhow!(
                        "Failed to create HDF5 file after 2 attempts: {}",
                        e
                    ));
                }
            }
        }
    }

    unreachable!()
}

/// Create HDF5 file with base structure, handling the case where groups already exist
fn create_hdf5_file_with_structure(
    file_path: &Path,
    subject: Option<&str>,
    session_id: Option<&str>,
    notes: Option<&str>,
) -> Result<File> {
    let file = File::create(file_path)?;

    // Create base structure - handle case where groups already exist
    let _ = file
        .create_group("streams")
        .or_else(|_| file.group("streams"));
    let _ = file.create_group("sync").or_else(|_| file.group("sync"));

    let meta_group = file.create_group("meta").or_else(|_| file.group("meta"))?;

    // Add metadata if provided - ignore errors if attributes already exist
    if let Some(subject) = subject {
        if let Ok(subject_unicode) = VarLenUnicode::from_str(subject) {
            let _ = meta_group
                .new_attr::<VarLenUnicode>()
                .create("subject")
                .and_then(|attr| attr.write_scalar(&subject_unicode));
        }
    }

    if let Some(session_id) = session_id {
        if let Ok(session_unicode) = VarLenUnicode::from_str(session_id) {
            let _ = meta_group
                .new_attr::<VarLenUnicode>()
                .create("session_id")
                .and_then(|attr| attr.write_scalar(&session_unicode));
        }
    }

    if let Some(notes) = notes {
        if let Ok(notes_unicode) = VarLenUnicode::from_str(notes) {
            let _ = meta_group
                .new_attr::<VarLenUnicode>()
                .create("notes")
                .and_then(|attr| attr.write_scalar(&notes_unicode));
        }
    }

    // Add start time and global reference - ignore errors if they already exist
    let start_time = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64();
    let _ = meta_group
        .new_attr::<f64>()
        .create("start_time")
        .and_then(|attr| attr.write_scalar(&start_time));

    if let Ok(ref_unicode) = VarLenUnicode::from_str("LSL clock of recorder host") {
        let _ = meta_group
            .new_attr::<VarLenUnicode>()
            .create("global_reference")
            .and_then(|attr| attr.write_scalar(&ref_unicode));
    }

    Ok(file)
}

/// Serialize LSL StreamInfo to JSON string
fn serialize_stream_info(info: &lsl::StreamInfo) -> Result<String> {
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
        "version": info.version()
    });

    Ok(serde_json::to_string_pretty(&stream_info_json)?)
}

/// Create or get stream group with datasets for a specific stream
pub fn setup_stream_group(
    file: &File,
    stream_name: &str,
    info: &lsl::StreamInfo,
    channel_format: lsl::ChannelFormat,
    recorder_config_json: &str,
) -> Result<(Group, Dataset, Dataset)> {
    let streams_group = file.group("streams")?;

    // Create or get stream group
    let stream_group = if streams_group.link_exists(stream_name) {
        streams_group.group(stream_name)?
    } else {
        let group = streams_group.create_group(stream_name)?;

        // Add complete stream info as JSON
        let stream_info_json = serialize_stream_info(info)?;
        group
            .new_attr::<VarLenUnicode>()
            .create("stream_info_json")?
            .write_scalar(&VarLenUnicode::from_str(&stream_info_json)?)?;

        // Add complete recorder config as JSON
        group
            .new_attr::<VarLenUnicode>()
            .create("recorder_config_json")?
            .write_scalar(&VarLenUnicode::from_str(recorder_config_json)?)?;

        group
    };

    // Create or get data dataset with appropriate type
    let data_dataset = if stream_group.link_exists("data") {
        stream_group.dataset("data")?
    } else {
        let channels = info.channel_count() as usize;

        macro_rules! create_dataset {
            ($type:ty) => {
                stream_group
                    .new_dataset::<$type>()
                    .chunk((channels, 100))
                    .shape((hdf5::Extent::fixed(channels), hdf5::Extent::resizable(0)))
                    .create("data")?
            };
        }

        match channel_format {
            lsl::ChannelFormat::Float32 => create_dataset!(f32),
            lsl::ChannelFormat::Double64 => create_dataset!(f64),
            lsl::ChannelFormat::Int32 => create_dataset!(i32),
            lsl::ChannelFormat::Int16 => create_dataset!(i16),
            lsl::ChannelFormat::Int8 => create_dataset!(i8),
            _ => {
                return Err(anyhow::anyhow!(
                    "Unsupported channel format for HDF5: {:?}",
                    channel_format
                ))
            }
        }
    };

    // Create or get time dataset
    let time_dataset = if stream_group.link_exists("time") {
        stream_group.dataset("time")?
    } else {
        stream_group
            .new_dataset::<f64>()
            .chunk(100)
            .shape(hdf5::Extent::resizable(0))
            .create("time")?
    };

    Ok((stream_group, data_dataset, time_dataset))
}

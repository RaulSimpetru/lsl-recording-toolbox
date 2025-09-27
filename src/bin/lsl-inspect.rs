use anyhow::Result;
use hdf5::{types::VarLenUnicode, File};
use serde_json::Value;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let default_file = "merged_test.h5".to_string();
    let file_path = args.get(1).unwrap_or(&default_file);

    println!("HDF5 Metadata Inspector");
    println!("=======================");
    println!("File: {}", file_path);
    println!();

    let file = File::open(file_path)?;

    // Inspect global metadata
    if let Ok(meta_group) = file.group("meta") {
        println!("GLOBAL METADATA:");
        for attr_name in meta_group.attr_names()? {
            if let Ok(attr) = meta_group.attr(&attr_name) {
                if let Ok(unicode_val) = attr.read_scalar::<VarLenUnicode>() {
                    let value_str = unicode_val.to_string();
                    println!(
                        "\t{}: {}",
                        attr_name,
                        if value_str.len() > 100 {
                            format!("{}... ({} chars)", &value_str[..100], value_str.len())
                        } else {
                            value_str
                        }
                    );
                } else if let Ok(f64_val) = attr.read_scalar::<f64>() {
                    println!("\t{}: {:.6}", attr_name, f64_val);
                }
            }
        }
        println!();
    }

    // Inspect streams
    if let Ok(streams_group) = file.group("streams") {
        println!("STREAMS:");
        for stream_name in streams_group.member_names()? {
            println!("\tStream: {}", stream_name);

            let stream_group = streams_group.group(&stream_name)?;

            // Show datasets
            for member in stream_group.member_names()? {
                if let Ok(dataset) = stream_group.dataset(&member) {
                    println!("\t\tDataset '{}': shape {:?}", member, dataset.shape());
                }
            }

            // Show attributes
            for attr_name in stream_group.attr_names()? {
                if let Ok(attr) = stream_group.attr(&attr_name) {
                    if let Ok(unicode_val) = attr.read_scalar::<VarLenUnicode>() {
                        let json_str = unicode_val.to_string();
                        if let Ok(parsed) = serde_json::from_str::<Value>(&json_str) {
                            println!("\t\tAttribute '{}' (JSON):", attr_name);
                            if attr_name == "stream_info_json" {
                                // Show key stream info fields
                                if let Some(source_id) = parsed.get("source_id") {
                                    println!("\t\t\tsource_id: {}", source_id);
                                }
                                if let Some(hostname) = parsed.get("hostname") {
                                    println!("\t\t\thostname: {}", hostname);
                                }
                                if let Some(channel_count) = parsed.get("channel_count") {
                                    println!("\t\t\tchannel_count: {}", channel_count);
                                }
                                if let Some(nominal_srate) = parsed.get("nominal_srate") {
                                    println!("\t\t\tnominal_srate: {}", nominal_srate);
                                }
                                if let Some(channel_format) = parsed.get("channel_format") {
                                    println!("\t\t\tchannel_format: {}", channel_format);
                                }
                            } else if attr_name == "recorder_config_json" {
                                // Show key recorder config fields
                                if let Some(subject) = parsed.get("subject") {
                                    println!("\t\t\tsubject: {}", subject);
                                }
                                if let Some(session_id) = parsed.get("session_id") {
                                    println!("\t\t\tsession_id: {}", session_id);
                                }
                                if let Some(recorded_at) = parsed.get("recorded_at") {
                                    println!("\t\t\trecorded_at: {}", recorded_at);
                                }
                                if let Some(recorder_version) = parsed.get("recorder_version") {
                                    println!("\t\t\trecorder_version: {}", recorder_version);
                                }
                            }
                        } else {
                            println!("\t\tAttribute '{}': {} chars", attr_name, json_str.len());
                        }
                    }
                }
            }
            println!();
        }
    }

    // Inspect sync metadata
    if let Ok(sync_group) = file.group("sync") {
        println!("SYNCHRONIZATION METADATA:");
        for attr_name in sync_group.attr_names()? {
            if let Ok(attr) = sync_group.attr(&attr_name) {
                if let Ok(unicode_val) = attr.read_scalar::<VarLenUnicode>() {
                    let json_str = unicode_val.to_string();
                    if let Ok(parsed) = serde_json::from_str::<Value>(&json_str) {
                        println!(
                            "\t{}: {}",
                            attr_name,
                            serde_json::to_string_pretty(&parsed)?
                        );
                    } else {
                        println!("\t{}: {}", attr_name, json_str);
                    }
                }
            }
        }
    }

    Ok(())
}

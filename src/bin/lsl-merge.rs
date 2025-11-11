use anyhow::Result;
use clap::Parser;
use ndarray::IxDyn;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use zarrs::array::Array;
use zarrs::array_subset::ArraySubset;
use zarrs::filesystem::FilesystemStore;
use zarrs::group::GroupBuilder;
use zarrs::storage::{ReadableStorageTraits, StoreKey, WritableStorageTraits};

#[derive(Parser)]
#[command(name = "lsl-merge")]
#[command(about = "Merge multiple Zarr stores created by lsl-recorder into a single store")]
struct Args {
    /// Input Zarr stores to merge
    #[arg(help = "Zarr stores to merge (e.g., experiment_EMG.zarr experiment_EEG.zarr)")]
    input_stores: Vec<PathBuf>,

    #[arg(
        short = 'o',
        long = "output",
        help = "Output Zarr store path",
        default_value = "merged_experiment.zarr"
    )]
    output: PathBuf,

    #[arg(
        short = 'v',
        long = "verbose",
        help = "Verbose output with detailed progress information"
    )]
    verbose: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    if args.input_stores.is_empty() {
        println!("Error: No input stores specified");
        return Ok(());
    }

    println!("Zarr Multi-Stream Store Merger");
    println!("===============================");
    println!();

    // Check if all input stores exist
    let mut missing_stores = Vec::new();
    for store_path in &args.input_stores {
        if !store_path.exists() {
            missing_stores.push(store_path.to_string_lossy().to_string());
        }
    }

    if !missing_stores.is_empty() {
        println!("Error: The following input stores were not found:");
        for store in missing_stores {
            println!("\t{}", store);
        }
        println!();
        println!(
            "Make sure to run 'cargo run --example multi_recorder' first to generate test stores."
        );
        return Ok(());
    }

    if args.verbose {
        println!("Configuration:");
        println!("\tOutput store:\t{}", args.output.display());
        println!("\tInput stores:\t{}", args.input_stores.len());
        for (i, store_path) in args.input_stores.iter().enumerate() {
            println!("\t\t[{}] {}", i + 1, store_path.display());
        }
        println!();
    }

    // Create output store
    std::fs::create_dir_all(&args.output)?;
    let output_store = Arc::new(FilesystemStore::new(&args.output)?);

    // Initialize output store structure
    println!("Creating output store structure...");
    let root_group = GroupBuilder::new().build(output_store.clone(), "/")?;
    root_group.store_metadata()?;

    let streams_group = GroupBuilder::new().build(output_store.clone(), "/streams")?;
    streams_group.store_metadata()?;

    let sync_group = GroupBuilder::new().build(output_store.clone(), "/sync")?;
    sync_group.store_metadata()?;

    let meta_group = GroupBuilder::new().build(output_store.clone(), "/meta")?;
    meta_group.store_metadata()?;

    // Merge metadata from all input stores
    let mut merged_meta = serde_json::Map::new();
    let mut source_files = Vec::new();

    for store_path in &args.input_stores {
        source_files.push(store_path.to_string_lossy().to_string());

        // Read and merge /meta/.zattrs
        let input_store = Arc::new(FilesystemStore::new(store_path)?);
        if let Ok(meta) = read_attributes(&input_store, "/meta") {
            if let Some(obj) = meta.as_object() {
                for (key, value) in obj {
                    if !merged_meta.contains_key(key) {
                        merged_meta.insert(key.clone(), value.clone());
                    }
                }
            }
        }
    }

    // Add merge provenance
    merged_meta.insert("merged_from".to_string(), json!(source_files));
    merged_meta.insert(
        "merged_at".to_string(),
        json!(chrono::Utc::now().to_rfc3339()),
    );

    // Write merged metadata
    write_attributes(&output_store, "/meta", &merged_meta)?;

    // Process each input store and copy streams
    for (store_idx, store_path) in args.input_stores.iter().enumerate() {
        println!(
            "Processing store [{}/{}]: {}",
            store_idx + 1,
            args.input_stores.len(),
            store_path.display()
        );

        let input_store = Arc::new(FilesystemStore::new(store_path)?);

        // Find all streams in /streams/
        let streams_dir = store_path.join("streams");
        if !streams_dir.exists() || !streams_dir.is_dir() {
            println!("\tWarning: No streams found in this store");
            continue;
        }

        for entry in std::fs::read_dir(streams_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }

            let stream_name = entry.file_name().to_string_lossy().to_string();
            println!("\tCopying stream: {}", stream_name);

            copy_stream(&input_store, &output_store, &stream_name, args.verbose)?;
        }
    }

    println!();
    println!("SUCCESS! Merge operation completed");
    println!("Output store:\t{}", args.output.display());
    println!();
    println!("Next steps:");
    println!("\tInspect:\tlsl-inspect {}", args.output.display());

    Ok(())
}

/// Copy a stream from input store to output store
fn copy_stream(
    input_store: &Arc<FilesystemStore>,
    output_store: &Arc<FilesystemStore>,
    stream_name: &str,
    verbose: bool,
) -> Result<()> {
    let input_stream_path = format!("/streams/{}", stream_name);
    let output_stream_path = format!("/streams/{}", stream_name);

    // Create stream group in output
    let stream_group = GroupBuilder::new().build(output_store.clone(), &output_stream_path)?;
    stream_group.store_metadata()?;

    // Copy data array
    let input_data_path = format!("{}/data", input_stream_path);
    let output_data_path = format!("{}/data", output_stream_path);

    if let Ok(input_array) = Array::<FilesystemStore>::open(input_store.clone(), &input_data_path)
    {
        let shape = input_array.shape();
        if verbose {
            println!("\t\tCopying data array: shape {:?}", shape);
        }

        // Read all data from input
        let data_subset = ArraySubset::new_with_ranges(&[0..shape[0], 0..shape[1]]);
        let data = input_array.retrieve_array_subset_ndarray::<f64>(&data_subset)?;

        // Create output array with same metadata
        let output_array = input_array.builder().build(output_store.clone(), &output_data_path)?;
        output_array.store_metadata()?;

        // Write data
        output_array.store_array_subset_ndarray::<f64, IxDyn>(&[0, 0], data)?;

        // Copy attributes
        if let Ok(attrs) = read_attributes(input_store, &input_data_path) {
            if let Some(obj) = attrs.as_object() {
                write_attributes(output_store, &output_data_path, obj)?;
            }
        }
    }

    // Copy time array
    let input_time_path = format!("{}/time", input_stream_path);
    let output_time_path = format!("{}/time", output_stream_path);

    if let Ok(input_array) = Array::<FilesystemStore>::open(input_store.clone(), &input_time_path)
    {
        let shape = input_array.shape();
        if verbose {
            println!("\t\tCopying time array: shape {:?}", shape);
        }

        // Read all data from input
        let time_subset = ArraySubset::new_with_ranges(&[0..shape[0]]);
        let data = input_array.retrieve_array_subset_ndarray::<f64>(&time_subset)?;

        // Create output array
        let output_array = input_array.builder().build(output_store.clone(), &output_time_path)?;
        output_array.store_metadata()?;

        // Write data
        output_array.store_array_subset_ndarray::<f64, IxDyn>(&[0], data)?;

        // Copy attributes
        if let Ok(attrs) = read_attributes(input_store, &input_time_path) {
            if let Some(obj) = attrs.as_object() {
                write_attributes(output_store, &output_time_path, obj)?;
            }
        }
    }

    Ok(())
}

/// Read attributes from a path's .zattrs file
fn read_attributes(store: &Arc<FilesystemStore>, path: &str) -> Result<Value> {
    let trimmed_path = path.trim_end_matches('/');
    let attrs_path = if trimmed_path.is_empty() || trimmed_path == "/" {
        ".zattrs".to_string()  // Root group
    } else {
        format!("{}/.zattrs", trimmed_path.trim_start_matches('/'))
    };
    let attrs_key = StoreKey::new(&attrs_path)?;
    let attrs_bytes = store
        .get(&attrs_key)?
        .ok_or_else(|| anyhow::anyhow!("Attributes not found at {}", attrs_path))?;
    let attrs: Value = serde_json::from_slice(&attrs_bytes)?;
    Ok(attrs)
}

/// Write attributes to a path's .zattrs file
fn write_attributes(
    store: &Arc<FilesystemStore>,
    path: &str,
    attrs: &serde_json::Map<String, serde_json::Value>,
) -> Result<()> {
    let trimmed_path = path.trim_end_matches('/');
    let attrs_path = if trimmed_path.is_empty() || trimmed_path == "/" {
        ".zattrs".to_string()  // Root group
    } else {
        format!("{}/.zattrs", trimmed_path.trim_start_matches('/'))
    };
    let attrs_key = StoreKey::new(&attrs_path)?;
    let attrs_json = serde_json::to_vec(&attrs)?;
    store.set(&attrs_key, attrs_json.into())?;
    Ok(())
}

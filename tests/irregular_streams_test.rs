use anyhow::Result;
use std::time::Duration;
use std::thread;

/// Test helper to clean up test zarr files
fn cleanup_test_file(path: &str) {
    let _ = std::fs::remove_dir_all(path);
}

#[test]
#[ignore] // Ignore by default as it requires LSL streams to be running
fn test_irregular_numeric_stream_recording() -> Result<()> {
    // This test verifies that the recorder can handle irregular numeric streams
    // (streams with nominal_srate = 0)

    let test_output = "test_irregular_numeric.zarr";
    cleanup_test_file(test_output);

    // Note: This test requires a separate LSL stream generator running
    // Run: lsl-dummy-stream --name "IrregularTest" --source-id "irregular_numeric" --sample-rate 0

    println!("Test requires LSL stream with source_id='irregular_numeric' and nominal_srate=0");
    println!("Start stream with: lsl-dummy-stream --name IrregularTest --source-id irregular_numeric --sample-rate 0");

    // Sleep to allow manual stream setup
    thread::sleep(Duration::from_secs(2));

    // TODO: Add actual recording test once stream is available
    // For now, this is a placeholder for the test structure

    cleanup_test_file(test_output);
    Ok(())
}

#[test]
#[ignore] // Ignore by default as it requires LSL streams to be running
fn test_string_event_marker_stream() -> Result<()> {
    // This test verifies that the recorder can handle string-based event marker streams

    let test_output = "test_string_markers.zarr";
    cleanup_test_file(test_output);

    // Note: This test requires a separate LSL stream generator for string markers
    // String format streams typically use channel_format = String

    println!("Test requires LSL string stream with source_id='string_markers'");

    // Sleep to allow manual stream setup
    thread::sleep(Duration::from_secs(2));

    // TODO: Add actual recording test once stream is available
    // Should verify:
    // 1. String data is written correctly to Zarr
    // 2. Metadata is preserved
    // 3. Description field is included

    cleanup_test_file(test_output);
    Ok(())
}

#[test]
#[ignore] // Ignore by default
fn test_lsl_description_in_metadata() -> Result<()> {
    // This test verifies that LSL stream descriptions are saved to metadata

    let test_output = "test_description_metadata.zarr";
    cleanup_test_file(test_output);

    // TODO: Record a stream and verify that the metadata includes "description" field
    // Use zarr::open to read the metadata and check for description

    cleanup_test_file(test_output);
    Ok(())
}

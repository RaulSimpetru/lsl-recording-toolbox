//! LSL Recording Toolbox - A comprehensive toolkit for recording Lab Streaming Layer (LSL) data
//!
//! This crate provides a collection of command-line tools and library functions for recording,
//! synchronizing, inspecting, and validating LSL data streams in Zarr format.
//!
//! # Overview
//!
//! The LSL Recording Toolbox is designed for neuroscience and psychophysiology researchers who need
//! to record multiple synchronized data streams (EEG, EMG, eye-tracking, markers, etc.) with
//! precise timing and efficient storage.
//!
//! # Key Features
//!
//! - **Multi-stream recording** with millisecond-level synchronization
//! - **Zarr format** for efficient hierarchical storage and analysis
//! - **Interactive control** via stdin commands (START/STOP/QUIT)
//! - **Post-processing synchronization** to align timestamps across streams
//! - **Validation tools** for timing accuracy and drift analysis
//! - **Inspection utilities** for metadata and structure visualization
//! - **Test stream generation** for development and testing
//!
//! # Command-Line Tools
//!
//! The toolkit includes six main binaries:
//!
//! - [`lsl-recorder`](../lsl_recorder/index.html) - Single-stream recorder with interactive control
//! - [`lsl-multi-recorder`](../lsl_multi_recorder/index.html) - Multi-stream unified controller
//! - [`lsl-sync`](../lsl_sync/index.html) - Post-processing timestamp synchronization
//! - [`lsl-inspect`](../lsl_inspect/index.html) - Zarr file inspection and visualization
//! - [`lsl-validate`](../lsl_validate/index.html) - Synchronization quality analyzer
//! - [`lsl-dummy-stream`](../lsl_dummy_stream/index.html) - Test stream generator
//!
//! # Quick Start
//!
//! ## Recording a Single Stream
//!
//! ```bash
//! # Start a test stream (in one terminal)
//! lsl-dummy-stream --name "TestEMG" --source-id "EMG_1234" --channels 8
//!
//! # Record it (in another terminal)
//! lsl-recorder --source-id "EMG_1234" --output experiment --subject P001
//! # Commands: START, STOP, QUIT
//! ```
//!
//! ## Recording Multiple Streams
//!
//! ```bash
//! # Start test streams
//! lsl-dummy-stream --name "EMG" --source-id "emg1" --channels 8 &
//! lsl-dummy-stream --name "EEG" --source-id "eeg1" --channels 64 --type EEG &
//!
//! # Record all streams with unified control
//! lsl-multi-recorder \
//!   --source-ids "emg1" "eeg1" \
//!   --stream-names "EMG" "EEG" \
//!   --output experiment \
//!   --subject P001 \
//!   --session-id session_001
//! # Commands: START, STOP, QUIT
//! ```
//!
//! ## Post-Processing Workflow
//!
//! ```bash
//! # 1. Synchronize timestamps across streams
//! lsl-sync experiment.zarr --mode common-start --trim-both
//!
//! # 2. Inspect the synchronized data
//! lsl-inspect experiment.zarr --verbose
//!
//! # 3. Validate synchronization quality
//! lsl-validate experiment.zarr
//! ```
//!
//! # Examples
//!
//! The repository includes complete workflow examples in the [`examples/`](https://github.com/RaulSimpetru/lsl-recording-toolbox/tree/master/examples) directory:
//!
//! **Unix/Linux/Mac** ([`examples/unix/`](https://github.com/RaulSimpetru/lsl-recording-toolbox/tree/master/examples/unix)):
//! - [`multi_recorder_demo.sh`](https://github.com/RaulSimpetru/lsl-recording-toolbox/blob/master/examples/unix/multi_recorder_demo.sh) - Multi-stream recording demonstration
//! - [`inspect_demo.sh`](https://github.com/RaulSimpetru/lsl-recording-toolbox/blob/master/examples/unix/inspect_demo.sh) - File inspection examples
//! - [`sync_demo.sh`](https://github.com/RaulSimpetru/lsl-recording-toolbox/blob/master/examples/unix/sync_demo.sh) - Timestamp synchronization workflow
//!
//! **Windows** ([`examples/windows/`](https://github.com/RaulSimpetru/lsl-recording-toolbox/tree/master/examples/windows)):
//! - [`multi_recorder_demo.bat`](https://github.com/RaulSimpetru/lsl-recording-toolbox/blob/master/examples/windows/multi_recorder_demo.bat) - Multi-stream recording demonstration
//! - [`inspect_demo.bat`](https://github.com/RaulSimpetru/lsl-recording-toolbox/blob/master/examples/windows/inspect_demo.bat) - File inspection examples
//! - [`sync_demo.bat`](https://github.com/RaulSimpetru/lsl-recording-toolbox/blob/master/examples/windows/sync_demo.bat) - Timestamp synchronization workflow
//!
//! **Python** ([`examples/`](https://github.com/RaulSimpetru/lsl-recording-toolbox/tree/master/examples)):
//! - [`lsl-inspect.py`](https://github.com/RaulSimpetru/lsl-recording-toolbox/blob/master/examples/lsl-inspect.py) - Python-based Zarr file inspection
//!
//! To run the examples:
//! ```bash
//! # Unix/Linux/Mac
//! bash examples/unix/multi_recorder_demo.sh
//!
//! # Windows
//! examples\windows\multi_recorder_demo.bat
//! ```
//!
//! # Zarr File Structure
//!
//! Recordings are stored in Zarr v3 format with this hierarchy:
//!
//! ```text
//! experiment.zarr/
//! ├── streams/
//! │   ├── EMG/
//! │   │   ├── data           [N × C] float32 (samples × channels)
//! │   │   ├── time           [N] float64 (LSL timestamps)
//! │   │   ├── aligned_time   [N] float64 (synchronized, created by lsl-sync)
//! │   │   └── zarr.json      (metadata and attributes)
//! │   └── EEG/
//! │       ├── data
//! │       ├── time
//! │       ├── aligned_time
//! │       └── zarr.json
//! └── meta/
//!     ├── subject
//!     ├── session_id
//!     ├── start_time
//!     └── notes
//! ```
//!
//! # Library Usage
//!
//! While primarily a CLI toolkit, the library modules can be used programmatically:
//!
//! - [`zarr`] - Zarr file writing and metadata management
//! - [`lsl`] - LSL stream recording and configuration
//! - [`sync`] - Timestamp synchronization algorithms
//! - [`cli`] - Command-line argument definitions
//! - [`commands`] - Interactive command handling
//!
//! # License
//!
//! This project is licensed under the GNU General Public License v3.0.
//! See LICENSE.md for details.

pub mod zarr;
pub mod sync;
pub mod cli;
pub mod commands;
pub mod lsl;

use chrono::Datelike;

/// Display GPL license notice for a program
pub fn display_license_notice(program_name: &str) {
	let version = env!("CARGO_PKG_VERSION");
	let current_year = chrono::Utc::now().year();
	let copyright_year = if current_year == 2025 {
		"2025".to_string()
	} else {
		format!("2025-{}", current_year)
	};

	println!("{} {} Copyright (C) {} Raul C. Sîmpetru", program_name, version, copyright_year);
	println!("This program comes with ABSOLUTELY NO WARRANTY.");
	println!("For details see https://www.gnu.org/licenses/gpl-3.0.html#license-text.");
	println!("This is free software, and you are welcome to redistribute it under certain conditions.");
	println!();
}

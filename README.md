# LSL Recording Toolbox

A Rust toolkit for recording, managing, and analyzing Lab Streaming Layer (LSL) data streams in Zarr format.

## Overview

The LSL Recording Toolbox provides a suite of command-line tools for synchronized recording of multiple LSL data streams. Designed for research and real-time data acquisition, the toolkit supports hierarchical Zarr storage, metadata management, and multi-stream synchronization with millisecond precision.

## Features

- **Multi-Stream Recording**: Synchronized recording of multiple LSL streams with unified control
- **Zarr Format**: Array storage format optimized for scientific analysis
- **Interactive Control**: Real-time START/STOP/QUIT commands via stdin
- **Metadata Management**: Subject, session, and experiment metadata stored with recordings
- **Adaptive Buffering**: Automatic buffer sizing based on stream sample rates
- **Concurrent Writing**: Multiple recorders can write to the same Zarr file safely
- **Duration Analysis**: Automatic calculation and display of recording duration
- **File Merging**: Combine multiple Zarr files with configurable time alignment
- **Validation Tools**: Analyze synchronization quality and timing accuracy

## Installation

### Prerequisites

- Rust toolchain (1.70+)
- LSL library installed and accessible
- Zarr support via zarrs crate (automatically handled by Cargo)

### Build from Source

```bash
# Clone the repository
git clone <repository-url>
cd lsl-recording-toolbox

# Build all tools
cargo build --release

# Tools will be available in target/release/
```

### Environment Setup

Set the `PYLSL_LIB` environment variable to point to your LSL shared library:

```bash
# Linux
export PYLSL_LIB=/path/to/liblsl.so

# macOS
export PYLSL_LIB=/path/to/liblsl.dylib

# Windows
set PYLSL_LIB=C:\path\to\lsl.dll
```

## Quick Start

### Single Stream Recording

```bash
# Start a test stream (in one terminal)
./target/release/lsl-dummy-stream --name "TestEMG" --source-id "EMG_001" --channels 8

# Record the stream (in another terminal)
./target/release/lsl-recorder \
  --source-id "EMG_001" \
  --output my_experiment \
  --subject P001 \
  --duration 60
```

### Multi-Stream Recording

```bash
# Start multiple test streams
./target/release/lsl-dummy-stream --name "EMG" --source-id "EMG_001" --channels 8 &
./target/release/lsl-dummy-stream --name "EEG" --source-id "EEG_001" --channels 64 &

# Record all streams with synchronized control
./target/release/lsl-multi-recorder \
  --source-ids "EMG_001" "EEG_001" \
  --stream-names "EMG" "EEG" \
  --output experiment \
  --subject P001 \
  --session-id session_001

# In the interactive prompt:
# START    - Begin recording
# STOP     - Stop recording
# QUIT     - Exit
```

### Inspect and Analyze

```bash
# Inspect Zarr file structure and metadata
./target/release/lsl-inspect experiment_EMG.zarr

# Validate synchronization
./target/release/lsl-validate experiment_EMG.zarr
```

## Tools

### lsl-recorder

Main recording tool for capturing single LSL streams to Zarr.

**Features:**

- Interactive or direct recording modes
- Configurable flush intervals and buffer sizes
- Memory monitoring and adaptive buffer sizing
- Full metadata support (subject, session-id, notes)

**Usage:**

```bash
lsl-recorder --source-id <ID> --output <path> [OPTIONS]

Options:
  --interactive              Enable interactive mode (START/STOP/QUIT commands)
  --duration <seconds>       Auto-stop after specified duration
  --subject <id>            Subject identifier
  --session-id <id>         Session identifier
  --notes <text>            Recording notes
  --flush-interval <sec>    Flush interval (default: 1.0s)
  --quiet                   Minimal output mode
```

### lsl-multi-recorder

Unified controller for recording multiple LSL streams simultaneously.

**Features:**

- Synchronized START/STOP/QUIT across all streams
- Shared metadata propagation
- Process lifecycle management
- Professional tab-delimited output
- Millisecond-precision synchronization

**Usage:**

```bash
lsl-multi-recorder --source-ids <ID>... [OPTIONS]

Options:
  --source-ids <ID>...      Stream source IDs (space-separated, required)
  --stream-names <NAME>...  Custom stream names (optional)
  --output <path>           Base output path (default: "experiment")
  --subject <id>            Subject identifier (shared)
  --session-id <id>         Session identifier (shared)
  --notes <text>            Recording notes (shared)
  --quiet                   Minimal output for child recorders
```

### lsl-inspect

Inspect Zarr metadata, structure, and recording duration.

**Features:**

- Displays global metadata (subject, session, notes)
- Shows stream information (channels, sample rate, format)
- Calculates recording duration from timestamps
- Extracts and formats JSON attributes

**Usage:**

```bash
lsl-inspect <file.zarr>
```

**Example Output:**

```bash
Zarr Metadata Inspector
=======================
File: experiment_EMG.zarr

GLOBAL METADATA:
 subject: P001
 session_id: session_001
 start_time: 1759674580.727153

STREAMS:
 Stream: EMG
  Dataset 'data': shape [8, 9610]
  Dataset 'time': shape [9610]
  Recording duration: 9.610 seconds (9610 samples)
  Attribute 'stream_info_json' (JSON):
   source_id: "EMG_001"
   channel_count: 8
   nominal_srate: 1000.0
```

### lsl-validate

Analyze Zarr files for synchronization quality and timing accuracy.

**Usage:**

```bash
lsl-validate <file.zarr>
```

### lsl-dummy-stream

Generate dummy LSL streams with configurable sine wave data for testing.

**Usage:**

```bash
lsl-dummy-stream [OPTIONS]

Options:
  --name <name>             Stream name (default: "TestStream")
  --source-id <id>          Source ID (default: "TEST_1234")
  --type <type>             Stream type (default: "EMG")
  --channels <n>            Number of channels (default: 100)
  --sample-rate <hz>        Sample rate in Hz (default: 10000)
  --chunk-size <n>          Samples per chunk (default: 18)
```

## Zarr Store Structure

LSL Recorder creates hierarchical Zarr stores optimized for scientific analysis:

```bash
experiment.zarr/
├── .zgroup                    # Root group metadata
├── meta/
│   ├── .zgroup
│   └── .zattrs               # Global metadata (subject, session_id, start_time, notes)
├── streams/
│   ├── .zgroup
│   ├── EMG/
│   │   ├── .zgroup
│   │   ├── data/
│   │   │   ├── .zarray       # Array metadata [channels × samples]
│   │   │   ├── .zattrs       # stream_info, recorder_config (JSON)
│   │   │   └── c/            # Compressed chunks
│   │   │       ├── 0.0
│   │   │       ├── 0.1
│   │   │       └── ...
│   │   └── time/
│   │       ├── .zarray       # Array metadata [samples]
│   │       ├── .zattrs       # Timestamp metadata
│   │       └── c/            # Compressed chunks
│   │           ├── 0
│   │           ├── 1
│   │           └── ...
│   └── EEG/
│       └── ... (similar structure)
└── sync/
    ├── .zgroup
    └── .zattrs               # Synchronization metadata
```

**Key Features:**

- **Channels-first layout**: `data[channels, samples]` for efficient channel access
- **Float64 timestamps**: Microsecond-precision LSL timestamps
- **Chunked storage**: 100-sample chunks with Blosc/LZ4 compression
- **JSON metadata**: Complete stream and recorder configuration in .zattrs files
- **Cloud-optimized**: Directory-based format compatible with cloud storage
- **Concurrent writes**: Thread-safe Zarr access for multi-recorder scenarios

## Common Workflows

### Basic Recording Session

```bash
# 1. Start test streams
lsl-dummy-stream --name "EMG" --source-id "EMG_001" --channels 8 &
lsl-dummy-stream --name "EEG" --source-id "EEG_001" --channels 64 &

# 2. Record with multi-recorder
lsl-multi-recorder \
  --source-ids "EMG_001" "EEG_001" \
  --stream-names "EMG" "EEG" \
  --output session_001 \
  --subject P001 \
  --session-id session_001_baseline

# 3. Interactive control
START        # Begin recording
STOP         # Stop recording after desired duration
QUIT         # Exit

# 4. Inspect results (all streams in single zarr file)
lsl-inspect session_001.zarr

# 5. Validate synchronization
lsl-validate session_001.zarr
```

### Automated Recording with Duration

```bash
# Direct mode: auto-start, record for 60 seconds, auto-stop
lsl-recorder \
  --source-id "EMG_001" \
  --output experiment \
  --subject P001 \
  --duration 60
```

### Interactive Recording with Delayed Stop

```bash
# Start recording, then schedule auto-stop
lsl-recorder --interactive --source-id "EMG_001" --output experiment

# In the interactive prompt:
START              # Begin recording
STOP_AFTER 300     # Stop after 5 minutes
# ... wait ...
QUIT               # Exit when done
```

## Development

### Building

```bash
# Debug build
cargo build

# Release build (optimized)
cargo build --release

# Build specific binary
cargo build --bin lsl-multi-recorder

# Check code without building
cargo check
```

### Running Examples

```bash
# Multi-recorder demo (spawns streams, records, cleans up)
cargo run --example multi_recorder

# Other examples
cargo run --example basic_recording_demo
cargo run --example merge_workflow_demo
cargo run --example validation_demo
cargo run --example inspection_demo
```

### Code Quality

```bash
# Format code
cargo fmt

# Run linter
cargo clippy

# Run tests
cargo test
```

## Documentation

For detailed development documentation, architecture information, and contributor guidelines, see [CLAUDE.md](CLAUDE.md).

## Project Structure

```bash
lsl-recorder/
├── src/
│   ├── main.rs              # Main lsl-recorder binary
│   ├── cli.rs               # CLI argument definitions
│   ├── commands.rs          # Interactive command handler
│   ├── lsl.rs               # LSL stream recording logic
│   ├── zarr/                # Zarr writing and management
│   ├── sync.rs              # Synchronization coordination
│   └── bin/                 # Additional toolkit binaries
│       ├── lsl-multi-recorder.rs
│       ├── lsl-sync.rs
│       ├── lsl-validate.rs
│       ├── lsl-inspect.rs
│       └── lsl-dummy-stream.rs
├── examples/                # Example workflows
├── CLAUDE.md               # Detailed documentation
└── README.md               # This file
```

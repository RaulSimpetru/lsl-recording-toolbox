# LSL Recorder

A professional Rust toolkit for recording, managing, and analyzing Lab Streaming Layer (LSL) data streams in HDF5 format.

## Overview

LSL Recorder provides a comprehensive suite of command-line tools for high-performance, synchronized recording of multiple LSL data streams. Designed for scientific research and real-time data acquisition, the toolkit supports hierarchical HDF5 storage, metadata management, and multi-stream synchronization with millisecond precision.

## Features

- **Multi-Stream Recording**: Synchronized recording of multiple LSL streams with unified control
- **HDF5 Format**: Industry-standard hierarchical data format optimized for scientific analysis
- **Interactive Control**: Real-time START/STOP/QUIT commands via stdin
- **Metadata Management**: Subject, session, and experiment metadata stored with recordings
- **Adaptive Buffering**: Automatic buffer sizing based on stream sample rates
- **Concurrent Writing**: Multiple recorders can write to the same HDF5 file safely
- **Duration Analysis**: Automatic calculation and display of recording duration
- **File Merging**: Combine multiple HDF5 files with configurable time alignment
- **Validation Tools**: Analyze synchronization quality and timing accuracy
- **Professional Output**: Tab-delimited, emoji-free formatting suitable for scientific workflows

## Installation

### Prerequisites

- Rust toolchain (1.70+)
- LSL library installed and accessible
- HDF5 library (automatically handled by Cargo)

### Build from Source

```bash
# Clone the repository
git clone <repository-url>
cd lsl-recorder

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
# Inspect HDF5 file structure and metadata
./target/release/lsl-inspect experiment_EMG.h5

# Merge multiple files
./target/release/lsl-merge experiment_EMG.h5 experiment_EEG.h5 -o merged.h5

# Validate synchronization
./target/release/lsl-validate merged.h5
```

## Tools

### lsl-recorder

Main recording tool for capturing single LSL streams to HDF5.

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

Inspect HDF5 metadata, structure, and recording duration.

**Features:**

- Displays global metadata (subject, session, notes)
- Shows stream information (channels, sample rate, format)
- Calculates recording duration from timestamps
- Extracts and formats JSON attributes

**Usage:**

```bash
lsl-inspect <file.h5>
```

**Example Output:**

```bash
HDF5 Metadata Inspector
=======================
File: experiment_EMG.h5

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

### lsl-merge

Merge multiple HDF5 files into a single synchronized file.

**Features:**

- Multiple time alignment strategies
- Metadata conflict resolution
- Provenance tracking
- Optional trimming to common time ranges

**Usage:**

```bash
lsl-merge <file1.h5> <file2.h5> ... -o <output.h5> [OPTIONS]

Options:
  --time-ref <strategy>     Time alignment: common-start, first-stream,
                           last-stream, absolute-zero, keep-original
  --conflict <strategy>     Metadata conflicts: merge, use-first,
                           use-last, error
  --trim-start              Trim before common start time
  --trim-end                Trim after common end time
  --verbose                 Detailed progress information
```

### lsl-validate

Analyze HDF5 files for synchronization quality and timing accuracy.

**Usage:**

```bash
lsl-validate <file.h5>
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

## HDF5 File Structure

LSL Recorder creates hierarchical HDF5 files optimized for scientific analysis:

```bash
experiment.h5
├── /meta
│   ├── subject            (string)
│   ├── session_id         (string)
│   ├── start_time         (float64, LSL timestamp)
│   ├── notes              (string)
│   └── global_reference   (string)
│
├── /streams
│   ├── /EMG
│   │   ├── data           [channels × samples] float32
│   │   ├── time           [samples] float64 (LSL timestamps)
│   │   └── attributes:
│   │       ├── stream_info_json    (source_id, channel_count, etc.)
│   │       └── recorder_config_json (flush settings, version, etc.)
│   │
│   └── /EEG
│       ├── data           [channels × samples]
│       ├── time           [samples]
│       └── attributes...
│
└── /sync
    └── attributes:
        └── synchronization metadata
```

**Key Features:**

- **Channels-first layout**: `data[channels, samples]` for efficient channel access
- **Float64 timestamps**: Microsecond-precision LSL timestamps
- **Chunked storage**: 1000-sample chunks optimized for sequential access
- **JSON metadata**: Complete stream and recorder configuration in attributes
- **Concurrent writes**: Thread-safe HDF5 access for multi-recorder scenarios

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

# 4. Inspect results
lsl-inspect session_001_EMG.h5
lsl-inspect session_001_EEG.h5

# 5. Merge if needed
lsl-merge session_001_EMG.h5 session_001_EEG.h5 -o session_001_merged.h5

# 6. Validate synchronization
lsl-validate session_001_merged.h5
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
│   ├── hdf5/                # HDF5 writing and management
│   ├── merger.rs            # HDF5 file merging
│   ├── sync.rs              # Synchronization coordination
│   └── bin/                 # Additional toolkit binaries
│       ├── lsl-multi-recorder.rs
│       ├── lsl-merge.rs
│       ├── lsl-validate.rs
│       ├── lsl-inspect.rs
│       └── lsl-dummy-stream.rs
├── examples/                # Example workflows
├── CLAUDE.md               # Detailed documentation
└── README.md               # This file
```

## License

[Add license information here]

## Contributing

[Add contribution guidelines here]

## Support

For issues, questions, or contributions, please [add contact/repository information].

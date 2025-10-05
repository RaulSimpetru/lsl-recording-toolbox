# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

LSL Recorder is a Rust command-line tool for recording Lab Streaming Layer (LSL) data streams to disk in HDF5 format. The project provides a dedicated control interface for managing recordings, with HDF5 support for hierarchical multi-stream experiments.

## LSL Toolkit Binaries

The project includes multiple binary tools that work together:

- **`lsl-recorder`** - Main recording tool for capturing LSL streams to HDF5
- **`lsl-multi-recorder`** - Unified controller for recording multiple LSL streams simultaneously with synchronized start/stop
- **`lsl-merge`** - Merge multiple HDF5 files into a single synchronized file
- **`lsl-validate`** - Analyze HDF5 files for synchronization quality and timing accuracy
- **`lsl-inspect`** - Inspect HDF5 metadata, structure, and recording duration
- **`lsl-dummy-stream`** - Generate dummy LSL streams with sine wave data for testing

### Usage Examples

```bash
# Build all tools
cargo build

# Generate test stream (run in separate terminal)
lsl-dummy-stream --name "TestEMG" --source-id "1234" --channels 8

# Record data
lsl-recorder --source-id "1234" --hdf5-file experiment_EMG.h5 --subject P001

# Generate another stream for multi-stream testing
lsl-dummy-stream --name "TestEEG" --source-id "5678" --type "EEG" --channels 64 --sample-rate 1000

# Multi-stream recording with unified control (recommended)
lsl-multi-recorder --source-ids "1234" "5678" \
  --stream-names "EMG" "EEG" \
  --output experiment \
  --subject P001 \
  --session-id session_001 \
  --notes "Multi-stream experiment"

# Merge multiple files
lsl-merge experiment_EMG.h5 experiment_EEG.h5 -o merged_experiment.h5

# Validate synchronization
lsl-validate merged_experiment.h5

# Inspect file structure and recording duration
lsl-inspect merged_experiment.h5
```

### Tool Descriptions

**lsl-recorder:**
- Single-stream recording with interactive or direct mode
- HDF5 output with hierarchical stream organization
- Configurable flush intervals and buffer sizes
- Memory monitoring and adaptive buffer sizing
- Supports metadata (subject, session-id, notes)

**lsl-multi-recorder:**
- Records multiple LSL streams with unified command broadcasting
- Synchronized START/STOP/QUIT across all child recorders
- Shares metadata across all recordings
- Each stream writes to separate HDF5 file with consistent naming
- Professional output with tab-delimited formatting
- Cross-platform support (Windows/Linux)

**lsl-inspect:**
- Displays HDF5 file structure and metadata
- Shows stream information (channels, sample rate, format)
- Calculates and displays recording duration in seconds
- Shows sample counts and dataset shapes
- Extracts JSON metadata from attributes

**lsl-merge:**
- Merges multiple HDF5 files into a single synchronized file
- Preserves all stream data and metadata
- Validates synchronization across streams

**lsl-validate:**
- Analyzes synchronization quality
- Reports timing accuracy and drift
- Validates LSL timestamp consistency

**lsl-dummy-stream:**
- Generates configurable sine wave test streams
- Supports various channel counts and sample rates
- Useful for testing and development
```

## Development Commands

### Build and Run

```bash
# Build the project
cargo build

# Run in direct recording mode (auto-starts recording)
cargo run -- --source-id "1234" --output experiment --duration 60

# Run in interactive mode (controlled via stdin commands)
cargo run -- --interactive --source-id "EMG_stream" --output experiment --subject P001

# HDF5 with full metadata
cargo run -- --interactive \
  --source-id "EEG_stream" \
  --output experiment \
  --subject P001 \
  --session-id session_001 \
  --notes "Multi-stream recording experiment"

# Multi-stream recording with lsl-multi-recorder
cargo run --bin lsl-multi-recorder -- \
  --source-ids "EMG_1234" "EEG_5678" \
  --stream-names "EMG" "EEG" \
  --output experiment \
  --subject P001

# Build for release
cargo build --release
```

### Interactive Control Commands

When running with `--interactive` flag, the recorder accepts these stdin commands:

- `START` - Begin recording
- `STOP` - Stop recording
- `STOP_AFTER <seconds>` - Stop recording after specified duration
- `QUIT` - Exit the program

### Multi-Stream Recording

**Using lsl-multi-recorder (recommended for production):**

The `lsl-multi-recorder` tool provides unified control over multiple LSL stream recordings with synchronized start/stop commands.

```bash
# Build the multi-recorder
cargo build --bin lsl-multi-recorder

# Interactive multi-stream recording
lsl-multi-recorder \
  --source-ids "EMG_1234" "EEG_5678" "Markers_9999" \
  --stream-names "EMG" "EEG" "Events" \
  --output experiment \
  --subject P001 \
  --session-id session_001 \
  --notes "Multi-modal recording session"

# Then use interactive commands:
# START - begin recording all streams
# STOP - stop recording all streams
# STOP_AFTER 60 - stop all streams after 60 seconds
# QUIT - terminate all recorders
```

**Multi-recorder features:**

- Broadcasts commands to all spawned recorders simultaneously
- Ensures synchronized start/stop across all streams (millisecond precision)
- Shares metadata (subject, session, notes) across all recordings
- Each stream writes to separate HDF5 file with consistent naming (e.g., `experiment_EMG.h5`, `experiment_EEG.h5`)
- Captures and displays labeled output from all child processes
- Professional output formatting with tab-delimited structure
- Handles process lifecycle management and clean shutdown

**Example Demo:**

The `multi_recorder` example demonstrates how to use `lsl-multi-recorder` in a complete workflow:

```bash
# Build and run the example (spawns dummy streams, records them, and cleans up)
cargo build
cargo run --example multi_recorder
```

This example shows:
- Spawning dummy LSL streams for testing
- Using lsl-multi-recorder to control both streams
- Sending synchronized START/STOP/QUIT commands
- Clean shutdown and output verification

### Development Environment

```bash
# Check code (fast compilation check)
cargo check

# Format code
cargo fmt

# Run linter
cargo clippy
```

## HDF5 File Structure

When using `--hdf5-file`, the recorder creates a hierarchical structure optimized for scientific data analysis:

```markdown
experiment.h5
│
├── /streams
│   ├── /EMG
│   │   ├── data           [N x C] float32   (samples × channels)
│   │   ├── time           [N] float64       (LSL time stamps)
│   │   └── info           (attrs: sampling_rate, units, channel_labels, source_id, etc.)
│   │
│   ├── /EEG
│   │   ├── data           [N x C]
│   │   ├── time           [N]
│   │   └── info
│   │
│   ├── /Markers
│   │   ├── events         [N] string        (marker labels)
│   │   ├── time           [N]
│   │   └── info
│   │
│   └── … (any other stream, e.g. Gaze, IMU, Video_timestamps)
│
├── /sync
│   ├── clock_offsets      [M] float64   (per-stream corrections logged during acquisition)
│   └── global_reference   string        (e.g. "LSL clock of recorder host")
│
└── /meta
    ├── subject            string
    ├── session_id         string
    ├── start_time         ISO-8601
    └── notes              string
```

**Key Features:**

- **Concurrent Writing**: Multiple recorder processes can write to the same HDF5 file
- **Chunked Storage**: Optimized for time-series data with 1000-sample chunks
- **Metadata**: Stream information stored as HDF5 attributes for easy access
- **Time Synchronization**: LSL timestamps preserved in float64 precision
- **Scalable**: Add any number of streams to the same experiment file
- **Scientific Format**: Industry-standard HDF5 format for scientific data analysis

**HDF5 CLI Arguments (lsl-recorder):**

- `-o, --output <path>`: HDF5 experiment base file path (without extension, default: "experiment")
- `--stream-name <name>`: Group name for this stream (defaults to source-id if not specified)
- `--suffix <suffix>`: Optional suffix for HDF5 file (defaults to stream name)
- `--subject <id>`: Subject identifier for metadata
- `--session-id <id>`: Session identifier for metadata
- `--notes <text>`: Additional notes for metadata
- `--flush-interval <seconds>`: Flush data to disk interval (default: 1.0s)
- `--flush-buffer-size <samples>`: Buffer size before forcing flush (default: 50)
- `--immediate-flush`: Flush immediately after every sample (maximum safety, lower performance)

**Multi-Recorder CLI Arguments (lsl-multi-recorder):**

- `--source-ids <ID>...`: LSL stream source IDs to record (space-separated, required)
- `--stream-names <NAME>...`: Custom stream names (must match source-ids count if provided)
- `-o, --output <path>`: HDF5 experiment base file path (default: "experiment")
- `--subject <id>`: Subject identifier shared across all recordings
- `--session-id <id>`: Session identifier shared across all recordings
- `--notes <text>`: Notes shared across all recordings
- `--resolve-timeout <seconds>`: Timeout for stream resolution (default: 5.0)
- `--flush-interval <seconds>`: Flush interval for all recorders (default: 1.0)
- `-q, --quiet`: Minimal output mode for child recorders

## Architecture

**Project Structure:**

- `src/main.rs` - Main lsl-recorder binary with async/await patterns
- `src/bin/` - Additional toolkit binaries:
  - `lsl-multi-recorder.rs` - Multi-stream recording controller
  - `lsl-merge.rs` - HDF5 file merger
  - `lsl-validate.rs` - Synchronization validator
  - `lsl-inspect.rs` - HDF5 metadata inspector
  - `lsl-dummy-stream.rs` - Test stream generator
- `src/cli.rs` - Command-line argument definitions
- `src/commands.rs` - Interactive command handler
- `src/hdf5/` - HDF5 file writing and management
- `src/lsl.rs` - LSL stream recording logic
- `src/merger.rs` - HDF5 file merging logic
- `src/sync.rs` - Synchronization coordination
- `examples/` - Example workflows and demos

**Core Dependencies:**

- `lsl`: Lab Streaming Layer interface for real-time data streaming
- `tokio`: Async runtime for concurrent operations
- `clap`: Command-line argument parsing with derive macros
- `tracing`: Structured logging and diagnostics
- `hdf5`: HDF5 file format support
- `serde/serde_json`: Configuration and metadata serialization
- `anyhow`: Error handling

**Key Components:**

- **CLI Interface**: Uses clap derive macros with support for both direct recording and interactive control modes
- **Recording State Management**: Thread-safe atomic booleans for controlling recording and quit states
- **Command Handler**: `handle_commands()` processes START/STOP/STOP_AFTER/QUIT commands from stdin
- **LSL Recording**: `record_lsl_stream()` function handles controllable stream recording with proper threading
- **Stream Processing**: Supports various LSL post-processing options (ClockSync, Dejitter, Threadsafe)
- **HDF5 Writer**: Buffered writing with configurable flush intervals and concurrent access support
- **Multi-Recorder**: Process spawning and command broadcasting for synchronized multi-stream recording

## Environment Configuration

The project requires specific environment variables for LSL library integration:

- `PYLSL_LIB`: Path to LSL shared library (`/home/linuxbrew/.linuxbrew/opt/lsl/lib/liblsl.so`)

These are pre-configured in `.vscode/settings.json` for the development environment.

## Data Flow

### Single Stream Recording (lsl-recorder)

1. **Stream Resolution**: Resolves LSL streams by source ID with configurable timeout and retry logic
2. **Inlet Creation**: Creates StreamInlet with adaptive buffer sizing based on sample rate
3. **Data Processing**: Applies post-processing filters and reads data in blocking loops
4. **HDF5 Writing**:
   - Hierarchical structure with `/streams/<stream_name>/` organization
   - Buffered writing with configurable flush intervals
   - Automatic flushing based on buffer size thresholds
   - Concurrent access support for multi-recorder scenarios
   - Metadata stored as HDF5 attributes (stream info, recorder config)

### Multi-Stream Recording (lsl-multi-recorder)

1. **Process Spawning**: Spawns individual `lsl-recorder` processes for each source ID
2. **Command Broadcasting**: Broadcasts START/STOP/QUIT commands to all child processes via stdin pipes
3. **Output Routing**: Captures and labels stdout/stderr from each recorder
4. **Synchronized Control**: Ensures millisecond-level synchronization of start/stop events
5. **File Output**: Each stream writes to separate HDF5 file with consistent naming (`<output>_<stream_name>.h5`)

### Inspection and Analysis

**lsl-inspect:**
- Opens HDF5 file and reads `/meta`, `/streams`, and `/sync` groups
- Calculates recording duration from timestamp datasets: `duration = last_time - first_time`
- Displays metadata, dataset shapes, and stream information

**lsl-merge:**
- Reads multiple HDF5 files and combines streams into single file
- Preserves all metadata and timestamps
- Validates synchronization across merged streams

**lsl-validate:**
- Analyzes timestamp consistency and synchronization quality
- Reports timing drift and accuracy metrics
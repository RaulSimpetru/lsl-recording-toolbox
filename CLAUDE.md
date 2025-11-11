# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

LSL Recorder is a Rust command-line tool for recording Lab Streaming Layer (LSL) data streams to disk in Zarr format. The project provides a dedicated control interface for managing recordings, with Zarr support for hierarchical multi-stream experiments.

## LSL Toolkit Binaries

The project includes multiple binary tools that work together:

- **`lsl-recorder`** - Main recording tool for capturing LSL streams to Zarr
- **`lsl-multi-recorder`** - Unified controller for recording multiple LSL streams simultaneously with synchronized start/stop
- **`lsl-sync`** - Post-processing tool to align timestamps across multiple streams in a Zarr file
- **`lsl-merge`** - Merge multiple Zarr files into a single synchronized file
- **`lsl-validate`** - Analyze Zarr files for synchronization quality and timing accuracy
- **`lsl-inspect`** - Inspect Zarr metadata, structure, and recording duration
- **`lsl-dummy-stream`** - Generate dummy LSL streams with sine wave data for testing

### Usage Examples

```bash
# Build all tools
cargo build

# Generate test stream (run in separate terminal)
lsl-dummy-stream --name "TestEMG" --source-id "1234" --channels 8

# Record single stream
lsl-recorder --source-id "1234" --stream-name "EMG" --output experiment --subject P001

# Generate another stream for multi-stream testing
lsl-dummy-stream --name "TestEEG" --source-id "5678" --type "EEG" --channels 64 --sample-rate 1000

# Multi-stream recording with unified control (recommended)
# All streams write to a single experiment.zarr file
lsl-multi-recorder --source-ids "1234" "5678" \
  --stream-names "EMG" "EEG" \
  --output experiment \
  --subject P001 \
  --session-id session_001 \
  --notes "Multi-stream experiment"

# Inspect the single Zarr file containing all streams
lsl-inspect experiment.zarr

# Inspect with verbose mode (shows additional metadata)
lsl-inspect experiment.zarr --verbose

# Filter to specific stream(s)
lsl-inspect experiment.zarr --stream EMG
lsl-inspect experiment.zarr --stream EMG --stream EEG

# Synchronize timestamps across streams (post-processing)
lsl-sync experiment.zarr --mode common-start

# Synchronize with trimming (remove data outside common window)
lsl-sync experiment.zarr --mode common-start --trim-both

# Inspect synchronized results
lsl-inspect experiment.zarr --verbose

# Validate synchronization quality across streams
lsl-validate experiment.zarr
```

### Tool Descriptions

**lsl-recorder:**
- Single-stream recording with interactive or direct mode
- Zarr output with hierarchical stream organization
- Configurable flush intervals and buffer sizes
- Memory monitoring and adaptive buffer sizing
- Supports metadata (subject, session-id, notes)

**lsl-multi-recorder:**
- Records multiple LSL streams with unified command broadcasting
- Synchronized START/STOP/QUIT across all child recorders
- Shares metadata across all recordings
- All streams write to a single shared Zarr file with hierarchical structure
- File locking ensures safe concurrent writes to different stream groups
- Professional output with tab-delimited formatting
- Cross-platform support (Windows/Linux)

**lsl-sync:**
- Post-processing tool for aligning timestamps across multiple streams
- Reads raw timestamps from `/streams/<name>/time` arrays
- Writes aligned timestamps to `/streams/<name>/aligned_time` arrays
- Multiple alignment modes:
  - `common-start`: Align all streams to latest start time (recommended)
  - `first-stream`: Align to earliest stream start
  - `last-stream`: Align to latest stream start
  - `absolute-zero`: Align to t=0
- Optional trimming to remove data outside common time window
- Preserves original raw timestamps (non-destructive)
- Stores alignment metadata in `/streams/<name>/.zattrs`

**lsl-inspect:**
- Displays Zarr file structure and metadata with improved formatting
- Shows all streams within a single Zarr file
- Displays key stream information (channels, sample rate, format, duration)
- Supports filtering by stream name with `--stream` flag
- Verbose mode (`--verbose`) shows additional details
- Clean hierarchical output with Unicode box drawing
- Default file: `experiment.zarr`

**lsl-merge:**
- Merges multiple Zarr files into a single synchronized file
- Useful for combining data from separate recording sessions
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

# Zarr with full metadata
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
- All streams write to a single shared Zarr file (e.g., `experiment.zarr` with `/streams/EMG/`, `/streams/EEG/`)
- File locking prevents race conditions during concurrent initialization
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

## Zarr File Structure

When using `--zarr-file`, the recorder creates a hierarchical structure optimized for scientific data analysis:

```markdown
experiment.zarr
│
├── /streams
│   ├── /EMG
│   │   ├── data           [N x C] float32   (samples × channels)
│   │   ├── time           [N] float64       (LSL time stamps)
│   │   ├── aligned_time   [N] float64       (synchronized timestamps, created by lsl-sync)
│   │   └── zarr.json      (Zarr v3 metadata with attributes: stream_info, recorder_config, alignment_offset, trim indices)
│   │
│   ├── /EEG
│   │   ├── data           [N x C]
│   │   ├── time           [N]
│   │   ├── aligned_time   [N]
│   │   └── zarr.json
│   │
│   ├── /Markers
│   │   ├── events         [N] string        (marker labels)
│   │   ├── time           [N]
│   │   ├── aligned_time   [N]
│   │   └── zarr.json
│   │
│   └── … (any other stream, e.g. Gaze, IMU, Video_timestamps)
│
└── /meta
    ├── subject            string
    ├── session_id         string
    ├── start_time         ISO-8601
    └── notes              string
```

**Key Features:**

- **Concurrent Writing**: Multiple recorder processes can write to the same Zarr file
- **Chunked Storage**: Optimized for time-series data with 1000-sample chunks
- **Metadata**: Stream information stored as Zarr attributes for easy access
- **Time Synchronization**: LSL timestamps preserved in float64 precision
- **Scalable**: Add any number of streams to the same experiment file
- **Scientific Format**: Industry-standard Zarr format for scientific data analysis

**Zarr CLI Arguments (lsl-recorder):**

- `-o, --output <path>`: Zarr file path (without extension, default: "experiment" → creates "experiment.zarr")
- `--stream-name <name>`: Group name for this stream under /streams/ (defaults to source-id if not specified)
- `--suffix <suffix>`: Deprecated - no longer used for file naming (kept for backward compatibility)
- `--subject <id>`: Subject identifier for metadata
- `--session-id <id>`: Session identifier for metadata
- `--notes <text>`: Additional notes for metadata
- `--flush-interval <seconds>`: Flush data to disk interval (default: 1.0s)
- `--flush-buffer-size <samples>`: Buffer size before forcing flush (default: 50)
- `--immediate-flush`: Flush immediately after every sample (maximum safety, lower performance)

**Multi-Recorder CLI Arguments (lsl-multi-recorder):**

- `--source-ids <ID>...`: LSL stream source IDs to record (space-separated, required)
- `--stream-names <NAME>...`: Custom stream names (must match source-ids count if provided)
- `-o, --output <path>`: Zarr file path (default: "experiment" → creates single "experiment.zarr" for all streams)
- `--subject <id>`: Subject identifier shared across all recordings
- `--session-id <id>`: Session identifier shared across all recordings
- `--notes <text>`: Notes shared across all recordings
- `--resolve-timeout <seconds>`: Timeout for stream resolution (default: 5.0)
- `--flush-interval <seconds>`: Flush interval for all recorders (default: 1.0)
- `-q, --quiet`: Minimal output mode for child recorders

**Sync CLI Arguments (lsl-sync):**

- `<zarr_file>`: Path to Zarr file to synchronize (default: "experiment.zarr")
- `--mode <mode>`: Alignment mode (default: "common-start")
  - Options: `common-start`, `first-stream`, `last-stream`, `absolute-zero`
- `--trim-start`: Trim data before common start time
- `--trim-end`: Trim data after common end time
- `--trim-both`: Shorthand for `--trim-start --trim-end`

## Architecture

**Project Structure:**

- `src/main.rs` - Main lsl-recorder binary with async/await patterns
- `src/bin/` - Additional toolkit binaries:
  - `lsl-multi-recorder.rs` - Multi-stream recording controller
  - `lsl-sync.rs` - Post-processing timestamp synchronization tool
  - `lsl-merge.rs` - Zarr file merger
  - `lsl-validate.rs` - Synchronization validator
  - `lsl-inspect.rs` - Zarr metadata inspector
  - `lsl-dummy-stream.rs` - Test stream generator
- `src/cli.rs` - Command-line argument definitions
- `src/commands.rs` - Interactive command handler
- `src/zarr/` - Zarr file writing and management
- `src/lsl.rs` - LSL stream recording logic
- `src/merger.rs` - Zarr file merging logic
- `src/sync.rs` - Synchronization coordination
- `examples/` - Example workflows and demos

**Core Dependencies:**

- `lsl`: Lab Streaming Layer interface for real-time data streaming
- `tokio`: Async runtime for concurrent operations
- `clap`: Command-line argument parsing with derive macros
- `tracing`: Structured logging and diagnostics
- `zarr`: Zarr file format support
- `serde/serde_json`: Configuration and metadata serialization
- `anyhow`: Error handling

**Key Components:**

- **CLI Interface**: Uses clap derive macros with support for both direct recording and interactive control modes
- **Recording State Management**: Thread-safe atomic booleans for controlling recording and quit states
- **Command Handler**: `handle_commands()` processes START/STOP/STOP_AFTER/QUIT commands from stdin
- **LSL Recording**: `record_lsl_stream()` function handles controllable stream recording with proper threading
- **Stream Processing**: Supports various LSL post-processing options (ClockSync, Dejitter, Threadsafe)
- **Zarr Writer**: Buffered writing with configurable flush intervals and concurrent access support
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
4. **Zarr Writing**:
   - Hierarchical structure with `/streams/<stream_name>/` organization
   - Buffered writing with configurable flush intervals
   - Automatic flushing based on buffer size thresholds
   - Concurrent access support for multi-recorder scenarios
   - Metadata stored as Zarr attributes (stream info, recorder config)

### Multi-Stream Recording (lsl-multi-recorder)

1. **Process Spawning**: Spawns individual `lsl-recorder` processes for each source ID
2. **Command Broadcasting**: Broadcasts START/STOP/QUIT commands to all child processes via stdin pipes
3. **Output Routing**: Captures and labels stdout/stderr from each recorder
4. **Synchronized Control**: Ensures millisecond-level synchronization of start/stop events
5. **File Output**: Each stream writes to separate Zarr file with consistent naming (`<output>_<stream_name>.zarr`)

### Inspection and Analysis

**lsl-inspect:**
- Opens Zarr file and reads `/meta` and `/streams` groups
- Calculates recording duration from timestamp datasets: `duration = last_time - first_time`
- Displays metadata, dataset shapes, and stream information

**lsl-merge:**
- Reads multiple Zarr files and combines streams into single file
- Preserves all metadata and timestamps
- Validates synchronization across merged streams

**lsl-validate:**
- Analyzes timestamp consistency and synchronization quality
- Reports timing drift and accuracy metrics

### Synchronization Workflow

**LSL synchronization happens in two phases:**

**Phase 1: Recording (Automatic)**
- LSL library automatically applies clock synchronization during recording via `inlet.time_correction()`
- Raw timestamps are saved to `/streams/<stream_name>/time`
- Stream metadata (sampling rate, channel info, etc.) is stored in `/streams/<stream_name>/zarr.json` (Zarr v3 format)

**Phase 2: Post-Processing (Optional)**
- Use `lsl-sync` tool to align timestamps across streams
- Reads raw timestamps from `/streams/<stream_name>/time`
- Calculates alignment offsets based on selected mode
- Writes aligned timestamps to `/streams/<stream_name>/aligned_time`
- Stores alignment metadata in `/streams/<stream_name>/zarr.json` attributes:
  - `alignment_offset`: Time offset applied to this stream
  - `trim_start_index`: Array index where trimming started (if trimming enabled)
  - `trim_end_index`: Array index where trimming ended (if trimming enabled)
  - `original_sample_count`: Number of samples before trimming
  - `aligned_sample_count`: Number of samples after trimming

**Alignment Modes:**
- `common-start` (recommended): Align to latest start time where ALL streams have data
- `first-stream`: Align to earliest stream start (may have gaps)
- `last-stream`: Align to latest stream start
- `absolute-zero`: Align to t=0

**Example Workflow:**

```bash
# Step 1: Record multiple streams
lsl-multi-recorder --source-ids "emg1" "eeg1" --stream-names "EMG" "EEG" --output experiment

# Step 2: Inspect raw recording
lsl-inspect experiment.zarr

# Step 3: Synchronize timestamps
lsl-sync experiment.zarr --mode common-start --trim-both

# Step 4: Inspect synchronized results
lsl-inspect experiment.zarr --verbose

# Step 5: Validate synchronization quality
lsl-validate experiment.zarr
```

**Key Points:**
- Original raw timestamps are preserved in `/streams/<stream>/time`
- Aligned timestamps are written to `/streams/<stream>/aligned_time`
- All metadata is stored in `/streams/<stream>/zarr.json` (Zarr v3 format)
- Synchronization is non-destructive
- Trimming removes data outside the common time window
- Use `--trim-both` for cleanest aligned datasets
- No `.zattrs` files - pure Zarr v3 format throughout
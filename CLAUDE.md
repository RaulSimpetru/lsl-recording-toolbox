# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

LSL Recorder is a Rust command-line tool for recording Lab Streaming Layer (LSL) data streams to disk in HDF5 format. The project provides a dedicated control interface for managing recordings, with HDF5 support for hierarchical multi-stream experiments.

## Development Commands

### Build and Run

```bash
# Build the project
cargo build

# Run in direct recording mode (auto-starts recording)
cargo run -- --source-id "1234" --hdf5-file experiment.h5 --duration 60

# Run in interactive mode (controlled via stdin commands)
cargo run -- --interactive --source-id "EMG_stream" --hdf5-file experiment.h5 --subject P001

# HDF5 with full metadata
cargo run -- --interactive \
  --source-id "EEG_stream" \
  --hdf5-file experiment.h5 \
  --subject P001 \
  --session-id session_001 \
  --notes "Multi-stream recording experiment"

# Build for release
cargo build --release
```

### Interactive Control Commands

When running with `--interactive` flag, the recorder accepts these stdin commands:

- `START` - Begin recording
- `STOP` - Stop recording
- `STOP_AFTER <seconds>` - Stop recording after specified duration
- `QUIT` - Exit the program

### Multi-Instance Usage

**Cross-platform approach using anonymous pipes (recommended):**
Each spawned process gets its own stdin pipe for independent control.

**Multi-stream approach with shared HDF5 file:**

```rust
// Multiple streams writing to same HDF5 file
let mut emg_recorder = Command::new("./lsl-recorder")
    .args([
        "--interactive", "--source-id", "EMG_stream",
        "--hdf5-file", "experiment.h5",
        "--subject", "P001", "--session-id", "session_001",
        "--notes", "Multi-stream recording demo", "--quiet"
    ])
    .stdin(Stdio::piped())
    .spawn()?;

let mut eeg_recorder = Command::new("./lsl-recorder")
    .args([
        "--interactive", "--source-id", "EEG_stream",
        "--hdf5-file", "experiment.h5",
        "--quiet"
    ])
    .stdin(Stdio::piped())
    .spawn()?;

// Both write to same file in different groups
writeln!(emg_recorder.stdin.as_mut().unwrap(), "START")?;
writeln!(eeg_recorder.stdin.as_mut().unwrap(), "START")?;
```

**Example usage:**

```bash
# Build and run the multi-recorder example
cargo build
cargo run --example multi_recorder
```

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

**HDF5 CLI Arguments:**

- `--hdf5-file <path>`: Enable HDF5 mode and specify file path
- `--stream-name <name>`: Group name for this stream (defaults to source-id if not specified)
- `--subject <id>`: Subject identifier for metadata
- `--session-id <id>`: Session identifier for metadata
- `--notes <text>`: Additional notes for metadata

## Architecture

**Single Binary Structure**: This is a monolithic Rust application with all logic in `src/main.rs`. The application uses async/await patterns with Tokio runtime.

**Core Dependencies**:

- `lsl`: Lab Streaming Layer interface for real-time data streaming
- `tokio`: Async runtime for concurrent operations
- `clap`: Command-line argument parsing with derive macros
- `tracing`: Structured logging and diagnostics

**Key Components**:

- **CLI Interface**: Uses clap derive macros with support for both direct recording and interactive control modes
- **Recording State Management**: Thread-safe atomic booleans for controlling recording and quit states
- **Command Handler**: `handle_commands()` processes START/STOP/STOP_AFTER/QUIT commands from stdin
- **LSL Recording**: `record_lsl_stream()` function handles controllable stream recording with proper threading
- **Stream Processing**: Supports various LSL post-processing options (ClockSync, Dejitter, Threadsafe)

## Environment Configuration

The project requires specific environment variables for LSL library integration:

- `PYLSL_LIB`: Path to LSL shared library (`/home/linuxbrew/.linuxbrew/opt/lsl/lib/liblsl.so`)

These are pre-configured in `.vscode/settings.json` for the development environment.

## Data Flow

1. **Stream Resolution**: Resolves LSL streams by source ID with configurable timeout
2. **Inlet Creation**: Creates StreamInlet with buffer management and chunk optimization
3. **Data Processing**: Applies post-processing filters and reads data in blocking loops
4. **HDF5 Writing**: Hierarchical structure with buffered writing, automatic flushing and concurrent access support

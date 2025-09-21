# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

LSL Recorder is a Rust command-line tool for recording Lab Streaming Layer (LSL) data streams to disk in XDF format. The project provides a dedicated control interface for managing recordings.

## Development Commands

### Build and Run
```bash
# Build the project
cargo build

# Run in direct recording mode (auto-starts recording)
cargo run -- --source-id "1234" --output recording.xdf --duration 60

# Run in interactive mode (controlled via stdin commands)
cargo run -- --interactive --source-id "stream1" --output data.xdf

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

```rust
// Parent program example
use std::process::{Command, Stdio};
use std::io::Write;

let mut child1 = Command::new("./lsl-recorder")
    .args(["--interactive", "--source-id", "stream1", "--quiet"])
    .stdin(Stdio::piped())
    .spawn()?;

let mut child2 = Command::new("./lsl-recorder")
    .args(["--interactive", "--source-id", "stream2", "--quiet"])
    .stdin(Stdio::piped())
    .spawn()?;

// Independent control - each has its own stdin pipe
writeln!(child1.stdin.as_mut().unwrap(), "START")?;
writeln!(child2.stdin.as_mut().unwrap(), "STOP_AFTER 30")?;
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

# Run with specific LSL library path (required for this environment)
CXXFLAGS="-DPTHREAD_STACK_MIN=16384" cargo build

# Format code
cargo fmt

# Run linter
cargo clippy
```

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
- `CXXFLAGS`: Must include `-DPTHREAD_STACK_MIN=16384` for proper threading

These are pre-configured in `.vscode/settings.json` for the development environment.

## Data Flow

1. **Stream Resolution**: Resolves LSL streams by source ID with configurable timeout
2. **Inlet Creation**: Creates StreamInlet with buffer management and chunk optimization
3. **Data Processing**: Applies post-processing filters and reads data in blocking loops
4. **Output**: Intended to write XDF format files (implementation in progress)
#!/bin/bash
# Quick demo of multi-stream recording

# Build release version
cargo build --release

# Show help
./target/release/lsl-multi-recorder --help

# Start dummy streams in background
./target/release/lsl-dummy-stream --name TestEMG --source-id emg1 --channels 64 --sample-rate 2000 &
./target/release/lsl-dummy-stream --name TestEEG --source-id eeg1 --channels 32 --sample-rate 500 &

# Wait for streams to initialize
sleep 3

# Run multi-recorder and send commands
{
    sleep 5      # Wait for stream resolution
    echo START
    sleep 10     # Record for 10 seconds
    echo STOP
    sleep 1
    echo QUIT
} | ./target/release/lsl-multi-recorder \
    --source-ids emg1 eeg1 \
    --stream-names EMG EEG \
    --output demo_experiment

# Cleanup
pkill lsl-dummy-stream

# Done
echo ""
echo "Recording complete!"
echo "To inspect: bash examples/unix/inspect_demo.sh"

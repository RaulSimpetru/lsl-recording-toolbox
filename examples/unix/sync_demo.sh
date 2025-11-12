#!/bin/bash
# Quick sync examples

# Show help
./target/release/lsl-sync --help

# With trimming (removes data outside common window)
./target/release/lsl-sync demo_experiment.zarr --mode common-start --trim-both

# Inspect after sync
./target/release/lsl-inspect demo_experiment.zarr --verbose

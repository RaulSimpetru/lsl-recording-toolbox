#!/bin/bash
# Quick inspection examples

# Show help
./target/release/lsl-inspect --help

# Basic info
./target/release/lsl-inspect demo_experiment.zarr

# Validate sync
./target/release/lsl-validate demo_experiment.zarr

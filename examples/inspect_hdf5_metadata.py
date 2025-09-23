#!/usr/bin/env python3
"""
Inspect JSON metadata stored in HDF5 files created by the multi_recorder example.

This script demonstrates how to read the comprehensive stream information
and recorder configuration that is now stored as JSON attributes in the
HDF5 files instead of individual attributes.

Usage:
    # Inspect both files created by multi_recorder example
    python3 examples/inspect_hdf5_metadata.py

    # Inspect a specific file
    python3 examples/inspect_hdf5_metadata.py experiment_EMG.h5
    python3 examples/inspect_hdf5_metadata.py experiment_EEG.h5
"""

import h5py
import json
import sys
import os
from pprint import pprint

def inspect_hdf5_metadata(filepath):
    """Read and display JSON metadata from an HDF5 file created by lsl-recorder."""
    print(f"\n=== Inspecting: {filepath} ===")

    if not os.path.exists(filepath):
        print(f"❌ File not found: {filepath}")
        return False

    try:
        with h5py.File(filepath, 'r') as f:
            print("✅ File opened successfully")

            # Show file structure
            print(f"\n📁 File structure:")
            def print_structure(name, obj):
                if isinstance(obj, h5py.Group):
                    print(f"  📂 /{name}")
                else:
                    shape_info = f" {obj.shape}" if hasattr(obj, 'shape') else ""
                    print(f"  📄 /{name}{shape_info}")
            f.visititems(print_structure)

            # Check if we have streams
            if 'streams' in f:
                streams_group = f['streams']
                print(f"\n🎵 Found {len(streams_group.keys())} stream(s): {list(streams_group.keys())}")

                for stream_name in streams_group.keys():
                    stream_group = streams_group[stream_name]
                    print(f"\n--- 🎯 Stream: {stream_name} ---")

                    # Display stream info JSON
                    if 'stream_info_json' in stream_group.attrs:
                        stream_info_raw = stream_group.attrs['stream_info_json']
                        if isinstance(stream_info_raw, bytes):
                            stream_info_raw = stream_info_raw.decode('utf-8')

                        print("\n📊 Stream Information (JSON):")
                        stream_info = json.loads(stream_info_raw)

                        # Highlight key information
                        key_info = {
                            'source_id': stream_info.get('source_id'),
                            'type': stream_info.get('type'),
                            'channel_count': stream_info.get('channel_count'),
                            'nominal_srate': stream_info.get('nominal_srate'),
                            'hostname': stream_info.get('hostname')
                        }

                        print("  🔍 Key Info:")
                        for key, value in key_info.items():
                            print(f"    {key}: {value}")

                        print("\n  📋 Complete Stream Info:")
                        pprint(stream_info, indent=4, width=80)

                        # Show data dimensions
                        if 'data' in stream_group:
                            data_shape = stream_group['data'].shape
                            print(f"\n  📈 Data shape: {data_shape} (samples × channels)")

                        if 'time' in stream_group:
                            time_shape = stream_group['time'].shape
                            time_data = stream_group['time']
                            duration = time_data[-1] - time_data[0] if len(time_data) > 1 else 0
                            print(f"  ⏱️  Time shape: {time_shape} (timestamps)")
                            print(f"  ⏱️  Recording duration: {duration:.2f} seconds")

                    # Display recorder config JSON
                    if 'recorder_config_json' in stream_group.attrs:
                        recorder_config_raw = stream_group.attrs['recorder_config_json']
                        if isinstance(recorder_config_raw, bytes):
                            recorder_config_raw = recorder_config_raw.decode('utf-8')

                        print("\n⚙️  Recorder Configuration (JSON):")
                        recorder_config = json.loads(recorder_config_raw)

                        # Highlight key configuration
                        key_config = {
                            'recorded_at': recorder_config.get('recorded_at'),
                            'subject': recorder_config.get('subject'),
                            'session_id': recorder_config.get('session_id'),
                            'flush_interval': recorder_config.get('flush_interval'),
                            'recorder_version': recorder_config.get('recorder_version')
                        }

                        print("  🔍 Key Config:")
                        for key, value in key_config.items():
                            if value is not None:
                                print(f"    {key}: {value}")

                        print("\n  📋 Complete Recorder Config:")
                        pprint(recorder_config, indent=4, width=80)

            # Check global metadata
            if 'meta' in f:
                meta_group = f['meta']
                print("\n🌐 Global Metadata:")
                for attr_name in meta_group.attrs.keys():
                    attr_value = meta_group.attrs[attr_name]
                    if isinstance(attr_value, bytes):
                        attr_value = attr_value.decode('utf-8')
                    print(f"  {attr_name}: {attr_value}")

        return True

    except Exception as e:
        print(f"❌ Error reading {filepath}: {e}")
        return False

def inspect_multi_recorder_files():
    """Inspect both files created by the multi_recorder example."""
    files = ['experiment_EMG.h5', 'experiment_EEG.h5']

    print("🎯 Multi-Recorder JSON Metadata Inspector")
    print("=" * 50)
    print("This script inspects the JSON metadata stored in HDF5 files")
    print("created by the multi_recorder example (cargo run --example multi_recorder)")

    success_count = 0
    for filepath in files:
        if inspect_hdf5_metadata(filepath):
            success_count += 1

    print(f"\n📊 Summary: Successfully inspected {success_count}/{len(files)} files")

    if success_count == 0:
        print("\n💡 To create these files, run:")
        print("   cargo build")
        print("   cargo run --example multi_recorder")
        print("   # Wait for recording to complete, then run this script")

def main():
    if len(sys.argv) == 1:
        # No arguments - inspect both multi_recorder files
        inspect_multi_recorder_files()
    elif len(sys.argv) == 2:
        # Single file argument
        filepath = sys.argv[1]
        inspect_hdf5_metadata(filepath)
    else:
        print("Usage:")
        print("  python3 examples/inspect_hdf5_metadata.py                    # Inspect both experiment files")
        print("  python3 examples/inspect_hdf5_metadata.py <hdf5_file>        # Inspect specific file")
        print("\nExamples:")
        print("  python3 examples/inspect_hdf5_metadata.py experiment_EMG.h5")
        print("  python3 examples/inspect_hdf5_metadata.py experiment_EEG.h5")
        sys.exit(1)

    print("\n✨ JSON Metadata Benefits:")
    print("• Complete stream information preserved as structured data")
    print("• Full recorder configuration stored for reproducibility")
    print("• Easy to parse programmatically with any JSON library")
    print("• Timestamp shows exact recording start time")
    print("• Better than individual HDF5 attributes for complex metadata")

if __name__ == "__main__":
    main()
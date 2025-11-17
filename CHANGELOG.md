# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.6.0] - 2025-11-17

### Added

- Recording timestamp tracking
  - `first_timestamp`: LSL timestamp of the first recorded sample stored in stream attributes
  - `last_timestamp`: LSL timestamp of the last recorded sample stored in stream attributes
  - Critical for determining if irregular events were cut off by recording end
  - Allows calculation of actual recorded duration: `last_timestamp - first_timestamp`
  - Requested duration already available in `recorder_config.duration` (from `--duration` flag)

### Changed

- **Major metadata cleanup** - Removed all redundant data and simplified zarr structure
  - **Removed `/meta` group entirely** - All metadata was duplicated from other locations
    - Global `/meta` stored subject/session/notes (already in `recorder_config`)
    - Per-stream `/meta/<name>` stored stream info (already in stream group attributes)
  - **Removed `/streams` intermediate group** - Streams now at zarr root (`/EMG/` instead of `/streams/EMG/`)
    - Simpler path structure: `/<stream_name>/data` and `/<stream_name>/time`
    - Updated lsl-sync, lsl-inspect, and lsl-validate to work with new structure
  - **Cleaned recorder_config** - Removed redundant fields:
    - `source_id` → Already in `stream_info.source_id`
    - `output` → Implicit from file path
    - `stream_name` → Implicit from zarr group path
    - `suffix` → Deprecated, always null
  - **Removed duplicate top-level fields**:
    - `recording_host` → Duplicate of `stream_info.hostname`
  - Result: Cleaner structure, smaller file size, single source of truth for all metadata

### Fixed

- **Critical fix**: `lsl-multi-recorder` now correctly passes `--duration` flag to child lsl-recorder processes
  - Previously, the duration was accepted by lsl-multi-recorder but not propagated to child processes
  - This caused `recorder_config.duration` to always be `null` in zarr metadata
  - Now properly records requested duration for analyzing irregular event truncation

## [1.5.0] - 2025-11-17

### Added

- Blosc BitShuffle compression for optimal data storage
  - Automatic shuffle mode selection based on data type:
    - BitShuffle for Float32/Float64 (EMG/EEG signals) - provides 4-8x compression for high-frequency physiological data
    - Byte shuffle for Int8/Int16/Int32 data types
    - No shuffle for String data
  - Proper typesize configuration for all data types (1, 2, 4, or 8 bytes)
  - Applied to both data arrays and timestamp arrays for maximum compression efficiency
  - Significant disk space savings: 0.5-2 GB/hour per stream (vs 4-8 GB/hour without shuffling) for 2-10kHz EMG data
- Automatic stream validation in `lsl-sync`
  - Detects and skips streams with invalid timestamps (< 1.0s indicating uninitialized data)
  - Identifies streams with identical timestamps (bogus/placeholder data)
  - Skips completely empty streams (0 samples)
  - Provides clear warning messages for automatically skipped streams
- Manual stream selection with `--stream` flag in `lsl-sync`
  - Can be specified multiple times to select specific streams: `--stream EMG --stream EEG`
  - Consistent with `lsl-inspect` interface
  - Useful for manual override when automatic validation needs adjustment

### Changed

- Improved irregular stream validation logic in `lsl-sync`
  - Removed overly restrictive minimum sample count criterion (previously required 3+ samples)
  - Now accepts any irregular stream with valid, non-identical timestamps
  - Better support for legitimate low-event streams (e.g., start/stop markers with only 2 events)
  - Validation focuses on data quality issues rather than arbitrary sample counts

### Fixed

- Blosc shuffling now works correctly with proper parameter ordering
  - Fixed BloscCodec::new parameter order: (compressor, level, blocksize, shuffle_mode, typesize)
  - Added helper function `get_blosc_typesize()` to map LSL channel formats to correct byte sizes

## [1.4.0] - 2025-11-16

### Added

- Coordinated STOP_AFTER functionality in `lsl-multi-recorder`
  - New `--duration` parameter for automatic fixed-duration recordings
  - Event-driven architecture monitors FIRST_SAMPLE status from all child recorders
  - Waits for all regular streams to pull their first sample before broadcasting STOP_AFTER
  - Ensures accurate recording duration by eliminating initialization overhead
  - Handles edge case of irregular-only streams (immediate STOP_AFTER broadcast)
- Stream type detection in `lsl-recorder`
  - Emits `STATUS FIRST_SAMPLE (regular)` or `STATUS FIRST_SAMPLE (irregular)` messages
  - Automatic detection based on `nominal_srate == 0.0` for irregular streams
  - Provides feedback to parent multi-recorder for coordination

### Changed

- `lsl-sync` offset display now shows intuitive relative timing
  - Changed from "alignment offset" to "relative to reference" for clarity
  - Positive values indicate stream started AFTER reference time
  - Negative values indicate stream started BEFORE reference time
  - Internal alignment calculations remain unchanged (backwards compatible)

### Fixed

- Fixed recording duration accuracy in multi-stream recordings
  - Previous: ~27s recordings when requesting 30s due to initialization overhead
  - Now: Accurate 30s recordings by coordinating STOP_AFTER after all streams ready
  - Eliminates data loss from sequential stream startup delays

## [1.3.0] - 2025-11-16

### Added

- Intelligent irregular stream handling in `lsl-sync`
  - Automatic detection of irregular streams based on `nominal_srate == 0`
  - Irregular streams (event markers, triggers) no longer constrain the common time window
  - Only regular streams (continuous data) determine alignment reference time and common window
  - Event coverage reporting shows distribution of irregular stream events relative to common window
- Verbose mode for `lsl-sync` (`--verbose` or `-v` flag)
  - Displays detailed stream information including sample rates and time ranges
  - Shows aligned time ranges for each stream after offset calculation
  - Indicates whether alignment uses regular streams or all streams

### Changed

- `lsl-sync` now reads `nominal_srate` from both nested `stream_info.nominal_srate` and top-level attributes
- Common window calculation excludes irregular streams to prevent data loss from continuous streams
- Alignment offset calculation uses only regular streams for reference time determination

### Fixed

- Irregular event streams with sparse events no longer cause loss of continuous data during synchronization
- Proper handling of mixed regular and irregular streams in multi-stream recordings

## [1.2.0] - 2025-11-15

### Changed

- Enhanced license notice to display version number alongside program name
  - All binaries now show version information in the format: `program-name 1.2.0 Copyright (C) ...`
  - Version is automatically sourced from Cargo.toml at compile time

## [1.1.0] - 2025-11-15

### Added

- Support for string-based event marker streams (LSL streams with `String` channel format)
  - Event markers are now stored in 2D Zarr string arrays with shape `[channels, samples]`
  - Typical use case: recording event labels, triggers, and annotations
- Enhanced LSL metadata serialization in Zarr files
  - Stream metadata now includes the full LSL `description` field containing:
    - Channel labels and units
    - Sensor locations/positions
    - Calibration information
    - Custom application-specific metadata
    - Device information
  - Description data is parsed from XML and stored as structured JSON in `/streams/<stream_name>/zarr.json`

### Changed

- Updated Zarr metadata structure to include comprehensive LSL stream descriptor information
- Improved handling of irregular streams (event-based, `nominal_srate = 0`)

## [1.0.0] - 2025-11-12

### Added

- Initial release of LSL Recording Toolbox
- Core recording tools:
  - `lsl-recorder` - Single stream recording to Zarr format
  - `lsl-multi-recorder` - Unified multi-stream recording controller
  - `lsl-sync` - Post-processing timestamp synchronization
  - `lsl-validate` - Synchronization quality analysis
  - `lsl-inspect` - Zarr metadata and structure inspection
  - `lsl-dummy-stream` - Test stream generator
- Zarr v3 format support with hierarchical stream organization
- Support for multiple LSL data formats (Float32, Float64, Int32, Int16, Int8)
- Interactive and direct recording modes
- Configurable buffering and flushing strategies
- Memory monitoring and adaptive buffer sizing
- Comprehensive metadata preservation (subject, session, notes)
- Cross-platform support (Windows, Linux, macOS)
- GitHub Actions CI/CD workflow for automated builds and releases

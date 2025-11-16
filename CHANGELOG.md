# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

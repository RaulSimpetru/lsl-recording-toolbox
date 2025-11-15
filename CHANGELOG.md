# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

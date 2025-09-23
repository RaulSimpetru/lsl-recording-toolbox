use anyhow::Result;
use clap::Parser;
use lsl_recorder::merger::{Hdf5Merger, MergerConfig, TimeReference, ConflictResolution};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "merge_hdf5")]
#[command(about = "Merge multiple HDF5 files created by lsl-recorder into a single file")]
struct Args {
    /// Input HDF5 files to merge
    #[arg(help = "HDF5 files to merge (e.g., experiment_EMG.h5 experiment_EEG.h5)")]
    input_files: Vec<PathBuf>,

    #[arg(
        short = 'o',
        long = "output",
        help = "Output HDF5 file path",
        default_value = "merged_experiment.h5"
    )]
    output: PathBuf,

    #[arg(
        long = "time-ref",
        help = "Time reference strategy for alignment",
        value_enum,
        default_value = "first-stream"
    )]
    time_reference: TimeReferenceArg,

    #[arg(
        long = "conflict",
        help = "Strategy for resolving metadata conflicts",
        value_enum,
        default_value = "merge"
    )]
    conflict_resolution: ConflictResolutionArg,

    #[arg(
        long = "no-provenance",
        help = "Don't preserve provenance information (source files, merge time)"
    )]
    no_provenance: bool,

    #[arg(
        short = 'v',
        long = "verbose",
        help = "Verbose output with detailed progress information"
    )]
    verbose: bool,

    #[arg(
        long = "dry-run",
        help = "Show what would be merged without creating output file"
    )]
    dry_run: bool,

    #[arg(
        long = "trim-start",
        help = "Trim samples before the common start time (when all streams have data)"
    )]
    trim_start: bool,

    #[arg(
        long = "trim-end",
        help = "Trim samples after the common end time (when any stream ends)"
    )]
    trim_end: bool,
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum TimeReferenceArg {
    #[value(name = "first-stream")]
    FirstStream,
    #[value(name = "last-stream")]
    LastStream,
    #[value(name = "absolute-zero")]
    AbsoluteZero,
    #[value(name = "keep-original")]
    KeepOriginal,
    #[value(name = "common-start")]
    CommonStart,
}

impl From<TimeReferenceArg> for TimeReference {
    fn from(arg: TimeReferenceArg) -> Self {
        match arg {
            TimeReferenceArg::FirstStream => TimeReference::FirstStream,
            TimeReferenceArg::LastStream => TimeReference::LastStream,
            TimeReferenceArg::AbsoluteZero => TimeReference::AbsoluteZero,
            TimeReferenceArg::KeepOriginal => TimeReference::KeepOriginal,
            TimeReferenceArg::CommonStart => TimeReference::CommonStart,
        }
    }
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum ConflictResolutionArg {
    #[value(name = "error")]
    Error,
    #[value(name = "use-first")]
    UseFirst,
    #[value(name = "use-last")]
    UseLast,
    #[value(name = "merge")]
    Merge,
}

impl From<ConflictResolutionArg> for ConflictResolution {
    fn from(arg: ConflictResolutionArg) -> Self {
        match arg {
            ConflictResolutionArg::Error => ConflictResolution::Error,
            ConflictResolutionArg::UseFirst => ConflictResolution::UseFirst,
            ConflictResolutionArg::UseLast => ConflictResolution::UseLast,
            ConflictResolutionArg::Merge => ConflictResolution::Merge,
        }
    }
}

fn print_usage_examples() {
    println!("📖 USAGE EXAMPLES:");
    println!("==================");
    println!();
    println!("🔗 Basic merge (align to first stream start time):");
    println!("   cargo run --example merge_hdf5 -- experiment_EMG.h5 experiment_EEG.h5");
    println!();
    println!("🎯 Custom output file:");
    println!("   cargo run --example merge_hdf5 -- *.h5 -o my_merged_data.h5");
    println!();
    println!("⏰ Align all timestamps to start from zero:");
    println!("   cargo run --example merge_hdf5 -- *.h5 --time-ref absolute-zero");
    println!();
    println!("🔀 Handle metadata conflicts by using first occurrence:");
    println!("   cargo run --example merge_hdf5 -- *.h5 --conflict use-first");
    println!();
    println!("🔍 Dry run to see what would be merged:");
    println!("   cargo run --example merge_hdf5 -- *.h5 --dry-run --verbose");
    println!();
    println!("✂️  Drop samples before common start time:");
    println!("   cargo run --example merge_hdf5 -- *.h5 --trim-start");
    println!();
    println!("🎯 Perfect synchronization (all streams start together at t=0):");
    println!("   cargo run --example merge_hdf5 -- *.h5 --time-ref common-start --trim-start");
    println!();
    println!("🎯 Ultimate synchronization (all streams cover exact same time period):");
    println!("   cargo run --example merge_hdf5 -- *.h5 --time-ref common-start --trim-start --trim-end");
    println!();
    println!("📋 Time reference options:");
    println!("   • first-stream:   Align to earliest stream start time (default)");
    println!("   • last-stream:    Align to latest stream start time");
    println!("   • absolute-zero:  All timestamps start from 0");
    println!("   • common-start:   Set t=0 at first timestamp where ALL streams have data");
    println!("   • keep-original:  Preserve original timestamps");
    println!();
    println!("🔧 Conflict resolution options:");
    println!("   • merge:     Combine conflicting metadata intelligently (default)");
    println!("   • use-first: Use metadata from first encountered stream");
    println!("   • use-last:  Use metadata from last encountered stream");
    println!("   • error:     Fail if conflicts are detected");
}

fn main() -> Result<()> {
    let args = Args::parse();

    if args.input_files.is_empty() {
        println!("❌ Error: No input files specified");
        println!();
        print_usage_examples();
        return Ok(());
    }

    println!("🔄 HDF5 Multi-Stream File Merger");
    println!("================================");
    println!();

    // Check if all input files exist
    let mut missing_files = Vec::new();
    for file in &args.input_files {
        if !file.exists() {
            missing_files.push(file.to_string_lossy().to_string());
        }
    }

    if !missing_files.is_empty() {
        println!("❌ Error: The following input files were not found:");
        for file in missing_files {
            println!("   • {}", file);
        }
        println!();
        println!("💡 Make sure to run 'cargo run --example multi_recorder' first to generate test files.");
        return Ok(());
    }

    // Create merger configuration
    let config = MergerConfig {
        output_file: args.output.to_string_lossy().to_string(),
        time_reference: args.time_reference.into(),
        conflict_resolution: args.conflict_resolution.into(),
        preserve_provenance: !args.no_provenance,
        trim_start: args.trim_start,
        trim_end: args.trim_end,
    };

    if args.verbose {
        println!("⚙️  Configuration:");
        println!("   📁 Output file: {}", config.output_file);
        println!("   ⏰ Time reference: {:?}", config.time_reference);
        println!("   🔀 Conflict resolution: {:?}", config.conflict_resolution);
        println!("   📋 Preserve provenance: {}", config.preserve_provenance);
        println!("   ✂️  Trim start: {}", config.trim_start);
        println!("   🎯 Trim end: {}", config.trim_end);
        println!();
    }

    // Create merger and load all files
    let mut merger = Hdf5Merger::new(config);

    for file_path in &args.input_files {
        println!("📂 Loading: {}", file_path.display());
        match merger.add_file(file_path) {
            Ok(()) => println!("   ✅ Successfully loaded"),
            Err(e) => {
                println!("   ❌ Failed to load: {}", e);
                continue;
            }
        }
    }

    println!();

    // Show summary
    if args.verbose || args.dry_run {
        println!("{}", merger.summary());
        println!();
    }

    if args.dry_run {
        println!("🔍 DRY RUN MODE - No files will be created");
        println!("✅ Dry run completed successfully!");
        return Ok(());
    }

    // Perform the merge
    match merger.merge() {
        Ok(()) => {
            println!("🎉 SUCCESS!");
            println!("📁 Merged file created: {}", args.output.display());
            println!();
            println!("🔍 To validate the merged file:");
            println!("   cargo run --example sync_validator");
            println!("   python3 examples/inspect_hdf5_metadata.py {}", args.output.display());
        }
        Err(e) => {
            println!("❌ ERROR: Failed to merge files: {}", e);
            return Err(e);
        }
    }

    Ok(())
}
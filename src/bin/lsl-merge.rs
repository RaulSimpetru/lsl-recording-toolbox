use anyhow::Result;
use clap::Parser;
use lsl_recorder::merger::{ConflictResolution, Hdf5Merger, MergerConfig, TimeReference};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "lsl-merge")]
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
        default_value = "common-start"
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

fn main() -> Result<()> {
    let args = Args::parse();

    if args.input_files.is_empty() {
        println!("Error: No input files specified");
        return Ok(());
    }

    println!("HDF5 Multi-Stream File Merger");
    println!("=============================");
    println!();

    // Check if all input files exist
    let mut missing_files = Vec::new();
    for file in &args.input_files {
        if !file.exists() {
            missing_files.push(file.to_string_lossy().to_string());
        }
    }

    if !missing_files.is_empty() {
        println!("Error: The following input files were not found:");
        for file in missing_files {
            println!("\t{}", file);
        }
        println!();
        println!(
            "Make sure to run 'cargo run --example multi_recorder' first to generate test files."
        );
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
        println!("Configuration:");
        println!("\tOutput file:\t\t{}", config.output_file);
        println!("\tTime reference:\t\t{:?}", config.time_reference);
        println!("\tConflict resolution:\t{:?}", config.conflict_resolution);
        println!("\tPreserve provenance:\t{}", config.preserve_provenance);
        println!("\tTrim start:\t\t{}", config.trim_start);
        println!("\tTrim end:\t\t{}", config.trim_end);
        println!();
    }

    // Create merger and load all files
    let mut merger = Hdf5Merger::new(config);

    for file_path in &args.input_files {
        println!("Loading file:\t{}", file_path.display());
        match merger.add_file(file_path) {
            Ok(()) => println!("\tLoaded successfully"),
            Err(e) => {
                println!("\tFailed to load: {}", e);
                continue;
            }
        }
    }

    println!();

    // Show summary
    if args.verbose {
        println!("{}", merger.summary());
        println!();
    }

    // Perform the merge
    match merger.merge() {
        Ok(()) => {
            println!();
            println!("SUCCESS! Merge operation completed");
            println!("Output file:\t{}", args.output.display());
            println!();
            println!("Next steps:");
            println!("\tValidate:\tlsl-validate {}", args.output.display());
            println!("\tInspect:\tlsl-inspect {}", args.output.display());
        }
        Err(e) => {
            println!();
            println!("ERROR: Merge operation failed - {}", e);
            return Err(e);
        }
    }

    Ok(())
}

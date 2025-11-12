//! LSL Dummy Stream - Generate test LSL streams with sine wave data
//!
//! This tool generates configurable LSL streams with sine wave data for testing
//! and development of recording pipelines.
//!
//! # Features
//!
//! - Generate sine wave test streams
//! - Configurable channel count and sample rate
//! - Customizable stream name, type, and source ID
//! - Adjustable chunk size for streaming
//! - Frequency range configuration per channel
//! - Multiple data types supported (float32, float64, int32, etc.)
//! - Verbose output mode
//!
//! # Usage
//!
//! ```bash
//! # Generate default test stream (100 channels, 10kHz EMG)
//! lsl-dummy-stream
//!
//! # Custom EMG stream
//! lsl-dummy-stream --name "TestEMG" \
//!   --source-id "EMG_1234" \
//!   --channels 8 \
//!   --sample-rate 2000
//!
//! # Generate EEG stream
//! lsl-dummy-stream --name "TestEEG" \
//!   --type "EEG" \
//!   --source-id "EEG_5678" \
//!   --channels 64 \
//!   --sample-rate 1000
//!
//! # Custom frequency range
//! lsl-dummy-stream --name "TestSignal" \
//!   --source-id "SIG_9999" \
//!   --channels 4 \
//!   --freq-range "5,20"
//!
//! # Verbose output
//! lsl-dummy-stream --verbose
//! ```
//!
//! # Signal Generation
//!
//! Generates sine waves with:
//! - Each channel has a different frequency
//! - Frequencies linearly spaced across specified range
//! - Continuous phase-coherent output
//! - Realistic timing and chunk delivery

use anyhow::Result;
use clap::Parser;
use lsl::{Pushable, StreamInfo, StreamOutlet};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Parser)]
#[command(name = "lsl-dummy-stream")]
#[command(about = "Generate dummy LSL streams with sine wave data for testing")]
struct Args {
    #[arg(long = "name", help = "Stream name", default_value = "TestStream")]
    name: String,

    #[arg(long = "type", help = "Stream type", default_value = "EMG")]
    stream_type: String,

    #[arg(long = "source-id", help = "Source ID", default_value = "TEST_1234")]
    source_id: String,

    #[arg(long = "channels", help = "Number of channels", default_value = "100")]
    channels: u32,

    #[arg(
        long = "sample-rate",
        help = "Sampling rate in Hz",
        default_value = "10000"
    )]
    sample_rate: f64,

    #[arg(long = "chunk-size", help = "Samples per chunk", default_value = "18")]
    chunk_size: u32,

    #[arg(
        long = "freq-range",
        help = "Frequency range for channels as 'min,max'",
        default_value = "1,10"
    )]
    freq_range: String,

    #[arg(
        long = "data-type",
        help = "Data type for samples",
        default_value = "float32"
    )]
    data_type: String,

    #[arg(short = 'v', long = "verbose", help = "Verbose output")]
    verbose: bool,
}

fn parse_freq_range(freq_range: &str) -> Result<(f64, f64)> {
    let parts: Vec<&str> = freq_range.split(',').collect();
    if parts.len() != 2 {
        return Err(anyhow::anyhow!(
            "Frequency range must be in format 'min,max'"
        ));
    }

    let min_freq: f64 = parts[0]
        .trim()
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid minimum frequency"))?;
    let max_freq: f64 = parts[1]
        .trim()
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid maximum frequency"))?;

    if min_freq >= max_freq {
        return Err(anyhow::anyhow!(
            "Minimum frequency must be less than maximum frequency"
        ));
    }

    Ok((min_freq, max_freq))
}

fn main() -> Result<()> {
    let args = Args::parse();

    lsl_recording_toolbox::display_license_notice("lsl-dummy-stream");

    // Parse frequency range
    let (min_freq, max_freq) = parse_freq_range(&args.freq_range)?;

    // Parse data type
    let channel_format = match args.data_type.to_lowercase().as_str() {
        "float32" | "f32" => lsl::ChannelFormat::Float32,
        "int16" | "i16" => lsl::ChannelFormat::Int16,
        _ => {
            return Err(anyhow::anyhow!(
                "Invalid data type. Supported: float32, int16"
            ))
        }
    };

    // Create stream info
    let info = StreamInfo::new(
        &args.name,
        &args.stream_type,
        args.channels,
        args.sample_rate,
        channel_format,
        &args.source_id,
    )?;

    // Create outlet
    let outlet = StreamOutlet::new(&info, 0, 360)?;

    println!("LSL Dummy Stream Generator");
    println!("==========================");
    println!("Stream name:\t{}", args.name);
    println!("Stream type:\t{}", args.stream_type);
    println!("Source ID:\t{}", args.source_id);
    println!("Channels:\t{}", args.channels);
    println!("Sample rate:\t{} Hz", args.sample_rate);
    println!("Chunk size:\t{} samples", args.chunk_size);
    println!("Freq. range:\t{:.1} - {:.1} Hz", min_freq, max_freq);
    println!("Data type:\t{:?}", channel_format);
    println!();
    println!("Starting continuous sine wave generation...");
    println!("Press Ctrl+C to stop");
    println!();

    // Calculate frequencies for each channel (linearly spaced)
    let frequencies: Vec<f64> = if args.channels == 1 {
        vec![(min_freq + max_freq) / 2.0]
    } else {
        (0..args.channels)
            .map(|i| min_freq + (max_freq - min_freq) * (i as f64) / ((args.channels - 1) as f64))
            .collect()
    };

    if args.verbose {
        println!("Channel frequencies:");
        for (i, freq) in frequencies.iter().enumerate() {
            println!("\tChannel {}: {:.2} Hz", i + 1, freq);
        }
        println!();
    }

    // Generate and stream data
    let mut sample_count = 0u64;
    let chunk_duration = Duration::from_secs_f64(args.chunk_size as f64 / args.sample_rate);
    let start_time = Instant::now();
    let mut next_chunk_time = start_time;

   macro_rules! generate_and_push_chunk {
        ($ty:ty, $scale:expr, $convert:expr, $outlet:expr, $args:expr, 
        $sample_count:expr, $frequencies:expr) => {{
            let mut chunk: Vec<Vec<$ty>> = Vec::with_capacity($args.chunk_size as usize);

            for sample_idx in 0..$args.chunk_size {
                let sample_time = (($sample_count * $args.chunk_size as u64) + sample_idx as u64)
                    as f64
                    / $args.sample_rate;

                let mut sample: Vec<$ty> = Vec::with_capacity($args.channels as usize);
                for freq in &$frequencies {
                    // Varying amplitude: 0.5 + 0.3 * sin(2π * 0.1 * freq * t)
                    let amplitude =
                        0.5 + 0.3 * (2.0 * std::f64::consts::PI * 0.1 * freq * sample_time).sin();
                    let value_f64 = amplitude * (2.0 * std::f64::consts::PI * freq * sample_time).sin();
                    let value = $convert(value_f64 * $scale);
                    sample.push(value);
                }
                chunk.push(sample);
            }

            // Push chunk to LSL
            $outlet.push_chunk(&chunk)?;
        }};
    }


    loop {
        match channel_format {
            lsl::ChannelFormat::Float32 => {
                generate_and_push_chunk!(
                    f32,          // type
                    1.0,          // scale
                    |v| v as f32, // conversion
                    outlet,
                    args,
                    sample_count,
                    frequencies
                );
            }
            lsl::ChannelFormat::Int16 => {
                generate_and_push_chunk!(
                    i16,
                    32767.0,
                    |v| v as i16,
                    outlet,
                    args,
                    sample_count,
                    frequencies
                );
            }
            _ => unreachable!("Only Float32 and Int16 are supported"),
        }

        if args.verbose && sample_count % 100 == 0 {
            let elapsed = start_time.elapsed().as_secs_f64();
            let samples_sent = (sample_count + 1) * args.chunk_size as u64;
            let expected_samples = (elapsed * args.sample_rate) as u64;
            let drift = samples_sent as i64 - expected_samples as i64;
            println!(
                "Status: {} samples sent in {:.1}s (avg rate: {:.1} Hz, drift: {} samples)",
                samples_sent,
                elapsed,
                samples_sent as f64 / elapsed,
                drift
            );
        }

        sample_count += 1;

        // Calculate when the next chunk should be sent
        next_chunk_time += chunk_duration;

        // Sleep until close to the target time
        let now = Instant::now();
        if next_chunk_time > now {
            let sleep_duration = next_chunk_time - now;

            // If we need to sleep more than 1ms, use thread::sleep for most of it
            if sleep_duration > Duration::from_millis(1) {
                thread::sleep(sleep_duration - Duration::from_millis(1));
            }

            // Spin-wait for the remaining time for better accuracy
            while Instant::now() < next_chunk_time {
                std::hint::spin_loop();
            }
        }
        // If we're already late, don't sleep at all (catch up)
    }

}

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

    loop {
        // Generate chunk based on data type
        match channel_format {
            lsl::ChannelFormat::Float32 => {
                let mut chunk: Vec<Vec<f32>> = Vec::with_capacity(args.chunk_size as usize);

                for sample_idx in 0..args.chunk_size {
                    let sample_time = ((sample_count * args.chunk_size as u64) + sample_idx as u64)
                        as f64
                        / args.sample_rate;

                    let mut sample: Vec<f32> = Vec::with_capacity(args.channels as usize);
                    for freq in &frequencies {
                        // Varying amplitude: 0.5 + 0.3 * sin(2π * 0.1 * freq * t)
                        let amplitude = 0.5
                            + 0.3 * (2.0 * std::f64::consts::PI * 0.1 * freq * sample_time).sin();
                        let value = (amplitude
                            * (2.0 * std::f64::consts::PI * freq * sample_time).sin())
                            as f32;
                        sample.push(value);
                    }
                    chunk.push(sample);
                }

                // Push chunk to LSL
                outlet.push_chunk(&chunk)?;
            }
            lsl::ChannelFormat::Int16 => {
                let mut chunk: Vec<Vec<i16>> = Vec::with_capacity(args.chunk_size as usize);

                for sample_idx in 0..args.chunk_size {
                    let sample_time = ((sample_count * args.chunk_size as u64) + sample_idx as u64)
                        as f64
                        / args.sample_rate;

                    let mut sample: Vec<i16> = Vec::with_capacity(args.channels as usize);
                    for freq in &frequencies {
                        // Varying amplitude: 0.5 + 0.3 * sin(2π * 0.1 * freq * t)
                        let amplitude = 0.5
                            + 0.3 * (2.0 * std::f64::consts::PI * 0.1 * freq * sample_time).sin();
                        let value_f64 =
                            amplitude * (2.0 * std::f64::consts::PI * freq * sample_time).sin();
                        // Scale to int16 range: [-32768, 32767]
                        let value = (value_f64 * 32767.0) as i16;
                        sample.push(value);
                    }
                    chunk.push(sample);
                }

                // Push chunk to LSL
                outlet.push_chunk(&chunk)?;
            }
            _ => unreachable!("Only Float32 and Int16 are supported"),
        }

        if args.verbose && sample_count % 100 == 0 {
            let elapsed = start_time.elapsed().as_secs_f64();
            let samples_sent = (sample_count + 1) * args.chunk_size as u64;
            println!(
                "Status: {} samples sent in {:.1}s (avg rate: {:.1} Hz)",
                samples_sent,
                elapsed,
                samples_sent as f64 / elapsed
            );
        }

        sample_count += 1;

        // Sleep for appropriate duration
        thread::sleep(chunk_duration);
    }
}

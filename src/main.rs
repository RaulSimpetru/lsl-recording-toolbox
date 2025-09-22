use anyhow::Result;
use clap::Parser;
use lsl::Pullable;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing_subscriber::fmt::format;

use hdf5::types::VarLenUnicode;
use hdf5::{Dataset, File, Group};
use hdf5_sys::{h5f::H5Fstart_swmr_write, h5i::hid_t};
use ndarray::{Array1, Array2};
use std::str::FromStr;

#[derive(Parser)]
#[command(name = "lsl-recorder")]
#[command(about = "Record LSL streams to disk with dedicated control interface")]
pub struct Args {
    #[arg(long, help = "LSL stream source ID to record", default_value = "1234")]
    pub source_id: String,

    #[arg(
        long,
        short = 'o',
        help = "Output file path (XDF format)",
        default_value = "output.xdf"
    )]
    pub output: PathBuf,

    #[arg(long, help = "HDF5 experiment file path (enables HDF5 mode)")]
    pub hdf5_file: Option<PathBuf>,

    #[arg(
        long,
        help = "Stream name for HDF5 group (defaults to source-id if not specified)"
    )]
    pub stream_name: Option<String>,

    #[arg(
        long,
        short = 'i',
        help = "Interactive mode - accept commands via stdin"
    )]
    pub interactive: bool,

    #[arg(
        long,
        help = "Auto-start recording (default: true for non-interactive, false for interactive)"
    )]
    pub auto_start: Option<bool>,

    #[arg(long, short = 'd', help = "Maximum recording duration in seconds")]
    pub duration: Option<u64>,

    #[arg(long, default_value = "1000", help = "Stream buffer size")]
    pub buffer_size: usize,

    #[arg(long, short = 'q', help = "Minimal output mode")]
    pub quiet: bool,

    #[arg(
        long,
        default_value = "5.0",
        help = "Timeout for stream resolution in seconds"
    )]
    pub resolve_timeout: f64,

    #[arg(long, help = "Subject identifier for HDF5 metadata")]
    pub subject: Option<String>,

    #[arg(long, help = "Session identifier for HDF5 metadata")]
    pub session_id: Option<String>,

    #[arg(long, help = "Notes for HDF5 metadata")]
    pub notes: Option<String>,

    #[arg(
        long,
        help = "Enable SWMR (Single Writer Multiple Reader) mode for HDF5"
    )]
    pub swmr: bool,
}

/// Robustly open or create HDF5 file with base structure, handling concurrent access
fn open_or_create_hdf5_file(
    file_path: &std::path::Path,
    subject: Option<&str>,
    session_id: Option<&str>,
    notes: Option<&str>,
) -> Result<File> {
    use std::time::Duration;

    // Try to open existing file first
    if file_path.exists() {
        // Try to open existing file with retries
        for attempt in 0..3 {
            match File::open_rw(file_path) {
                Ok(file) => return Ok(file),
                Err(e) => {
                    if attempt < 2 {
                        eprintln!(
                            "Warning: Failed to open existing HDF5 file (attempt {}): {}",
                            attempt + 1,
                            e
                        );
                        std::thread::sleep(Duration::from_millis(100 * (attempt + 1) as u64));
                    } else {
                        return Err(anyhow::anyhow!(
                            "Failed to open existing HDF5 file after 3 attempts: {}",
                            e
                        ));
                    }
                }
            }
        }
    }

    // File doesn't exist, try to create it
    for attempt in 0..3 {
        match create_hdf5_file_with_structure(file_path, subject, session_id, notes) {
            Ok(file) => return Ok(file),
            Err(e) => {
                // Check if file was created by another process while we were trying
                if file_path.exists() {
                    // Another process created it, try to open it
                    match File::open_rw(file_path) {
                        Ok(file) => return Ok(file),
                        Err(open_err) => {
                            eprintln!(
                                "Warning: File exists but cannot open (attempt {}): {}",
                                attempt + 1,
                                open_err
                            );
                        }
                    }
                }

                if attempt < 2 {
                    eprintln!(
                        "Warning: Failed to create HDF5 file (attempt {}): {}",
                        attempt + 1,
                        e
                    );
                    std::thread::sleep(Duration::from_millis(50 * (attempt + 1) as u64));
                } else {
                    return Err(anyhow::anyhow!(
                        "Failed to create HDF5 file after 3 attempts: {}",
                        e
                    ));
                }
            }
        }
    }

    unreachable!()
}

/// Create HDF5 file with base structure, handling the case where groups already exist
fn create_hdf5_file_with_structure(
    file_path: &std::path::Path,
    subject: Option<&str>,
    session_id: Option<&str>,
    notes: Option<&str>,
) -> Result<File> {
    let file = File::create(file_path)?;

    // Create base structure - handle case where groups already exist
    let _ = file
        .create_group("streams")
        .or_else(|_| file.group("streams"));
    let _ = file.create_group("sync").or_else(|_| file.group("sync"));

    let meta_group = file.create_group("meta").or_else(|_| file.group("meta"))?;

    // Add metadata if provided - ignore errors if attributes already exist
    if let Some(subject) = subject {
        if let Ok(subject_unicode) = VarLenUnicode::from_str(subject) {
            let _ = meta_group
                .new_attr::<VarLenUnicode>()
                .create("subject")
                .and_then(|attr| attr.write_scalar(&subject_unicode));
        }
    }

    if let Some(session_id) = session_id {
        if let Ok(session_unicode) = VarLenUnicode::from_str(session_id) {
            let _ = meta_group
                .new_attr::<VarLenUnicode>()
                .create("session_id")
                .and_then(|attr| attr.write_scalar(&session_unicode));
        }
    }

    if let Some(notes) = notes {
        if let Ok(notes_unicode) = VarLenUnicode::from_str(notes) {
            let _ = meta_group
                .new_attr::<VarLenUnicode>()
                .create("notes")
                .and_then(|attr| attr.write_scalar(&notes_unicode));
        }
    }

    // Add start time and global reference - ignore errors if they already exist
    let start_time = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64();
    let _ = meta_group
        .new_attr::<f64>()
        .create("start_time")
        .and_then(|attr| attr.write_scalar(&start_time));

    if let Ok(ref_unicode) = VarLenUnicode::from_str("LSL clock of recorder host") {
        let _ = meta_group
            .new_attr::<VarLenUnicode>()
            .create("global_reference")
            .and_then(|attr| attr.write_scalar(&ref_unicode));
    }

    Ok(file)
}

/// Initialize or open HDF5 file with SWMR mode and create base structure
fn initialize_hdf5_file(
    file_path: &PathBuf,
    subject: Option<&str>,
    session_id: Option<&str>,
    notes: Option<&str>,
    enable_swmr: bool,
) -> Result<File> {
    // Choose file path based on SWMR mode
    let actual_file_path = if enable_swmr {
        // SWMR mode: use shared file for multiple writers
        file_path.clone()
    } else {
        // Non-SWMR mode: create process-specific file path to avoid conflicts
        let process_id = std::process::id();
        if file_path.extension().is_some() {
            file_path.with_file_name(format!(
                "{}_proc_{}.{}",
                file_path.file_stem().unwrap().to_string_lossy(),
                process_id,
                file_path.extension().unwrap().to_string_lossy()
            ))
        } else {
            file_path.with_extension(format!("proc_{}", process_id))
        }
    };

    if !enable_swmr {
        println!("Writing to process-specific file: {:?}", actual_file_path);
    } else {
        println!("Writing to shared SWMR file: {:?}", actual_file_path);
    }

    // Robust file opening with retry logic for concurrent access
    let file = open_or_create_hdf5_file(&actual_file_path, subject, session_id, notes)?;

    // Enable SWMR mode if requested
    if enable_swmr {
        enable_swmr_mode(&file)?;
    }

    Ok(file)
}

/// Enable SWMR (Single Writer Multiple Reader) mode for an HDF5 file
fn enable_swmr_mode(file: &File) -> Result<()> {
    // Get the raw file ID from the high-level File object
    let file_id = file.id();

    // Convert to hid_t for the low-level API
    let raw_file_id = file_id as hid_t;

    // Call the low-level SWMR function
    let result = unsafe { H5Fstart_swmr_write(raw_file_id) };

    if result < 0 {
        eprintln!(
            "Warning: Could not enable SWMR mode (error {}), continuing without SWMR",
            result
        );
        eprintln!("This may be due to file format requirements or VFD compatibility");
    } else {
        eprintln!("SWMR mode enabled successfully");
    }

    // Don't fail the entire recording if SWMR can't be enabled
    Ok(())
}

/// Create or get stream group with datasets for a specific stream
fn setup_stream_group(
    file: &File,
    stream_name: &str,
    info: &lsl::StreamInfo,
    channel_format: lsl::ChannelFormat,
) -> Result<(Group, Dataset, Dataset)> {
    let streams_group = file.group("streams")?;

    // Create or get stream group
    let stream_group = if streams_group.link_exists(stream_name) {
        streams_group.group(stream_name)?
    } else {
        let group = streams_group.create_group(stream_name)?;

        // Add stream metadata as attributes
        group
            .new_attr::<f64>()
            .create("sampling_rate")?
            .write_scalar(&info.nominal_srate())?;

        group
            .new_attr::<VarLenUnicode>()
            .create("source_id")?
            .write_scalar(&VarLenUnicode::from_str(&info.source_id())?)?;

        group
            .new_attr::<i32>()
            .create("channel_count")?
            .write_scalar(&info.channel_count())?;

        group
            .new_attr::<VarLenUnicode>()
            .create("hostname")?
            .write_scalar(&VarLenUnicode::from_str(&info.hostname())?)?;

        group
            .new_attr::<VarLenUnicode>()
            .create("type")?
            .write_scalar(&VarLenUnicode::from_str(&info.stream_type())?)?;

        group
    };

    // Create or get data dataset with appropriate type
    let data_dataset = if stream_group.link_exists("data") {
        stream_group.dataset("data")?
    } else {
        let channels = info.channel_count() as usize;

        macro_rules! create_dataset {
            ($type:ty) => {
                stream_group
                    .new_dataset::<$type>()
                    .chunk((channels, 100))
                    .shape((hdf5::Extent::fixed(channels), hdf5::Extent::resizable(0)))
                    .create("data")?
            };
        }

        match channel_format {
            lsl::ChannelFormat::Float32 => create_dataset!(f32),
            lsl::ChannelFormat::Double64 => create_dataset!(f64),
            lsl::ChannelFormat::Int32 => create_dataset!(i32),
            lsl::ChannelFormat::Int16 => create_dataset!(i16),
            lsl::ChannelFormat::Int8 => create_dataset!(i8),
            _ => {
                return Err(anyhow::anyhow!(
                    "Unsupported channel format for HDF5: {:?}",
                    channel_format
                ))
            }
        }
    };

    // Create or get time dataset
    let time_dataset = if stream_group.link_exists("time") {
        stream_group.dataset("time")?
    } else {
        stream_group
            .new_dataset::<f64>()
            .chunk(100)
            .shape(hdf5::Extent::resizable(0))
            .create("time")?
    };

    Ok((stream_group, data_dataset, time_dataset))
}

/// Enum to handle different LSL data types
#[derive(Debug, Clone)]
enum SampleData {
    Float32(Vec<f32>),
    Float64(Vec<f64>),
    Int32(Vec<i32>),
    Int16(Vec<i16>),
    Int8(Vec<i8>),
    String(Vec<String>),
}

impl SampleData {
    fn len(&self) -> usize {
        match self {
            SampleData::Float32(v) => v.len(),
            SampleData::Float64(v) => v.len(),
            SampleData::Int32(v) => v.len(),
            SampleData::Int16(v) => v.len(),
            SampleData::Int8(v) => v.len(),
            SampleData::String(v) => v.len(),
        }
    }
}

/// Structure to manage HDF5 writing with buffering
struct Hdf5Writer {
    data_dataset: Dataset,
    time_dataset: Dataset,
    sample_buffer: Vec<SampleData>,
    time_buffer: Vec<f64>,
    buffer_size: usize,
    current_length: usize,
    channel_format: lsl::ChannelFormat,
}

impl Hdf5Writer {
    fn new(
        data_dataset: Dataset,
        time_dataset: Dataset,
        buffer_size: usize,
        channel_format: lsl::ChannelFormat,
    ) -> Result<Self> {
        let current_length = data_dataset.shape()[1]; // Second dimension is now time
        Ok(Self {
            data_dataset,
            time_dataset,
            sample_buffer: Vec::new(),
            time_buffer: Vec::new(),
            buffer_size,
            current_length,
            channel_format,
        })
    }

    fn add_sample(&mut self, sample: SampleData, timestamp: f64) {
        self.sample_buffer.push(sample);
        self.time_buffer.push(timestamp);
    }

    fn flush(&mut self) -> Result<()> {
        if self.sample_buffer.is_empty() {
            return Ok(());
        }

        let num_samples = self.sample_buffer.len();
        let num_channels = self.sample_buffer[0].len();
        let new_length = self.current_length + num_samples;

        // Resize datasets to accommodate new data
        self.data_dataset.resize((num_channels, new_length))?;
        self.time_dataset.resize(new_length)?;

        // Prepare time as 1D array
        let time_array = Array1::from_vec(self.time_buffer.clone());

        // Write data based on channel format using write_slice
        macro_rules! write_samples {
            ($type:ty, $variant:ident) => {{
                let mut data_array = Array2::<$type>::zeros((num_channels, num_samples));
                for (i, sample) in self.sample_buffer.iter().enumerate() {
                    if let SampleData::$variant(values) = sample {
                        for (j, &value) in values.iter().enumerate() {
                            data_array[[j, i]] = value; // j is channel, i is time
                        }
                    }
                }
                self.data_dataset
                    .write_slice(&data_array, (.., self.current_length..new_length))?;
            }};
        }

        match self.channel_format {
            lsl::ChannelFormat::Float32 => write_samples!(f32, Float32),
            lsl::ChannelFormat::Double64 => write_samples!(f64, Float64),
            lsl::ChannelFormat::Int32 => write_samples!(i32, Int32),
            lsl::ChannelFormat::Int16 => write_samples!(i16, Int16),
            lsl::ChannelFormat::Int8 => write_samples!(i8, Int8),
            _ => {
                return Err(anyhow::anyhow!(
                    "String format not yet implemented for HDF5"
                ));
            }
        }

        // Write time data to the specific slice
        self.time_dataset
            .write_slice(&time_array, self.current_length..new_length)?;

        self.current_length = new_length;
        self.sample_buffer.clear();
        self.time_buffer.clear();

        // Flush datasets to ensure data is written to disk
        self.data_dataset.file()?.flush()?;

        println!(
            "HDF5: Wrote {} samples (total: {} samples) - {:?}",
            num_samples, self.current_length, self.channel_format
        );

        Ok(())
    }

    fn should_flush(&self) -> bool {
        self.sample_buffer.len() >= self.buffer_size
    }
}

fn handle_commands(recording: Arc<AtomicBool>, quit: Arc<AtomicBool>) -> Result<()> {
    let stdin = io::stdin();
    for line_res in stdin.lock().lines() {
        match line_res {
            Ok(line) => {
                let cmd = line.trim();
                if cmd.eq_ignore_ascii_case("START") {
                    recording.store(true, Ordering::SeqCst);
                    println!("STATUS STARTED");
                    io::stdout().flush().ok();
                } else if cmd.eq_ignore_ascii_case("STOP") {
                    recording.store(false, Ordering::SeqCst);
                    println!("STATUS STOPPED");
                    io::stdout().flush().ok();
                } else if let Some(arg) = cmd.strip_prefix("STOP_AFTER ") {
                    if let Ok(secs) = arg.trim().parse::<u64>() {
                        println!("STATUS WILL STOP AFTER {}s", secs);
                        io::stdout().flush().ok();
                        let recording_clone = recording.clone();
                        thread::spawn(move || {
                            thread::sleep(Duration::from_secs(secs));
                            recording_clone.store(false, Ordering::SeqCst);
                            println!("STATUS STOPPED_BY_TIMER ({}s)", secs);
                            io::stdout().flush().ok();
                        });
                    } else {
                        println!("ERROR bad STOP_AFTER arg");
                        io::stdout().flush().ok();
                    }
                } else if cmd.eq_ignore_ascii_case("QUIT") {
                    println!("STATUS QUIT");
                    io::stdout().flush().ok();
                    quit.store(true, Ordering::SeqCst);
                    break;
                } else if !cmd.is_empty() {
                    println!("ERROR unknown command: {}", cmd);
                    io::stdout().flush().ok();
                }
            }
            Err(e) => {
                eprintln!("stdin read error: {}", e);
                break;
            }
        }
    }
    Ok(())
}

/// Resolve LSL stream with retry logic and random delays to avoid race conditions
fn resolve_lsl_stream_with_retry(
    source_id: &str,
    timeout: f64,
    quiet: bool,
) -> Result<Vec<lsl::StreamInfo>> {
    use std::time::Duration;

    if !quiet {
        println!("Resolving stream...");
    }

    let max_attempts = 3;
    let base_delay_ms = 100;

    for attempt in 0..max_attempts {
        // Add random delay to reduce race conditions between multiple processes
        if attempt > 0 {
            let random_delay = fastrand::u64(0..50); // Random 0-50ms
            let delay = Duration::from_millis(base_delay_ms * attempt as u64 + random_delay);
            if !quiet {
                println!("Retrying stream resolution in {:?}...", delay);
            }
            std::thread::sleep(delay);
        }

        match lsl::resolve_byprop("source_id", &source_id, 1, timeout) {
            Ok(streams) => {
                if !streams.is_empty() {
                    if !quiet && attempt > 0 {
                        println!("Successfully resolved stream on attempt {}", attempt + 1);
                    }
                    return Ok(streams);
                } else {
                    if !quiet {
                        println!("No streams found on attempt {} (will retry)", attempt + 1);
                    }
                }
            }
            Err(e) => {
                if attempt < max_attempts - 1 {
                    if !quiet {
                        println!(
                            "LSL resolution error on attempt {} (will retry): {}",
                            attempt + 1,
                            e
                        );
                    }
                } else {
                    return Err(anyhow::anyhow!(
                        "LSL error after {} attempts: {}",
                        max_attempts,
                        e
                    ));
                }
            }
        }
    }

    Err(anyhow::anyhow!(
        "No stream found with source_id={} after {} attempts",
        source_id,
        max_attempts
    ))
}

fn record_lsl_stream(
    source_id: &str,
    timeout: f64,
    recording: Arc<AtomicBool>,
    quit: Arc<AtomicBool>,
    quiet: bool,
    hdf5_config: Option<(
        PathBuf,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    )>,
    enable_swmr: bool,
) -> Result<()> {
    // Resolve stream with retry logic for robustness
    let res = resolve_lsl_stream_with_retry(source_id, timeout, quiet)?;

    let inl = lsl::StreamInlet::new(&res[0], 300, 0, true)
        .map_err(|e| anyhow::anyhow!("LSL error: {}", e))?;
    let info = inl
        .info(lsl::FOREVER)
        .map_err(|e| anyhow::anyhow!("LSL error: {}", e))?;

    if !quiet {
        println!("Connected to stream with {} channels", info.channel_count());
        println!("Sample rate: {}", info.nominal_srate());
    }

    inl.set_postprocessing(&[
        lsl::ProcessingOption::ClockSync,
        lsl::ProcessingOption::Dejitter,
        lsl::ProcessingOption::Monotonize,
        lsl::ProcessingOption::Threadsafe,
    ])
    .map_err(|e| anyhow::anyhow!("LSL error: {}", e))?;

    // Initialize HDF5 writer if config is provided
    let mut hdf5_writer =
        if let Some((file_path, stream_name, subject, session_id, notes)) = hdf5_config {
            if !quiet {
                println!("Initializing HDF5 file: {:?}", file_path);
                println!("Stream group: {}", stream_name);
            }

            let file = initialize_hdf5_file(
                &file_path,
                subject.as_deref(),
                session_id.as_deref(),
                notes.as_deref(),
                enable_swmr,
            )?;

            let channel_format = info.channel_format();
            let (_group, data_dataset, time_dataset) =
                setup_stream_group(&file, &stream_name, &info, channel_format)?;

            Some(Hdf5Writer::new(
                data_dataset,
                time_dataset,
                100,
                channel_format,
            )?)
        } else {
            None
        };

    // Create appropriate sample buffer based on channel format
    let channel_format = info.channel_format();

    // Create sample buffers for different types
    let mut sample_f32 = Vec::<f32>::new();
    let mut sample_f64 = Vec::<f64>::new();
    let mut sample_i32 = Vec::<i32>::new();
    let mut sample_i16 = Vec::<i16>::new();
    let mut sample_i8 = Vec::<i8>::new();
    let mut sample_string = Vec::<String>::new();

    let mut sample_count: u64 = 0;

    loop {
        if quit.load(Ordering::SeqCst) {
            break;
        }

        if recording.load(Ordering::SeqCst) {
            macro_rules! pull_and_record {
                ($sample_buf:expr, $sample_data:expr) => {{
                    let ts = inl
                        .pull_sample_buf(&mut $sample_buf, 0.1)
                        .map_err(|e| anyhow::anyhow!("LSL error: {}", e))?;

                    if ts != 0.0 {
                        if let Some(ref mut writer) = hdf5_writer {
                            writer.add_sample($sample_data($sample_buf.clone()), ts);
                        }
                    }
                    ts
                }};
            }

            let ts = match channel_format {
                lsl::ChannelFormat::Float32 => pull_and_record!(sample_f32, SampleData::Float32),
                lsl::ChannelFormat::Double64 => pull_and_record!(sample_f64, SampleData::Float64),
                lsl::ChannelFormat::Int32 => pull_and_record!(sample_i32, SampleData::Int32),
                lsl::ChannelFormat::Int16 => pull_and_record!(sample_i16, SampleData::Int16),
                lsl::ChannelFormat::Int8 => pull_and_record!(sample_i8, SampleData::Int8),
                lsl::ChannelFormat::String => pull_and_record!(sample_string, SampleData::String),
                _ => {
                    return Err(anyhow::anyhow!(
                        "Unsupported channel format: {:?}",
                        channel_format
                    ));
                }
            };

            if ts != 0.0 {
                sample_count += 1;

                // Check if we should flush
                if let Some(ref mut writer) = hdf5_writer {
                    if writer.should_flush() {
                        writer.flush()?;
                    }
                }

                if !quiet && sample_count % 100 == 0 {
                    println!("Recorded {} samples", sample_count);
                }
            }
        } else {
            thread::sleep(Duration::from_millis(50));
        }
    }

    // Final flush for any remaining samples
    if let Some(ref mut writer) = hdf5_writer {
        writer.flush()?;
    }

    if !quiet {
        println!("Recording stopped. Total samples: {}", sample_count);
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if !args.quiet {
        tracing_subscriber::fmt::init();
    }

    // Determine auto-start behavior
    let auto_start = args.auto_start.unwrap_or(!args.interactive);

    let recording = Arc::new(AtomicBool::new(auto_start));
    let quit = Arc::new(AtomicBool::new(false));

    // Prepare HDF5 configuration if specified
    let hdf5_config = args.hdf5_file.as_ref().map(|file_path| {
        (
            file_path.clone(),
            args.stream_name
                .clone()
                .unwrap_or_else(|| args.source_id.clone()),
            args.subject.clone(),
            args.session_id.clone(),
            args.notes.clone(),
        )
    });

    if args.interactive {
        // Interactive mode: spawn threads for command handling and recording
        let recording_clone = recording.clone();
        let quit_clone = quit.clone();
        let source_id = args.source_id.clone();
        let timeout = args.resolve_timeout;
        let quiet = args.quiet;
        let swmr = args.swmr;

        // Spawn LSL recording thread
        let recording_thread = {
            let recording = recording_clone;
            let quit = quit_clone;
            let hdf5_config_clone = hdf5_config.clone();
            thread::spawn(move || {
                if let Err(e) = record_lsl_stream(
                    &source_id,
                    timeout,
                    recording,
                    quit,
                    quiet,
                    hdf5_config_clone,
                    swmr,
                ) {
                    eprintln!("Recording error: {}", e);
                }
            })
        };

        // Handle commands on main thread
        if let Err(e) = handle_commands(recording, quit.clone()) {
            eprintln!("Command handling error: {}", e);
        }

        // Wait for recording thread to finish
        recording_thread.join().unwrap();
    } else {
        // Direct recording mode
        if !args.quiet {
            println!(
                "Starting direct recording for source ID: {}",
                args.source_id
            );
            if let Some(duration) = args.duration {
                println!("Recording will stop after {} seconds", duration);
                let recording_clone = recording.clone();
                thread::spawn(move || {
                    thread::sleep(Duration::from_secs(duration));
                    recording_clone.store(false, Ordering::SeqCst);
                });
            }
        }

        record_lsl_stream(
            &args.source_id,
            args.resolve_timeout,
            recording,
            quit,
            args.quiet,
            hdf5_config,
            args.swmr,
        )?;
    }

    Ok(())
}

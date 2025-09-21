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
use std::time::Duration;

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

    #[arg(long, short = 'i', help = "Interactive mode - accept commands via stdin")]
    pub interactive: bool,

    #[arg(long, help = "Auto-start recording (default: true for non-interactive, false for interactive)")]
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

fn record_lsl_stream(
    source_id: &str,
    timeout: f64,
    recording: Arc<AtomicBool>,
    quit: Arc<AtomicBool>,
    quiet: bool,
) -> Result<(), lsl::Error> {
    if !quiet {
        println!("Resolving stream...");
    }

    let res = lsl::resolve_bypred(&format!("source_id={}", source_id), 1, timeout)?;

    if res.is_empty() {
        eprintln!("ERROR: No stream found with source_id={}", source_id);
        return Ok(());
    }

    let inl = lsl::StreamInlet::new(&res[0], 360, 0, true)?;
    let info = inl.info(timeout)?;

    if !quiet {
        println!("Connected to stream with {} channels", info.channel_count());
        println!("Sample rate: {}", info.nominal_srate());
    }

    inl.set_postprocessing(&[
        lsl::ProcessingOption::ClockSync,
        lsl::ProcessingOption::Dejitter,
        lsl::ProcessingOption::Threadsafe,
    ])?;

    let mut sample = Vec::<f32>::new();
    let mut sample_count: u64 = 0;

    loop {
        if quit.load(Ordering::SeqCst) {
            break;
        }

        if recording.load(Ordering::SeqCst) {
            let ts = inl.pull_sample_buf(&mut sample, 0.1)?;

            if ts != 0.0 {
                sample_count += 1;
                if !quiet && sample_count % 100 == 0 {
                    println!("Recorded {} samples", sample_count);
                }
                // TODO: Write to XDF file here
            }
        } else {
            thread::sleep(Duration::from_millis(50));
        }
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

    if args.interactive {
        // Interactive mode: spawn threads for command handling and recording
        let recording_clone = recording.clone();
        let quit_clone = quit.clone();
        let source_id = args.source_id.clone();
        let timeout = args.resolve_timeout;
        let quiet = args.quiet;

        // Spawn LSL recording thread
        let recording_thread = {
            let recording = recording_clone;
            let quit = quit_clone;
            thread::spawn(move || {
                if let Err(e) = record_lsl_stream(&source_id, timeout, recording, quit, quiet) {
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
            println!("Starting direct recording for source ID: {}", args.source_id);
            if let Some(duration) = args.duration {
                println!("Recording will stop after {} seconds", duration);
                let recording_clone = recording.clone();
                thread::spawn(move || {
                    thread::sleep(Duration::from_secs(duration));
                    recording_clone.store(false, Ordering::SeqCst);
                });
            }
        }

        record_lsl_stream(&args.source_id, args.resolve_timeout, recording, quit, args.quiet)?;
    }

    Ok(())
}

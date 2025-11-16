use anyhow::Result;
use std::io::{self, BufRead, Write};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;

pub fn handle_commands(
    recording: Arc<AtomicBool>,
    quit: Arc<AtomicBool>,
    first_sample_pulled: Arc<AtomicBool>,
    is_irregular_stream: Arc<AtomicBool>,
) -> Result<()> {
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
                        let recording_clone = recording.clone();
                        let first_sample_clone = first_sample_pulled.clone();

                        // Check if this is an irregular stream (set by recording thread after stream resolution)
                        if is_irregular_stream.load(Ordering::SeqCst) {
                            // For irregular streams (events): start timer immediately
                            // Don't wait for first sample as events may be sparse or never arrive
                            println!("STATUS WILL STOP AFTER {}s (irregular stream: timer starts immediately)", secs);
                            io::stdout().flush().ok();
                            thread::spawn(move || {
                                println!("STATUS TIMER_STARTED ({}s countdown begins now - irregular stream)", secs);
                                io::stdout().flush().ok();
                                thread::sleep(Duration::from_secs(secs));
                                recording_clone.store(false, Ordering::SeqCst);
                                println!("STATUS STOPPED_BY_TIMER ({}s)", secs);
                                io::stdout().flush().ok();
                            });
                        } else {
                            // For regular streams: wait for first sample before starting timer
                            // This ensures accurate recording duration excluding initialization time
                            println!("STATUS WILL STOP AFTER {}s (regular stream: timer starts after first sample)", secs);
                            io::stdout().flush().ok();
                            thread::spawn(move || {
                                // Wait for first sample to be pulled
                                while !first_sample_clone.load(Ordering::SeqCst) {
                                    thread::sleep(Duration::from_millis(10));
                                }
                                println!("STATUS TIMER_STARTED ({}s countdown begins now)", secs);
                                io::stdout().flush().ok();
                                thread::sleep(Duration::from_secs(secs));
                                recording_clone.store(false, Ordering::SeqCst);
                                println!("STATUS STOPPED_BY_TIMER ({}s)", secs);
                                io::stdout().flush().ok();
                            });
                        }
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

use anyhow::Result;
use std::io::{self, BufRead, Write};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;

pub fn handle_commands(recording: Arc<AtomicBool>, quit: Arc<AtomicBool>) -> Result<()> {
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

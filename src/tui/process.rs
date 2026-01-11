//! Process management for running tools.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;

use anyhow::Result;

use super::app::{find_binary, ToolMetadata};

/// Events that can occur during process execution.
#[derive(Debug)]
pub enum ProcessEvent {
    /// A line of output from stdout or stderr
    Output(String),
    /// The process has exited (currently reported via check_exit instead)
    #[allow(dead_code)]
    Exited(Option<i32>),
    /// An error occurred
    Error(String),
}

/// Manages a running child process with stdin support.
pub struct ProcessManager {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    event_rx: Receiver<ProcessEvent>,
}

impl ProcessManager {
    /// Spawn a tool as a child process with the given arguments.
    /// Terminal size (columns, lines) is passed via environment variables.
    pub fn spawn(tool: &ToolMetadata, args: &[&str], terminal_size: (u16, u16)) -> Result<Self> {
        let binary_path = find_binary(tool.binary);
        let (cols, rows) = terminal_size;

        let mut child = Command::new(&binary_path)
            .args(args)
            .env("COLUMNS", cols.to_string())
            .env("LINES", rows.to_string())
            .env("TERM", "xterm-256color")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to start '{}' at '{}': {}",
                    tool.binary,
                    binary_path.display(),
                    e
                )
            })?;

        let stdin = child.stdin.take();
        let stdout = child.stdout.take().expect("stdout was piped");
        let stderr = child.stderr.take().expect("stderr was piped");

        let (tx, rx) = mpsc::channel();

        // Helper to spawn a reader thread for stdout or stderr
        fn spawn_reader_thread<R: std::io::Read + Send + 'static>(
            reader: R,
            tx: mpsc::Sender<ProcessEvent>,
            stream_name: &'static str,
        ) {
            thread::spawn(move || {
                let reader = BufReader::new(reader);
                for line in reader.lines() {
                    match line {
                        Ok(line) => {
                            if tx.send(ProcessEvent::Output(line)).is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(ProcessEvent::Error(format!("{} error: {}", stream_name, e)));
                            break;
                        }
                    }
                }
            });
        }

        spawn_reader_thread(stdout, tx.clone(), "stdout");
        spawn_reader_thread(stderr, tx, "stderr");

        Ok(Self {
            child: Some(child),
            stdin,
            event_rx: rx,
        })
    }

    /// Try to receive an event without blocking.
    pub fn try_recv(&self) -> Option<ProcessEvent> {
        self.event_rx.try_recv().ok()
    }

    /// Write a line to the process's stdin.
    pub fn write_line(&mut self, line: &str) -> Result<()> {
        if let Some(ref mut stdin) = self.stdin {
            writeln!(stdin, "{}", line)?;
            stdin.flush()?;
        }
        Ok(())
    }

    /// Check if the process has exited.
    pub fn check_exit(&mut self) -> Option<Option<i32>> {
        if let Some(ref mut child) = self.child {
            match child.try_wait() {
                Ok(Some(status)) => {
                    self.child = None;
                    self.stdin = None;
                    Some(status.code())
                }
                Ok(None) => None, // Still running
                Err(_) => {
                    self.child = None;
                    self.stdin = None;
                    Some(None)
                }
            }
        } else {
            None
        }
    }

    /// Kill the running process.
    pub fn kill(&mut self) {
        // Drop stdin first to signal EOF to the process
        self.stdin = None;
        if let Some(ref mut child) = self.child {
            let _ = child.kill();
            let _ = child.wait();
            self.child = None;
        }
    }

    /// Check if a process is currently running.
    #[allow(dead_code)]
    pub fn is_running(&self) -> bool {
        self.child.is_some()
    }
}

impl Drop for ProcessManager {
    fn drop(&mut self) {
        self.kill();
    }
}

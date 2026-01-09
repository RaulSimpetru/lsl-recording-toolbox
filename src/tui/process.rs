//! Process management for running tools.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
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

/// Manages a running child process.
pub struct ProcessManager {
    child: Option<Child>,
    event_rx: Receiver<ProcessEvent>,
}

impl ProcessManager {
    /// Spawn a tool as a child process with the given arguments.
    pub fn spawn(tool: &ToolMetadata, args: &[&str]) -> Result<Self> {
        let binary_path = find_binary(tool.binary);

        let mut child = Command::new(&binary_path)
            .args(args)
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

        let stdout = child.stdout.take().expect("stdout was piped");
        let stderr = child.stderr.take().expect("stderr was piped");

        let (tx, rx) = mpsc::channel();

        // Spawn thread to read stdout
        let stdout_tx = tx.clone();
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(line) => {
                        if stdout_tx.send(ProcessEvent::Output(line)).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = stdout_tx.send(ProcessEvent::Error(format!("stdout error: {}", e)));
                        break;
                    }
                }
            }
        });

        // Spawn thread to read stderr
        let stderr_tx = tx;
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                match line {
                    Ok(line) => {
                        if stderr_tx.send(ProcessEvent::Output(line)).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = stderr_tx.send(ProcessEvent::Error(format!("stderr error: {}", e)));
                        break;
                    }
                }
            }
        });

        Ok(Self {
            child: Some(child),
            event_rx: rx,
        })
    }

    /// Try to receive an event without blocking.
    pub fn try_recv(&self) -> Option<ProcessEvent> {
        self.event_rx.try_recv().ok()
    }

    /// Check if the process has exited.
    pub fn check_exit(&mut self) -> Option<Option<i32>> {
        if let Some(ref mut child) = self.child {
            match child.try_wait() {
                Ok(Some(status)) => {
                    self.child = None;
                    Some(status.code())
                }
                Ok(None) => None, // Still running
                Err(_) => {
                    self.child = None;
                    Some(None)
                }
            }
        } else {
            None
        }
    }

    /// Kill the running process.
    pub fn kill(&mut self) {
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

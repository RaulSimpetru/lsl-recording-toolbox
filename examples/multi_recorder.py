#!/usr/bin/env python3
"""
Example Python script demonstrating cross-platform multi-instance LSL recording
using anonymous pipes for independent control.
"""

import subprocess
import time
from datetime import datetime

def log_with_time(message):
    """Print message with timestamp"""
    timestamp = datetime.now().strftime("%H:%M:%S.%f")[:-3]
    print(f"[{timestamp}] {message}")

def main():
    log_with_time("Spawning multiple LSL recorders from Python...")

    # Spawn two recorder instances with independent stdin pipes
    recorder1 = subprocess.Popen([
        ".././target/debug/lsl-recorder",
        "--interactive",
        "--source-id", "stream1",
        "--quiet"
    ], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)

    recorder2 = subprocess.Popen([
        ".././target/debug/lsl-recorder",
        "--interactive",
        "--source-id", "stream2",
        "--quiet"
    ], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)

    log_with_time("Both recorders spawned successfully")

    try:
        # Check if stdin pipes were created successfully
        if not recorder1.stdin or not recorder2.stdin:
            log_with_time("ERROR: Failed to create stdin pipes")
            return

        # Example control sequence
        log_with_time("Sending START command to both recorders...")
        recorder1.stdin.write("START\n")
        recorder1.stdin.flush()
        log_with_time("  → START sent to recorder1")
        recorder2.stdin.write("START\n")
        recorder2.stdin.flush()
        log_with_time("  → START sent to recorder2")

        log_with_time("Waiting 2 seconds...")
        time.sleep(2)

        log_with_time("Setting recorder2 to stop after 5 seconds...")
        recorder2.stdin.write("STOP_AFTER 5\n")
        recorder2.stdin.flush()
        log_with_time("  → STOP_AFTER 5 sent to recorder2")

        log_with_time("Waiting 3 seconds...")
        time.sleep(3)

        log_with_time("Stopping recorder1...")
        recorder1.stdin.write("STOP\n")
        recorder1.stdin.flush()
        log_with_time("  → STOP sent to recorder1")

        log_with_time("Waiting 3 seconds...")
        time.sleep(3)

        log_with_time("Sending QUIT to both recorders...")
        recorder1.stdin.write("QUIT\n")
        recorder1.stdin.flush()
        log_with_time("  → QUIT sent to recorder1")
        recorder2.stdin.write("QUIT\n")
        recorder2.stdin.flush()
        log_with_time("  → QUIT sent to recorder2")

    except Exception as e:
        log_with_time(f"ERROR: {e}")

    finally:
        # Wait for processes to finish
        log_with_time("Waiting for processes to finish...")
        recorder1.wait()
        log_with_time("  → recorder1 finished")
        recorder2.wait()
        log_with_time("  → recorder2 finished")
        log_with_time("All recorders finished successfully")

if __name__ == "__main__":
    main()
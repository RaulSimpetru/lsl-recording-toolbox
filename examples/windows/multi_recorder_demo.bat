@echo off
REM Quick demo of multi-stream recording

REM Build release version
cargo build --release

REM Show help
.\target\release\lsl-multi-recorder.exe --help

REM Start dummy streams in background
start /B .\target\release\lsl-dummy-stream.exe --name TestEMG --source-id emg1 --channels 8  --sample-rate 1000
start /B .\target\release\lsl-dummy-stream.exe --name TestEEG --source-id eeg1 --channels 16 --sample-rate 500

REM Wait for streams to initialize
timeout /t 3 /nobreak > nul

REM Run multi-recorder with piped commands
(
    timeout /t 5 /nobreak > nul
    echo START
    timeout /t 10 /nobreak > nul
    echo STOP
    timeout /t 1 /nobreak > nul
    echo QUIT
) | .\target\release\lsl-multi-recorder.exe --source-ids emg1 eeg1 --stream-names EMG EEG --output demo_experiment

REM Cleanup
taskkill /IM lsl-dummy-stream.exe /F > nul 2>&1

REM Done
echo.
echo Recording complete!
echo To inspect: examples\windows\inspect_demo.bat

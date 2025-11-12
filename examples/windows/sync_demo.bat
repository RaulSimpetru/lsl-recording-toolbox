@echo off
REM Quick sync examples

REM Show help
.\target\release\lsl-sync.exe --help

REM With trimming (removes data outside common window)
.\target\release\lsl-sync.exe demo_experiment.zarr --mode common-start --trim-both

REM Inspect after sync
.\target\release\lsl-inspect.exe demo_experiment.zarr --verbose

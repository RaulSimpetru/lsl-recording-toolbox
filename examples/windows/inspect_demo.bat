@echo off
REM Quick inspection examples

REM Show help
.\target\release\lsl-inspect.exe --help

REM Basic info
.\target\release\lsl-inspect.exe demo_experiment.zarr

REM Validate sync
.\target\release\lsl-validate.exe demo_experiment.zarr

@echo off
REM Build the WASM bundle that the SolFlow editor consumes.
REM See build.sh for the rationale; this is the Windows equivalent.
cd /d %~dp0
wasm-pack build --release --target bundler --out-dir pkg
if errorlevel 1 exit /b 1
echo.
echo Built pkg/. Bundle:
dir pkg\solflow_compiler_wasm_bg.wasm

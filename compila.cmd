@echo off
rem Strumenti di compilazione per Photo Restore su questo computer.
rem Usa la toolchain Rust GNU + MinGW64 in %USERPROFILE%\.mingw64
setLOCAL
set "PATH=%USERPROFILE%\.cargo\bin;%USERPROFILE%\.mingw64\mingw64\bin;%PATH%"
set "CARGO_TARGET_DIR=%USERPROFILE%\pr-target"
if "%~1"=="test" (
  cargo +stable-x86_64-pc-windows-gnu test --target x86_64-pc-windows-gnu --workspace
) else if "%~1"=="verifica" (
  cargo +stable-x86_64-pc-windows-gnu clippy --target x86_64-pc-windows-gnu --workspace -- -D warnings
) else if "%~1"=="pulisci" (
  cargo +stable-x86_64-pc-windows-gnu fmt --all
) else (
  cargo +stable-x86_64-pc-windows-gnu build --target x86_64-pc-windows-gnu --workspace %*
)

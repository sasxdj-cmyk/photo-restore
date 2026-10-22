@echo off
rem Avvia Photo Restore senza passare da "tauri dev" (che ricompila tutto).
rem Se non esiste ancora il binario di rilascio lo compila una volta.

setlocal
set "PATH=%USERPROFILE%\.cargo\bin;%USERPROFILE%\.mingw64\mingw64\bin;%PATH%"
if not defined CARGO_TARGET_DIR set "CARGO_TARGET_DIR=%USERPROFILE%\pr-target"

set "EXE=%CARGO_TARGET_DIR%\release\photo-restore-app.exe"

if not exist "%EXE%" (
  echo Prima compilazione in corso, attendere...
  cargo build --workspace --release || (echo Compilazione fallita & exit /b 1)
)

start "" "%EXE%"

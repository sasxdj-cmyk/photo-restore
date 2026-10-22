@echo off
rem Avvia la versione di sviluppo di Photo Restore (finestra con ricaricamento).
rem Requisiti su questo computer: Rust GNU predefinito + MinGW64 in %USERPROFILE%\.mingw64
setLOCAL
set "PATH=%USERPROFILE%\.cargo\bin;%USERPROFILE%\.mingw64\mingw64\bin;%PATH%"
set "CARGO_TARGET_DIR=%USERPROFILE%\pr-target"
set "CARGO_BUILD_TARGET=x86_64-pc-windows-gnu"
call npm run dev

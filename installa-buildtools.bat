@echo off
rem Installazione degli strumenti di compilazione per Photo Restore
rem (Visual Studio 2022 Build Tools - workload C++, gratuito)
rem Nota: file necessario solo per sviluppare; l'utente finale non lo usa.
if not exist "%TEMP%\opencode\vs_BuildTools.exe" (
  echo Scarico Visual Studio Build Tools...
  powershell -NoProfile -Command "[Net.ServicePointManager]::SecurityProtocol=[Net.SecurityProtocolType]::Tls12; Invoke-WebRequest -Uri https://aka.ms/vs/17/release/vs_BuildTools.exe -OutFile \"$env:TEMP\opencode\vs_BuildTools.exe\""
  if not exist "%TEMP%\opencode\" mkdir "%TEMP%\opencode"
)
echo Avvio installazione silenziosa: approva la finestra di Controllo dell'account utente.
"%TEMP%\opencode\vs_BuildTools.exe" --quiet --wait --norestart --nocache --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended
echo Fatto. Verifica: cerca link.exe sotto C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC
pause

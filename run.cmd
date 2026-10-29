@echo off
setlocal
set "APP=%~dp0target\aarch64-pc-windows-msvc\release\mio-note.exe"
if not exist "%APP%" (
    echo First build the app with: powershell -File "%~dp0build.ps1"
    pause
    exit /b 1
)
start "" "%APP%"

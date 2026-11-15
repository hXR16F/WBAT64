@echo off
rem Build WBAT64 and refresh the copy used by the batch examples.
setlocal
cd /d "%~dp0"
cargo build --release
if errorlevel 1 (
    echo build failed.
    exit /b 1
)
if not exist "bin" mkdir "bin"
copy /y "target\release\wbat64.exe" "bin\wbat64.exe" >nul
if errorlevel 1 (
    echo could not copy target\release\wbat64.exe to bin.
    exit /b 1
)
echo Built bin\wbat64.exe

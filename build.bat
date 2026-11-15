@echo off
rem Build WBAT64 for Windows and Linux and prepare the distribution directory.
setlocal
cd /d "%~dp0"

if not exist "bin" mkdir "bin"

echo.
echo [1/4] Building WBAT64 for Windows...
cargo build --release --target x86_64-pc-windows-msvc
if errorlevel 1 (
    echo Windows build failed.
    exit /b 1
)

copy /y "target\x86_64-pc-windows-msvc\release\wbat64.exe" "bin\wbat64.exe" >nul
if errorlevel 1 (
    echo Could not copy Windows executable.
    exit /b 1
)

echo.
echo [2/4] Building WBAT64 for Linux...
cross build --release --target x86_64-unknown-linux-gnu
if errorlevel 1 (
    echo Linux build failed.
    exit /b 1
)

copy /y "target\x86_64-unknown-linux-gnu\release\wbat64" "bin\wbat64" >nul
if errorlevel 1 (
    echo Could not copy Linux executable.
    exit /b 1
)

echo.
echo [3/4] Copying documentation...
copy /y "README.md" "bin\README.md" >nul
copy /y "LICENSE" "bin\LICENSE" >nul

echo.
echo [4/4] Copying examples...
if exist "bin\examples" rmdir /s /q "bin\examples"
xcopy "examples" "bin\examples\" /E /I /H /Y >nul
if errorlevel 1 (
    echo Could not copy examples.
    exit /b 1
)

echo.
echo ========================================
echo WBAT64 build completed successfully.
echo ========================================
echo.
echo Distribution:
echo   bin\wbat64.exe  - Windows x64
echo   bin\wbat64      - Linux x64 GNU
echo   bin\README.md
echo   bin\LICENSE
echo   bin\examples\
echo.

exit /b 0

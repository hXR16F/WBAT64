@echo off
rem 05 - Pick a file from a directory listing.
rem exit code 0 = picked (item number in %?%, line in %wbat%),
rem 254 = ESC, 255 = error.
setlocal
call "%~dp0..\_env.bat"

dir /b /a-d > "%TEMP%\wbat64_files.txt"
call "%SHIM%" list 3,10 "%TEMP%\wbat64_files.txt" %*
set "rc=%ERRORLEVEL%"
if "%rc%"=="255" (
    echo Could not read the list.
    del "%TEMP%\wbat64_files.txt" >nul 2>&1
    exit /b 1
)
if "%rc%"=="254" (
    echo Cancelled.
    del "%TEMP%\wbat64_files.txt" >nul 2>&1
    exit /b 1
)

echo You picked item %?%: %wbat%
del "%TEMP%\wbat64_files.txt" >nul 2>&1
exit /b 0

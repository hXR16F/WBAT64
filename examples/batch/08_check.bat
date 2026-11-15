@echo off
rem 08 - Validate layouts before shipping them.
rem Prints "OK: n units" or file:line:column diagnostics.
setlocal
call "%~dp0..\_env.bat"

"%WBAT64_EXE%" check "@%DIALOGS%"
if errorlevel 1 (
    echo.
    echo This layout file has problems - see the messages above.
    exit /b 1
)
echo Layout is valid.
exit /b 0

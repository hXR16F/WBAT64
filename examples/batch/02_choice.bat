@echo off
rem 02 - A CHOICE replacement: three buttons, exit code 1..3 or 100 for ESC.
rem `if errorlevel n` means "n or greater", so test in descending order.
setlocal
call "%~dp0..\_env.bat"

"%WBAT64_EXE%" box 19,55 (blue on +brown) "Continue installation?^Existing files will be replaced." yes,no,cancel %*
if errorlevel 255 exit /b 1
if errorlevel 100 goto cancelled
if errorlevel 3 goto cancel
if errorlevel 2 goto skip
if errorlevel 1 goto install
goto cancelled

:install
echo Installing...
exit /b 0

:skip
echo Skipping existing files.
exit /b 0

:cancel
echo Cancelled.
exit /b 1

:cancelled
echo Cancelled.
exit /b 1

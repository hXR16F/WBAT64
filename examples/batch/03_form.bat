@echo off
rem 03 - A form with inputs, radio buttons and checkboxes, imported into
rem this batch file by w.bat.
setlocal
call "%~dp0..\_env.bat"

set "name="
set "city=Berlin"
set "WRB=2"
set "WCB1=0"
set "WCB2=1"

call "%SHIM%" box "@%DIALOGS%:signup" %*
if errorlevel 255 exit /b 1
if errorlevel 100 (
    echo Cancelled - all variables keep their previous values.
    exit /b 1
)

echo.
echo   Name=%name%
echo   City=%city%
echo   Gender=%WRB%   Over21=%WCB1%   Newsletter=%WCB2%
echo.
pause

@echo off
rem 01 - A PAUSE replacement: one box, one OK button.
rem Press Enter, Space or click the button to continue.
setlocal
call "%~dp0..\_env.bat"
echo Installing...
"%WBAT64_EXE%" box 10,20 "Installing..." " OK " %*
echo Done.

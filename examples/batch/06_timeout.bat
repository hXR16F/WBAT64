@echo off
rem 06 - A box with a countdown (#1,3 = close after 3 seconds, return button 1).
rem Any key or click cancels the timer; #? in the layout shows the seconds.
rem w.bat imports WBAT/?/input variables into this batch file.
setlocal
call "%~dp0..\_env.bat"

echo This box closes by itself; press a key to stop the countdown.
call "%SHIM%" box "@%DIALOGS%:autoclose" #1,3 %*
if errorlevel 255 exit /b 1
echo Returned WBAT=%wbat% with errorlevel %ERRORLEVEL%

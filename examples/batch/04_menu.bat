@echo off
rem 04 - A menu loop driven by WBAT and errorlevel.
rem #%mpos%+1 reopens the bar on the item after the previous choice.
setlocal
call "%~dp0..\_env.bat"
set "mpos=0"

:main
call "%SHIM%" menu "@%DIALOGS%:main_menu" #%mpos%+1 %*
if errorlevel 255 exit /b 1
set "mpos=%?%"
if errorlevel 100 goto done
if /i "%wbat%"=="Tour" goto tour
if /i "%wbat%"=="Setup" goto setup
if /i "%wbat%"=="Report" goto report
goto main

:tour
cls
echo This is the "tour" branch.
pause
goto main

:setup
echo This is the "setup" branch.
pause
goto main

:report
echo This is the "report" branch.
pause
goto main

:done
echo Exiting the menu.
exit /b 0

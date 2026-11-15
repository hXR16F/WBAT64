@echo off
rem A short tour of wbat64 from a batch file.
rem Run from anywhere; paths are resolved from this file's directory.
setlocal
call "%~dp0_env.bat"
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
"%WBAT64_EXE%" cls (white on blue) --and text 3,6 (+white on red) " wbat64 tour " --and text 6,6 "Press a key to continue..."
"%WBAT64_EXE%" box 12,10 "This box has a title, a shadow and buttons." OK
goto main

:setup
set "city=Berlin"
set "WRB=2"
set "WCB1=0"
set "WCB2=1"
call "%SHIM%" box "@%DIALOGS%:signup"
if errorlevel 255 exit /b 1
if errorlevel 100 goto main
echo.
echo   Name=%name%  City=%city%  Gender=%WRB%  Over21=%WCB1%  Newsletter=%WCB2%
echo.
pause
goto main

:report
cls
dir /b /a-d > "%TEMP%\wbat64_files.txt"
call "%SHIM%" list 3,10 "%TEMP%\wbat64_files.txt"
if errorlevel 255 goto main
if errorlevel 254 goto main
echo You picked %wbat% (item %?%)
del "%TEMP%\wbat64_files.txt" >nul 2>&1
pause
goto main

:done
for %%a in (wbat mpos) do set "%%a="
set "?="
exit /b 0

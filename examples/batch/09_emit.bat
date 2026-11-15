@echo off
rem 09 - Getting results without w.bat: write the set lines to a file,
rem then call that file. This is the same mechanism w.bat uses.
setlocal
call "%~dp0..\_env.bat"

set "vars=%TEMP%\wbat64_vars_%RANDOM%.bat"
"%WBAT64_EXE%" box "@%DIALOGS%:main_menu" --emit bat %* > "%vars%"
set "rc=%ERRORLEVEL%"
if exist "%vars%" call "%vars%"
if exist "%vars%" del "%vars%" >nul 2>&1

echo Exit code: %rc%
echo WBAT:      %wbat%
echo ? =        %?%
exit /b %rc%

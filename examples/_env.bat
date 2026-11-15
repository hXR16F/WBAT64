@echo off
rem Shared environment for the batch examples.
rem Sets WBAT64_EXE (executable), DIALOGS (layout file) and SHIM (w.bat).
rem Prefer a binary beside the scripts or in this checkout, then a built
rem release, then PATH.
if not defined WBAT64_EXE if exist "%~dp0wbat64.exe" set "WBAT64_EXE=%~dp0wbat64.exe"
if not defined WBAT64_EXE if exist "%~dp0..\wbat64.exe" set "WBAT64_EXE=%~dp0..\wbat64.exe"
if not defined WBAT64_EXE if exist "%~dp0..\bin\wbat64.exe" set "WBAT64_EXE=%~dp0..\bin\wbat64.exe"
if not defined WBAT64_EXE if exist "%~dp0..\target\release\wbat64.exe" set "WBAT64_EXE=%~dp0..\target\release\wbat64.exe"
if defined WBAT64_EXE goto :wbat64_env_done
where wbat64 >nul 2>&1
if not errorlevel 1 set "WBAT64_EXE=wbat64"
:wbat64_env_done
set "DIALOGS=%~dp0wbat.txt"
set "SHIM=%~dp0..\w.bat"

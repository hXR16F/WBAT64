@echo off
rem w.bat - run WBAT64 and import the resulting variables into this batch file.
rem Usage:  call w.bat box @WBAT.TXT:signup
rem Set WBAT64_EXE if the executable is not on PATH; set WBAT64_TMP to force the
rem directory used for the temporary result file.
rem No setlocal on purpose: the variables written by WBAT64 must reach the caller.

rem Resolve the executable: an explicit path/command, the copy shipped next to
rem w.bat, then PATH. A missing executable gets its own message instead of the
rem misleading "cannot create a temporary file".
if defined WBAT64_EXE (
    "%WBAT64_EXE%" --version >nul 2>&1
    if not errorlevel 1 goto :wbat64_exe_ok
    if defined WBAT64_DEBUG echo w.bat: WBAT64_EXE="%WBAT64_EXE%" is not runnable 1>&2
)
if exist "%~dp0wbat64.exe" (
    set "WBAT64_EXE=%~dp0wbat64.exe"
    goto :wbat64_exe_ok
)
if exist "%~dp0bin\wbat64.exe" (
    set "WBAT64_EXE=%~dp0bin\wbat64.exe"
    goto :wbat64_exe_ok
)
if exist "%~dp0target\release\wbat64.exe" (
    set "WBAT64_EXE=%~dp0target\release\wbat64.exe"
    goto :wbat64_exe_ok
)
where wbat64 >nul 2>&1
if not errorlevel 1 (
    set "WBAT64_EXE=wbat64"
    goto :wbat64_exe_ok
)
goto :wbat64_missing

:wbat64_exe_ok
if defined WBAT64_DEBUG echo w.bat: wbat64 = "%WBAT64_EXE%" 1>&2

set "_wf="
if defined WBAT64_TMP call :wbat64_pick "%WBAT64_TMP%"
if not defined _wf call :wbat64_pick "%CD%"
if not defined _wf call :wbat64_pick "%~dp0"
set "_wbdir="
for %%I in ("%WBAT64_EXE%") do set "_wbdir=%%~dpI"
if defined _wbdir call :wbat64_pick "%_wbdir%"
if not defined _wf call :wbat64_pick "%TEMP%"
if not defined _wf goto :wbat64_nodir

"%WBAT64_EXE%" --emit-file "%_wf%" %*
set "_wrc=%ERRORLEVEL%"
if exist "%_wf%" call "%_wf%"
if exist "%_wf%" del "%_wf%" >nul 2>&1
set "_wf=" & set "_wrc=" & exit /b %_wrc%

:wbat64_missing
echo w.bat: wbat64 executable not found. 1>&2
echo   Build it with "cargo build --release" and keep bin\wbat64.exe, 1>&2
echo   or set WBAT64_EXE to the wbat64 executable. 1>&2
exit /b 255

:wbat64_nodir
echo w.bat: cannot create a temporary batch file. 1>&2
if defined WBAT64_TMP echo   tried WBAT64_TMP="%WBAT64_TMP%" 1>&2
echo   tried "%CD%", "%~dp0", the executable directory and "%TEMP%". 1>&2
echo   Set WBAT64_TMP to a writable directory. 1>&2
exit /b 255

:wbat64_pick
set "_dir=%~1"
if "%_dir%"=="" exit /b 0
if "%_dir:~-1%"=="\" set "_dir=%_dir:~0,-1%"
if not exist "%_dir%\" exit /b 0
set "_candidate=%_dir%\wbat64_%RANDOM%%RANDOM%.bat"
"%WBAT64_EXE%" --probe-write "%_candidate%" >nul 2>nul
set "_probe=%ERRORLEVEL%"
if defined WBAT64_DEBUG echo w.bat: probe "%_candidate%" = %_probe% 1>&2
if "%_probe%"=="0" set "_wf=%_candidate%"
if defined _wf exit /b 0
rem Older WBAT64 builds do not know --probe-write (255); fall back to cmd's test.
if not "%_probe%"=="255" exit /b 0
copy /y nul "%_candidate%" >nul 2>nul
if not exist "%_candidate%" exit /b 0
del "%_candidate%" >nul 2>&1
set "_wf=%_candidate%"
exit /b 0

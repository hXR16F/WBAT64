@echo off
rem 07 - Draw a screen, then show a box on top of it.
rem '--and' chains commands without using '!' (safe with delayed expansion).
setlocal
call "%~dp0..\_env.bat"

"%WBAT64_EXE%" cls (white on blue) --and text 2,4 (+white on red) " Please ignore this message! " --and fill 10,4 (6,40) (+magenta on blue) "#" +2 --and box 20,30 "The box is on top." OK %*

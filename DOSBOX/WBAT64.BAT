@echo off
setlocal EnableDelayedExpansion
set "curr_path=%cd:~3,256%"
set "hmdrive=%homedrive:~0,1%"
if not exist "1.txt" (
	echo.> "1.txt"
	call wbat\DOSBox.exe -noconsole -c "mount %hmdrive% %hmdrive%:/" -c "%hmdrive%:" -c "cd %curr_path%/wbat" -c "cls" -c "call INDOS.BAT" -c "WBAT.COM %~1 ^&echo %ERRORLEVEL%>CLICKED.BAT" -c "exit"
	call run.bat
	goto :eof
) else (
	call run.bat
	goto :eof
)

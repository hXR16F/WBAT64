@echo off
:main
	if exist "1.txt" (
		del /f /q "1.txt" >nul
		del /f /q "stderr.txt" >nul
		del /f /q "stdout.txt" >nul
		goto :next
	)
	mode 80,25
	cls
	
	(echo.&echo  Press any key to display message...) & pause >nul
	
	echo  Current date: %date%> "wbat/date.txt"
	echo  Current time: %time%> "wbat/time.txt"
	
	WBAT64 "box (black on +white) :asd"

:next
	call "wbat/CLICKED.BAT"
	del /f /q "wbat/date.txt" >nul
	del /f /q "wbat/time.txt" >nul
	del /f /q "wbat/CLICKED.BAT" >nul
	cls
	(echo.&echo  Clicked button: '%WBAT%'&echo  Press any key to continue...)
	
	pause >nul
	goto :main
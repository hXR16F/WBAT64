@echo off
color 1f

:main
	cls
	(echo.&echo  Press any key to display message...) & pause >nul
	
	cls & cd wbat
		otvdm.exe --app-name WBAT.COM wbat cls ! box (black on white) :asd
	cd..
	
	cls
	(echo.&echo  Clicked button: '%ERRORLEVEL%' ^(not working^)&echo  Press any key to continue...)
	pause >nul
	
	goto :main
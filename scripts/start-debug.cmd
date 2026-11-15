@echo off
rem Startet das installierte Milo mit WebView2-Debug-Schnittstelle (Port 9223) fuer scripts/cdp.mjs.
taskkill /F /IM milo.exe >nul 2>&1
timeout /t 1 /nobreak >nul
set WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9223
start "" "%LOCALAPPDATA%\Milo\milo.exe"

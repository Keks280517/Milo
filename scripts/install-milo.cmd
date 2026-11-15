@echo off
rem Installiert die frisch gebaute Milo-Fassung still fuer den aktuellen Benutzer und startet sie.
rem Eine aeltere Fassung (auch noch unter dem alten Namen kubo.exe) wird vorher beendet und still
rem deinstalliert; Einstellungen, Notizen, Schluessel und Sprachmodelle bleiben erhalten und werden
rem beim ersten Start uebernommen.
rem Projektordner = eine Ebene ueber diesem Skript.
set ROOT=%~dp0..
set LOG="%ROOT%\src-tauri\target\install-result.txt"
echo %date% %time% Start > %LOG%
taskkill /F /IM milo.exe >nul 2>&1
taskkill /F /IM kubo.exe >nul 2>&1
timeout /t 1 /nobreak >nul
if exist "%LOCALAPPDATA%\Milo\uninstall.exe" (
  rem /UPDATE: sonst loescht der Deinstaller den Autostart-Eintrag „Milo“.
  "%LOCALAPPDATA%\Milo\uninstall.exe" /S /UPDATE
  rem Der Deinstaller laeuft im Hintergrund weiter: warten, bis das alte Programm weg ist.
  for /l %%i in (1,1,30) do (
    if not exist "%LOCALAPPDATA%\Milo\uninstall.exe" goto removed
    timeout /t 1 /nobreak >nul
  )
)
:removed
timeout /t 2 /nobreak >nul
echo %date% %time% alte Fassung entfernt >> %LOG%
for %%f in ("%ROOT%\src-tauri\target\release\bundle\nsis\Milo_*_x64-setup.exe") do set SETUP=%%f
"%SETUP%" /S
echo %date% %time% Milo installiert: %errorlevel% >> %LOG%
timeout /t 2 /nobreak >nul
start "" "%LOCALAPPDATA%\Milo\milo.exe"
echo %date% %time% gestartet >> %LOG%

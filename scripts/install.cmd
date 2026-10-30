@echo off
rem Canopy installer for people who'd rather double-click than open PowerShell.
rem Downloads install.ps1 from the latest release and runs it. Arguments are
rem passed through, for example:  install.cmd -Msi   or   install.cmd -Portable   or   install.cmd -Lite
rem (quote paths with single quotes:  install.cmd -Portable -Dir 'D:\Apps\Canopy')
setlocal
set "URL=https://github.com/Londopy/canopy/releases/latest/download/install.ps1"
powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "& ([scriptblock]::Create((Invoke-RestMethod -UseBasicParsing '%URL%'))) %*"
set "RC=%ERRORLEVEL%"
rem keep the window open when it was double-clicked (no arguments)
if "%~1"=="" pause
endlocal & exit /b %RC%

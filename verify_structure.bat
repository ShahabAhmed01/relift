@echo off
setlocal
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0verify_structure.ps1"
exit /b %ERRORLEVEL%

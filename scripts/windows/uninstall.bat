@echo off
setlocal enabledelayedexpansion

:: ============================================================
::  Lecoo Control Center - Uninstaller
:: ============================================================
set "INSTALL_DIR=%ProgramFiles%\LecooControlCenter"
set "SERVICE_NAME=LecooControlCenter"
set "DAEMON_EXE=lecoo-ec-daemon.exe"
:: ============================================================

net session >nul 2>&1
if !errorLevel! neq 0 (
    powershell -NoProfile -Command "try { Start-Process -FilePath cmd.exe -ArgumentList '/c \"\"%~f0\"\"' -Verb RunAs -Wait } catch { Write-Host 'UAC cancelled.'; pause }"
    exit /b
)

cd /d "%SystemRoot%"

echo.
echo ============================================================
echo   Lecoo Control Center - Uninstaller
echo ============================================================
echo   Install dir : %INSTALL_DIR%
echo   Service     : %SERVICE_NAME%
echo ============================================================
echo.

echo [1/4] Stopping service...
sc query "%SERVICE_NAME%" >nul 2>&1
if !errorLevel! neq 0 (
    echo       [OK] Service not found.
    echo.
    goto :kill_process
)

sc stop "%SERVICE_NAME%" >nul 2>&1

set "_w=0"
:wait_stop
sc query "%SERVICE_NAME%" 2>nul | find "STOPPED" >nul 2>&1
if !errorLevel! equ 0 goto :stopped
set /a _w+=1
if !_w! geq 15 (
    echo       [WARN] Timed out waiting for service to stop.
    goto :stopped
)
timeout /t 1 /nobreak >nul
goto :wait_stop

:stopped
echo       [OK] Service stopped.
echo       Deleting service...
sc delete "%SERVICE_NAME%" >nul 2>&1
if !errorLevel! neq 0 (
    echo       [WARN] Cannot delete service now. Will be removed after reboot.
) else (
    echo       [OK] Service deleted.
)
timeout /t 2 /nobreak >nul
echo.

:kill_process
echo [2/4] Checking for running process...
tasklist /FI "IMAGENAME eq %DAEMON_EXE%" 2>nul | find /I "%DAEMON_EXE%" >nul 2>&1
if !errorLevel! neq 0 (
    echo       [OK] No running process.
) else (
    echo       Killing %DAEMON_EXE%...
    taskkill /F /IM "%DAEMON_EXE%" >nul 2>&1
    timeout /t 2 /nobreak >nul
    echo       [OK] Process terminated.
)
echo.

echo [3/4] Removing from system PATH...
reg query "HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment" /v Path 2>nul | find /I "%INSTALL_DIR%" >nul 2>&1
if !errorLevel! neq 0 (
    echo       [OK] Not in PATH.
) else (
    powershell -NoProfile -Command "$dir='%INSTALL_DIR%'; $old=[Environment]::GetEnvironmentVariable('Path','Machine'); $new=($old -split ';' | Where-Object { $_ -and ($_.TrimEnd('\') -ine $dir.TrimEnd('\')) }) -join ';'; [Environment]::SetEnvironmentVariable('Path',$new,'Machine')" 2>nul
    if !errorLevel! equ 0 (
        echo       [OK] Removed from PATH.
    ) else (
        echo       [WARN] Could not update PATH. Remove manually.
    )
)
echo.

echo [4/4] Removing files...
if not exist "%INSTALL_DIR%" (
    echo       [OK] Directory not found.
    echo.
    goto :success
)

rmdir /S /Q "%INSTALL_DIR%" 2>nul
if exist "%INSTALL_DIR%" (
    echo       [WARN] Some files locked. Delete manually after reboot:
    echo              %INSTALL_DIR%
) else (
    echo       [OK] All files removed.
)
echo.

:success
echo ============================================================
echo   Uninstallation completed.
echo ============================================================
echo.
goto :done

:done
pause
endlocal
exit /b
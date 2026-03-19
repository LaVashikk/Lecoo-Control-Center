@echo off
setlocal enabledelayedexpansion

:: ============================================================
::  Lecoo Control Center - Installer
::  Edit ONLY this section to customize paths and names.
:: ============================================================
set "INSTALL_DIR=%ProgramFiles%\LecooControlCenter"
set "SERVICE_NAME=LecooControlDaemon"
set "SERVICE_DISPLAY=Lecoo EC Daemon"
set "SERVICE_DESC=Lecoo laptop EC hardware control daemon"
set "DAEMON_EXE=lecoo-ec-daemon.exe"
set "DAEMON_LIB=inpoutx64.dll"
set "CTRL_EXE=lecoo-ctrl.exe"
:: ============================================================

:: ---- Request admin rights -----------------------------------
net session >nul 2>&1
if !errorLevel! neq 0 (
    echo Requesting administrator privileges...
    powershell -NoProfile -Command ^
        "try { Start-Process -FilePath cmd.exe -ArgumentList '/c \"\"%~f0\"\"' -Verb RunAs -Wait } catch { Write-Host 'UAC cancelled or failed.'; pause }"
    exit /b
)

:: Safe working directory (never inside INSTALL_DIR)
cd /d "%SystemRoot%"

:: Source directory = where the .bat file lives
set "SRC=%~dp0"

echo.
echo ============================================================
echo   Lecoo Control Center - Installer
echo ============================================================
echo   Source  : %SRC%
echo   Target  : %INSTALL_DIR%
echo   Service : %SERVICE_NAME%
echo ============================================================
echo.

:: ---- Step 1: Verify source binaries -------------------------
echo [1/7] Verifying source binaries...
if not exist "%SRC%%DAEMON_EXE%" (
    echo       [FAIL] Not found: %SRC%%DAEMON_EXE%
    echo              Place this script next to the compiled binaries.
    goto :fail
)
if not exist "%SRC%%DAEMON_LIB%" (
    echo       [FAIL] Not found: %SRC%%DAEMON_LIB%
    echo              Place this script next to the compiled binaries.
    goto :fail
)
if not exist "%SRC%%CTRL_EXE%" (
    echo       [FAIL] Not found: %SRC%%CTRL_EXE%
    echo              Place this script next to the compiled binaries.
    goto :fail
)
echo       [OK] %DAEMON_EXE%
echo       [OK] %DAEMON_LIB%
echo       [OK] %CTRL_EXE%
echo.

:: ---- Step 2: Stop existing service if present ----------------
echo [2/7] Checking for existing service...
sc query "%SERVICE_NAME%" >nul 2>&1
if !errorLevel! neq 0 (
    echo       [OK] No existing service found. Fresh install.
    echo.
    goto :kill_process
)

echo       Service "%SERVICE_NAME%" exists. Stopping...
sc stop "%SERVICE_NAME%" >nul 2>&1

:: Poll up to 15 seconds for the service to stop
set "_w=0"
:wait_stop
sc query "%SERVICE_NAME%" 2>nul | find "STOPPED" >nul 2>&1
if !errorLevel! equ 0 goto :stopped
set /a _w+=1
if !_w! geq 15 (
    echo       [WARN] Service did not stop within 15 seconds.
    goto :stopped
)
timeout /t 1 /nobreak >nul
goto :wait_stop

:stopped
echo       Removing old service registration...
sc delete "%SERVICE_NAME%" >nul 2>&1
if !errorLevel! neq 0 (
    echo       [FAIL] Cannot delete service. It is marked for deletion.
    echo              Reboot the machine and run this installer again.
    goto :fail
)
echo       [OK] Old service removed.
:: Wait for SCM to fully release the name
timeout /t 3 /nobreak >nul
echo.

:: ---- Step 3: Kill lingering daemon process -------------------
:kill_process
echo [3/7] Checking for running daemon process...
tasklist /FI "IMAGENAME eq %DAEMON_EXE%" 2>nul | find /I "%DAEMON_EXE%" >nul 2>&1
if !errorLevel! neq 0 (
    echo       [OK] No running process.
) else (
    echo       Killing %DAEMON_EXE%...
    taskkill /F /IM "%DAEMON_EXE%" >nul 2>&1
    if !errorLevel! neq 0 (
        echo       [FAIL] Cannot kill %DAEMON_EXE%.
        echo              Close it manually or reboot, then try again.
        goto :fail
    )
    timeout /t 2 /nobreak >nul
    echo       [OK] Process terminated.
)
echo.

:: ---- Step 4: Create install directory ------------------------
echo [4/7] Creating install directory...
if not exist "%INSTALL_DIR%" (
    mkdir "%INSTALL_DIR%"
    if not exist "%INSTALL_DIR%" (
        echo       [FAIL] Cannot create: %INSTALL_DIR%
        goto :fail
    )
)
echo       [OK] %INSTALL_DIR%
echo.

:: ---- Step 5: Copy binaries ----------------------------------
echo [5/7] Copying binaries...
copy /Y "%SRC%%DAEMON_EXE%" "%INSTALL_DIR%\%DAEMON_EXE%" >nul
if !errorLevel! neq 0 (
    echo       [FAIL] Cannot copy %DAEMON_EXE%
    echo              File may be locked. Kill all related processes and retry.
    goto :fail
)
echo       [OK] %DAEMON_EXE%

copy /Y "%SRC%%DAEMON_LIB%" "%INSTALL_DIR%\%DAEMON_LIB%" >nul
if !errorLevel! neq 0 (
    echo       [FAIL] Cannot copy %DAEMON_LIB%
    echo              File may be locked. Kill all related processes and retry.
    goto :fail
)
echo       [OK] %DAEMON_LIB%

copy /Y "%SRC%%CTRL_EXE%" "%INSTALL_DIR%\%CTRL_EXE%" >nul
if !errorLevel! neq 0 (
    echo       [FAIL] Cannot copy %CTRL_EXE%
    goto :fail
)
echo       [OK] %CTRL_EXE%
echo.

:: ---- Step 6: Register and start service ----------------------
echo [6/7] Registering Windows service...
sc create "%SERVICE_NAME%" ^
    binPath= "\"%INSTALL_DIR%\%DAEMON_EXE%\" --service" ^
    start= auto ^
    DisplayName= "%SERVICE_DISPLAY%" >nul 2>&1
if !errorLevel! neq 0 (
    echo       [FAIL] sc create failed.
    echo              The service name may still be pending deletion.
    echo              Reboot and retry.
    goto :fail
)
sc description "%SERVICE_NAME%" "%SERVICE_DESC%" >nul 2>&1
echo       [OK] Service registered.

echo       Starting service...
sc start "%SERVICE_NAME%" >nul 2>&1
if !errorLevel! neq 0 (
    echo       [WARN] Service failed to start.
    echo.
    echo       --- Diagnostic info ---
    sc query "%SERVICE_NAME%"
    echo.
    echo       Common causes:
    echo         - Error 1053: the binary did not call StartServiceCtrlDispatcher
    echo           in time. Verify %DAEMON_EXE% implements Windows Service API.
    echo         - Error 1067: the binary crashed on startup. Run it manually:
    echo              "%INSTALL_DIR%\%DAEMON_EXE%" --service
    echo           and check stderr output.
    echo         - Missing DLLs: run the exe manually from a cmd window.
    echo       -----------------------
) else (
    echo       [OK] Service started successfully.
)
echo.

:: ---- Step 7: Add to system PATH -----------------------------
echo [7/7] Adding install directory to system PATH...
echo "%PATH%" | find /I "%INSTALL_DIR%" >nul 2>&1
if !errorLevel! equ 0 (
    echo       [OK] Already in PATH.
) else (
    powershell -NoProfile -Command ^
        "$old = [Environment]::GetEnvironmentVariable('Path','Machine');" ^
        "$dir = '%INSTALL_DIR%';" ^
        "if ($old -split ';' | Where-Object { $_.TrimEnd('\') -ieq $dir.TrimEnd('\') }) { exit 0 }" ^
        "[Environment]::SetEnvironmentVariable('Path', $old.TrimEnd(';') + ';' + $dir, 'Machine')" 2>nul
    if !errorLevel! equ 0 (
        echo       [OK] Added to system PATH.
        echo       [NOTE] Open a NEW terminal for PATH changes to take effect.
    ) else (
        echo       [WARN] Could not update PATH. Add manually:
        echo              %INSTALL_DIR%
    )
)
echo.

echo ============================================================
echo   Installation completed.
echo ============================================================
echo   Service  : %SERVICE_NAME%
echo   Location : %INSTALL_DIR%
echo   CLI tool : %CTRL_EXE% (open a new terminal)
echo.
echo   Useful commands:
echo     sc query %SERVICE_NAME%
echo     sc stop  %SERVICE_NAME%
echo     sc start %SERVICE_NAME%
echo ============================================================
echo.
goto :done

:fail
echo.
echo ============================================================
echo   INSTALLATION FAILED - review the errors above.
echo ============================================================
echo.

:done
pause
endlocal
exit /b

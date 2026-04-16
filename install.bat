@echo off
setlocal enabledelayedexpansion

echo Installing kiro-cli-auth for Windows...
echo.

REM Check for admin privileges
net session >nul 2>&1
if %errorLevel% neq 0 (
    echo ERROR: This script requires Administrator privileges.
    echo Please run as Administrator.
    pause
    exit /b 1
)

REM Check if binary exists
if not exist "target\release\kiro-cli-auth.exe" (
    echo ERROR: Binary not found. Please build first:
    echo    cargo build --release
    pause
    exit /b 1
)

REM Install to Program Files
set "INSTALL_DIR=%ProgramFiles%\kiro-cli-auth"
echo Installing to: %INSTALL_DIR%
if not exist "%INSTALL_DIR%" mkdir "%INSTALL_DIR%"

copy /Y "target\release\kiro-cli-auth.exe" "%INSTALL_DIR%\kiro-cli-auth.exe"
if %errorLevel% neq 0 (
    echo ERROR: Failed to copy binary
    pause
    exit /b 1
)

REM Add to PATH
echo Adding to system PATH...
setx /M PATH "%PATH%;%INSTALL_DIR%" >nul 2>&1

echo.
echo Installation successful!
echo Location: %INSTALL_DIR%\kiro-cli-auth.exe
echo.
echo Please restart your terminal to use: kiro-cli-auth
echo.
pause

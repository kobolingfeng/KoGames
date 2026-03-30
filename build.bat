@echo off
chcp 65001 >nul
echo ========================================
echo   KoGames - Build Installer
echo ========================================
echo.
echo Building release...
echo.

npm run tauri:build

if %ERRORLEVEL% neq 0 (
    echo.
    echo Build failed!
    pause
    exit /b 1
)

echo.
echo Copying installer to release folder...

if not exist "release" mkdir "release"

REM Copy NSIS installer
for %%f in (src-tauri\target\release\bundle\nsis\*.exe) do (
    copy /Y "%%f" "release\"
    echo   Copied: %%~nxf
)

REM Copy MSI installer if exists
for %%f in (src-tauri\target\release\bundle\msi\*.msi) do (
    copy /Y "%%f" "release\"
    echo   Copied: %%~nxf
)

echo.
echo ========================================
echo   Build complete! Check release folder.
echo ========================================
echo.
pause

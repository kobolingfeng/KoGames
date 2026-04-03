@echo off
chcp 65001 >nul
echo ========================================
echo   KoGames - Development Mode
echo ========================================
echo.
echo Starting hot-reload dev server...
echo.
npm run dev
pause

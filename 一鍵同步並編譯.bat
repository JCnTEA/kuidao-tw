@echo off
chcp 65001 >nul
title 鬼島 · 一鍵同步上游並編譯
cd /d "%~dp0"

echo.
echo  [1/4] 檢查並拉取上游…
python tools\sync_upstream.py
set SYNC=%ERRORLEVEL%
if "%SYNC%"=="2" (
    echo.
    echo  已是最新版本，仍要重新編譯嗎？
    choice /c YN /m "重新編譯"
    if errorlevel 2 goto :done
)
if not "%SYNC%"=="0" if not "%SYNC%"=="2" (
    echo  同步失敗，請確認網路與 Python / OpenCC 是否安裝。
    pause & exit /b 1
)

echo.
echo  [2/4] 編譯 release…
cargo build --release || (echo  編譯失敗 & pause & exit /b 1)

echo.
echo  [3/4] 收集產物…
for /f "tokens=2 delims== " %%v in ('findstr /b "version = " Cargo.toml') do set VER=%%~v
set VER=%VER:"=%
if not exist dist mkdir dist
copy /y target\release\kuidao.exe "dist\鬼島 Kuidao %VER% 台灣版.exe" >nul
if exist assets\kuidao_dev.ico (
   echo  提示：需要開發板版本時，把 assets\kuidao.ico 換成 kuidao_dev.ico 後重編。
)

echo.
echo  [4/4] 完成
echo  版本：%VER%
echo  產物：dist\鬼島 Kuidao %VER% 台灣版.exe
echo.

:done
pause

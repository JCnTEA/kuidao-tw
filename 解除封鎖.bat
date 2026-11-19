@echo off
chcp 65001 >nul
title 鬼島 · 解除 Windows SmartScreen 封鎖
echo.
echo  正在解除本資料夾內所有 exe 的「來自網路」標記…
echo.
for %%f in ("%~dp0*.exe") do (
    powershell -NoProfile -Command "Unblock-File -LiteralPath '%%~ff' -ErrorAction SilentlyContinue; Remove-Item -LiteralPath '%%~ff:Zone.Identifier' -ErrorAction SilentlyContinue" >nul 2>&1
    echo   [已完成] %%~nxf
)
echo.
echo  完成。請重新執行 exe。
echo.
echo  若仍跳出「Windows 已保護您的電腦」，請點「更多資訊」→「仍要執行」。
echo  這是因為程式沒有購買程式碼簽章憑證，Windows 對所有無簽章的自製程式都會提示一次。
echo.
pause

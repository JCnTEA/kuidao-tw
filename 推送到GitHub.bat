@echo off
chcp 65001 >nul
cd /d "%~dp0"
echo.
echo 使用方式：先在 GitHub 建一個空倉庫（不要勾選 README），
echo 再把下面這行網址換成你自己的，例如：
echo    https://github.com/你的帳號/kuidao-tw.git
echo.
set /p URL=貼上你的倉庫網址：
git remote remove origin >nul 2>nul
git remote add origin %URL%
git branch -M main
git push -u origin main
echo.
echo 完成。之後改動：git add -A ^&^& git commit -m "更新" ^&^& git push
pause

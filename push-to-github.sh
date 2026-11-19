#!/usr/bin/env bash
# 用法：./push-to-github.sh https://github.com/<你的帳號>/kuidao-tw.git
set -e
cd "$(dirname "$0")"
URL="${1:?請帶入倉庫網址}"
git remote remove origin 2>/dev/null || true
git remote add origin "$URL"
git branch -M main
git push -u origin main

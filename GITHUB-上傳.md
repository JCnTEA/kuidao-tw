# 推到 GitHub

## 一、先建倉庫
GitHub → New repository → 名稱建議 `kuidao-tw` → **不要**勾選 Add a README（本專案已有）。

## 二、兩種推送方式

### A. 這台機器直接推（最快）
```bash
./push-to-github.sh https://github.com/<你的帳號>/kuidao-tw.git
```

### B. Windows 上推
解壓本包，雙擊 `推送到GitHub.bat`，貼上倉庫網址即可。

## 三、推上去之後
- `docs/index.html` 是官網；Settings → Pages → Source 選 `main` / `docs`，即可有下載頁。
- `.github/workflows/auto-update.yml` 會定時同步上游並發 Release；建議先在 Actions 頁手動 Run 一次確認能編。
- 官網下載鈕請把 `var REPO = "https://github.com/";` 改成你的倉庫網址。

## 四、目前狀態
- 已建立 git 倉庫並完成第一次提交（537 個檔案）。
- 尚未推送：這台機器的 GitHub 權杖無效（401），需要你的帳號授權。

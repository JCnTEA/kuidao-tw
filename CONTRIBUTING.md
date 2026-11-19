# 參與開發

歡迎回報相容性問題、修正 bug 或補強使用說明。送出較大的功能前，先開 Issue 說明情境與預期行為。

## 本機開發

安裝 Rust stable、Visual Studio C++ 建置工具與 Windows SDK，複製倉庫後執行：

```powershell
cargo build --locked --bin kuidao
cargo test --locked --all-targets --features record -- --test-threads=1
```

為開發實例指定獨立目錄，避免蓋掉日常使用的資料：

```powershell
$env:KUIDAO_DATA_DIR = Join-Path $env:TEMP 'kuidao-development'
cargo run --locked --bin kuidao
```

一般測試使用合成資料。標記 `#[ignore]` 的真實帳號測試或截圖匯出測試須自行明確執行；不要在公開 CI 連私人帳號。

## 程式碼導覽

| 目錄／檔案 | 內容 |
| --- | --- |
| `src/main.rs` | Windows 外殼、系統狀態與背景服務銜接 |
| `src/app.rs`、`src/pages.rs` | 狀態與主介面 |
| `src/calendar.rs`、`src/calendar_ui.rs` | CalDAV、重複日程、月曆與清單 |
| `src/mail.rs`、`src/mail_reader.rs` | IMAP、MIME、驗證碼與郵件閱讀 |
| `src/quota*.rs` 與 provider 模組 | AI 額度與連線 |
| `src/native.rs`、`src/system.rs` | 原生視窗、輸入、拖放與系統事件 |
| `src/ui_tests.rs` | 無視窗互動與版面回歸測試 |
| `src/bin/kui-record` | 合成資料演示與軟體渲染器 |
| `src/mascot.rs` | 吉祥物形象（島虎、柴犬等） |
| `assets/pet-species/` | 各形象的逐幀表情序列 |
| `tools/sync_upstream.py` | 上游同步與台灣在地化修補 |
| `vendor/` | 少量必要的上游補丁，見相依說明 |
| `installer/`、`scripts/` | 安裝、卸載與打包 |

## 提交要求

修 bug 時附可重現的測試；介面變更附不含私人資訊的截圖。保持改動聚焦，不把個人設定、授權碼、Cookie、編譯產物或真實帳號資料提交進倉庫。

原始碼貢獻依倉庫 MIT 授權提交；第三方素材須同時附上正確來源與授權聲明。不要修改或移除 vendored 相依套件的原始著作權聲明。

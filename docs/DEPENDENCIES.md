# 依賴與第三方許可

鬼島原創代碼採用 MIT。第三方依賴、字體、圖標及品牌標識遵循各自許可；MIT 不授予第三方商標權。完整歸屬及許可文本見 [THIRD_PARTY_NOTICES.txt](../THIRD_PARTY_NOTICES.txt)。

## 本地補丁

- `vendor/eframe`：基於 0.31.1，處理 Windows WGL 上下文複用、島體縮放和窗口焦點。
- `vendor/egui`：基於 0.31.1，為不同縮放倍率維護獨立的字體紋理。
- `vendor/imap`：基於 2.4.1，關閉 `nom` 預設功能並保留 `std`，移除郵件協議解析不需要的 `lexical-core` 浮點解析依賴。伺服器拒絕進入 IDLE 時直接回到輪詢。
- `vendor/ureq`：基於 2.12.1。HTTP 代理的明文請求保持 absolute-form，HTTPS 的 CONNECT 隧道內改用 origin-form。原 MIT 與 Apache-2.0 許可證隨源碼保留。

- `vendor/imap-proto`：兼容騰訊企業信箱 CAPABILITY 響應末尾的空格，保留 IMAP4rev1 校驗和下一條響應邊界。

## 安全檢查

2026 年 11 月 3 日發佈前，對 Cargo.lock 的 registry 依賴查詢了 OSV 公告庫，並運行完整測試。已移除受 [RUSTSEC-2023-0086](https://rustsec.org/advisories/RUSTSEC-2023-0086.html) 影響的 `lexical-core 0.7.6`。

當前仍有兩項上游停止維護提示：

| 依賴 | 公告 | 當前處理 |
| --- | --- | --- |
| paste 1.0.15 | [RUSTSEC-2024-0436](https://rustsec.org/advisories/RUSTSEC-2024-0436.html) | 隨現有 UI 依賴使用，後續跟隨上游升級 |
| ttf-parser 0.25.1 | [RUSTSEC-2026-0192](https://rustsec.org/advisories/RUSTSEC-2026-0192.html) | 隨字體棧使用，後續評估維護中的替代版本 |

這兩項屬於維護狀態提示，並非本次檢查確認的可利用漏洞。公告庫檢查不能代替完整安全審計。`imap-proto` 還會產生 Rust 未來兼容性警告，當前編譯和測試通過。
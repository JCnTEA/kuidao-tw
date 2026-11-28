# 自繪畫布與首批官方插件：任務說明

插件 SDK 下一步開放「自繪畫布」。插件不再只能從固定組件裡挑，而是用一份 JSON 描述要畫的圖形，島仔負責繪製、縮放、主題、幀率和安全。首批五個官方插件全部用畫布實現，同時作為社區作者的樣例。

設計稿在 [ui/](ui/) 目錄，用瀏覽器直接打開：

- `00-開放方案.html`：畫布方案總覽。
- `P1`–`P5`：五個插件。每張稿左邊是展開頁（放大 1.5 倍顯示），下面是常駐條和其他狀態，右邊是設計說明和實現方式。

在線版是畫布「島仔官方插件 UI 稿」的「理想版」頁，兩者內容一致；有出入時以小唐的最新說明為準。

## 原則

- 畫布是數據，不是代碼。插件每次刷新返回一份圖形描述，不能每幀運行腳本。
- 會動的內容（時間、跳動的金額、倒計時、指針）用「綁定」交給島仔按秒或按分鐘計算。
- 標準組件全部保留。畫布可以作為狀態頁裡的一個組件，也可以佔滿整頁。
- 不強制打碼。隱私交給使用者，使用者可以用島仔已有的「截圖和錄屏中隱藏」等設定。
- 字體預設用島仔自帶的。使用者安裝字體包後，畫布可以按名字引用這個字體，缺字時回退到島仔字體。
- 動畫不能讓島一直重繪。靜止時不出幀；有動畫時，兼容渲染下要按交互幀交付，參考 `src/main.rs` 裡 `page_animating` 的處理。

## 閱讀順序

1. 本文：原則、分支和交付要求。
2. [WORK-PLAN.md](WORK-PLAN.md)：按宿主後端、宿主前端、投稿校驗、插件邏輯、插件畫面、數據分層的分工和四周排期。**五個插件從第一天起同時開工。**
3. 自己負責的任務說明，以及 `ui/` 裡對應的設計稿。

## 任務

| 任務 | 倉庫 | 分支 | 什麼時候開工 |
| --- | --- | --- | --- |
| [A：畫布核心](A-canvas-core.md) | 島仔 | `feat/plugin-canvas` | 立即 |
| [B：實時綁定、動畫與交互](B-canvas-live.md) | 島仔 | `feat/plugin-canvas-live` | A 之後 |
| [C：常駐小畫布、字體包與設定補項](C-canvas-extras.md) | 島仔 | `feat/plugin-canvas-extras` | A 之後，與 B 並行 |
| [P0：投稿校驗支援新插件](P0-submission.md) | 插件倉 | `feat/submission-status-canvas` | 立即 |
| [P1：便籤本](P1-notes.md) | 插件倉 | `feat/official-notes` | 邏輯層立即；畫面層 A 之後 |
| [P2：現貨金價](P2-gold.md) | 插件倉 | `feat/official-gold` | 同上 |
| [P3：實時工資](P3-live-pay.md) | 插件倉 | `feat/official-live-pay` | 同上 |
| [P4：世界時鐘](P4-world-clock.md) | 插件倉 | `feat/official-world-clock` | 同上 |
| [P5：日子](P5-days.md) | 插件倉 | `feat/official-days` | 同上 |

## 分支與工作樹

- 島仔任務從本地 `main` 開分支。工作樹放在 `D:\A-TangHao\REA\Windows小工具\01-專項工程\<功能中文名>`，例如 `插件自繪畫布`。
- B 和 C 從 A 的分支開出。
- 插件任務在插件倉 `D:\A-TangHao\REA\Windows小工具\01-專項工程\島仔插件倉` 開分支，工作樹放在同級目錄，例如 `官方插件-便籤本`。
- 編譯和測試按倉庫根目錄 AGENTS.md 的「本機編譯」規則排隊執行。命令統一用 `spike/cargo-serial.ps1`。
- 做完後提交到自己的分支，不合進 `main` 或 `test/acceptance`，也不推送。由小唐轉交驗收。

## 交付時附上

1. 分支名和最後一個提交號。
2. 下面這條命令的結果，以及任務裡另列的定向測試結果：

   ```powershell
   powershell -NoProfile -ExecutionPolicy Bypass -File spike/cargo-serial.ps1 test --locked --lib --features "sdk-js sdk-material" --target x86_64-pc-windows-msvc sdk_spike
   ```

3. 一張與設計稿同尺寸的截圖（插件任務必附）。
4. 沒做完或做法與說明不同的地方，逐條寫明原因。

## 共用約定

- 座標單位是島的邏輯像素。展開面板寬 419，內容區寬 387（左右各留 16），原點在內容區左上角，標題行以下。
- 顏色可以寫 `#RRGGBB`、`#RRGGBBAA`，也可以寫主題令牌：`$accent`（插件強調色）、`$text`、`$muted`、`$dim`、`$card`、`$track`、`$bg`。換皮膚時令牌自動跟隨。
- 時間統一用 Unix 秒。時區用 IANA 名稱，例如 `Asia/Shanghai`。
- 文檔寫進 `docs/plugins/sdk-v1-design.md` 的新小節「自繪畫布」，字段寫進 `docs/plugins/sdk-v1.schema.json`，並在 `spike/examples/` 各加一個樣例。

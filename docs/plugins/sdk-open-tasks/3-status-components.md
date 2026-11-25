# 第 3 步：狀態頁組件、實時組件與主題令牌

本步擴充狀態頁組件，並加入由島仔逐幀計算的實時組件、敏感值隱藏和受控的主題令牌。分支為 `feat/plugin-status-components`，共同規則見 [README](README.md)。

## 先讀

- `src/plugin_status.rs`：`PluginStatus`、`StatusLayout`、`StatusComponent`、`StatusStyle`。
  - 已有組件：state、summary、progress（環或條）、metrics、alert、updated_at。
  - 已有樣式：accent、density、surface。
- `src/plugin_status_ui.rs`：繪製。
- `spike/status_source_tests.rs`。
- `docs/plugins/sdk-v1-design.md` 的狀態頁說明。

## 要做的

### 普通組件

每種組件都要做三件事：在 `PluginStatus` 里加一個數據字段，在 `StatusComponent` 里加一種類型，在 `valid()` 裡設上限。

- **`list`**：`items` 最多 8 項，每項為 `{label, detail?, state?, action?}`。`state` 複用 `StatusState`，用顏色點表示。`action` 歸第 1 步實現，這裡只要能解析並保留下來。
- **`chart`**：`series` 為 2 到 48 個有限數字，畫成迷你折線。宿主負責畫面積填充、淡網格和末點強調。可帶 `series_label`（最多 12 字）。
- **`text`**：`note`，最多 300 字的多行說明。
- **`badge`**：`tags` 最多 4 個短標籤，每個最多 8 字。

### 實時組件

實時組件由島仔在本地逐幀計算，腳本不用每秒運行。腳本只返回下面的參數，宿主按當前時間算出要顯示的值。插件停用或頁面不可見時停止重繪。

- **`counter`**：跳動的數字。參數為 `{start_value, per_second, start_at, until?, decimals(0–4), prefix?, suffix?}`。顯示值為 `start_value + per_second × (now − start_at)`，超過 `until` 後停在終值。實時工資就用它。
- **`clock`**：某個時區的當前時間。參數為 `{label, utc_offset_minutes, format:"HH:mm" | "HH:mm:ss"}`。時區偏移由第 5 步的宿主時區換算提供，腳本也可以自己算。世界時鐘用它。
- **`countdown`**：倒數到某個時間點。參數為 `{label, target_at, format:"d天 HH:mm" | "HH:mm:ss"}`，到點後顯示 `done_label`。距離下班、倒計時都用它。

三種實時組件各自最多出現 4 個，這樣世界時鐘可以列 4 個城市。

刷新頻率：顯示到秒的每秒重繪一次，顯示到分鐘的每分鐘重繪一次。不能為了它們讓整個島每幀重繪，要用 `request_repaint_after` 精確安排下一次刷新。

### 敏感值

- `metrics`、`counter` 和 `list` 的項可以標 `sensitive: true`。
- 宿主默認把敏感值顯示為打碼樣式，用戶點一下才露出，30 秒後自動再蓋上。
- 系統鎖屏、錄屏或截圖排除開啟時，敏感值始終打碼。截圖排除功能本體已有，見 `src/native_capture.rs`。
- 實時工資的金額必須標成敏感。

### 主題令牌

- `StatusStyle` 新增 `emphasis`，取值 `normal` 或 `strong`。
- `StatusStyle` 新增 `icon`，從宿主內置圖標裡挑一小組名字開放，例如 printer、battery、cloud、cup、clock、chart、coin、calendar。宿主沒有對應圖標的名字就不開放。
- 繪製前，按當前皮膚的卡片底色自動校正 accent，保證對比度至少 3:1。
- 字體、圓角、間距、動效仍由宿主決定，不開放給插件。

### 佈局與密度

- 佈局上限從 6 個組件放寬到 10 個。實時組件按出現次數計入，其他組件仍然每種只能出現一次。
- 緊湊密度下，由宿主決定收起哪些組件，或只顯示前幾項。

### 不要做的

按鈕和輸入框不做，它們屬於第 1 步，那邊會在同一個枚舉里加 `actions` 和 `fields`。這邊的改動保持枚舉容易合併。

### 樣例與文檔

- 在 `spike/examples/` 加一個「實時工資」樣例，封面可以複製 payday 的。
  - 第 5 步的設置類型還沒合進來時，先用文本設置項代替，並在清單裡註釋說明。
  - 要用到 `counter`、`countdown` 和敏感值。
- 在 `spike/examples/` 再加一個「世界時鐘」樣例，用 4 個 `clock`。
- 更新 `docs/plugins/sdk-v1.schema.json` 的 `statusLayout` 部分。

## 驗收標準

**單元測試**至少覆蓋：

- 新字段的上限和非法值；
- 佈局上限；
- 實時組件在 `until` 和 `target_at` 前後的取值；
- 跨午夜的時區換算；
- 敏感值默認打碼；
- accent 對比度校正。

**egui 繪製測試**：同一份狀態在以下條件下渲染，文字不越出卡片，也不 panic。

- 不同皮膚，深淺兩種主題；
- 標準和緊湊兩種密度；
- 100% 和 200% 縮放。

另外要確認：只有秒級實時組件時，重繪間隔約為 1 秒，不是每幀都重繪。

**兼容性**：舊的 statusLayout 插件包不用改就能通過。

**測試命令**：下面這條全部通過，再把過濾詞換成 `sdk_spike` 跑一遍，也全部通過。

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File spike/cargo-serial.ps1 test --locked --lib --features "sdk-js sdk-material" --target x86_64-pc-windows-msvc plugin_status
```

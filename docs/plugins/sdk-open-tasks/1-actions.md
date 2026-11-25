# 第 1 步：按鈕、表單、事件與提醒

目標是讓腳本插件能交互，包括響應點擊、接收專注結束事件、發一次性提醒，以及打開鏈接和複製文字這兩個宿主動作。分支為 `feat/plugin-actions`，共同規則見 [README](README.md)。

## 先讀

- `spike/capabilities.rs`：能力表。
- `spike/local.rs`：重點看 `Manifest`、`Output`、`render_input`、`tick_features`。
- `src/plugin_status.rs`、`src/plugin_status_ui.rs`：狀態頁的數據和繪製。
- `src/plugin_notice.rs`：現有的通知、未讀和點擊回執。
- `docs/plugins/sdk-v1-design.md`。

## 要做的

### 按鈕與表單（能力 `action`）

**前提和輸出字段**

- 聲明 `action` 時必須同時聲明 `status.page`。
- 狀態頁輸出 `status` 增加兩個字段：
  - `actions`：最多 3 個 `{id, label}`，label 最多 6 個字。
  - `fields`：最多 3 個單行文本框 `{key, label, value?, max}`，max 不超過 200。
- `StatusLayout` 增加 `actions` 和 `fields` 兩種組件，先用宿主現有控件繪製。
- 第 3 步會新增 `list` 組件。它的每一項可以帶一個 `action` 字段：用戶點這一項，宿主就按這個 id 觸發調用，用來勾掉或刪除便籤。第 3 步的分支還沒合進來時，在你這邊先把 `items[].action` 的數據和點擊路徑做好，組件的繪製由驗收方合併時接上。

**用戶點擊後的調用**

- 宿主在後台調用腳本全局函數 `action(input)`，input 為 `{settings, storage, today, source?, action:{id, form:{key:value}}}`。
- 限制和 refresh 一樣：30 ms、4 MiB、輸出 16 KiB。返回值格式也和 refresh 相同，結果走現有的暫存和替換流程。
- 每個插件同一時間只跑一次調用。
- 插件停用、設置改動或模塊關閉後，遲到的結果一律丟棄，複用現有的 cancelled 機制。
- 按鈕和列表項只對產生它們的那次輸出有效。輸出更新後，舊的點擊作廢。

### 宿主動作（不運行腳本）

按鈕或列表項除了 `id`，也可以帶 `open` 或 `copy`，二者只能選一個。這兩種都由宿主直接執行，不調用腳本。

- `open`：一個 https 鏈接。主機必須在清單聲明的 `network.hosts` 裡（第 2 步定義的字段），沒聲明這個字段的插件不能用 `open`。宿主用系統瀏覽器打開，打開前不彈確認。
- `copy`：最多 2000 字的文字，宿主把它寫進剪貼板。

### 事件（能力 `event`）

- 清單新增可選字段 `events`，目前只接受 `"focus.finished"`。
- 在 `src/` 裡找到專注計時結束的位置，把事件交給插件宿主。
- 宿主調用 `event(input)`，input 帶 `event:{name, id, at, minutes}`。
- 同一事件對每個插件只投遞一次。
- 插件停用或所屬模塊關閉期間，事件直接丟棄，不排隊。

### 提醒（能力 `notice`）

- 輸出可以帶 `notice:{id, title, body?}`，title 最多 40 字，body 最多 120 字。
- 優先複用 `src/plugin_notice.rs` 的通知和未讀機制，用島上現有的提醒來呈現，不要另造一套。
- 同一 id 只提醒一次，每個插件 60 秒內最多一條。
- 遵守提醒總開關和「顯示通知正文」隱私開關。

### 其他

- 每項能力在 `CAPABILITIES` 里加一行，需要時擴展 `Role`，組合規則寫進 `check()`。
- 在 `spike/examples/` 加一個「便籤」樣例，封面可以複製 payday 的。樣例要做到：
  - 輸入框加「添加」按鈕；
  - 列表展示最近 8 條，點某一條就刪除它；
  - 每條帶「複製」；
  - 內容存在 storage 裡；
  - 專注結束時自動記一條「完成一次專注」；
  - 便籤滿 8 條時發一次提醒。

## 驗收標準

**單元測試**至少覆蓋：

- action 往返，以及 storage 寫回；
- 停用插件或輸出更新後，舊點擊作廢；
- 列表項的 action；
- 不在 hosts 裡的 `open` 被拒絕；
- `copy` 的長度上限；
- 同一事件只投遞一次；
- 提醒去重和限頻；
- 清單組合的非法情況。

**egui 界面測試**：點狀態頁按鈕後輸出會更新，寫法參考 `spike/local_management_test.rs`。

**測試命令**：下面這條全部通過，和改到的 `src` 模塊相關的定向測試也全部通過。

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File spike/cargo-serial.ps1 test --locked --lib --features "sdk-js sdk-material" --target x86_64-pc-windows-msvc sdk_spike
```

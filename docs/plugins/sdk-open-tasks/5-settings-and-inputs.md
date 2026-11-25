# 第 5 步：設置類型、時間輸入、數據文件與調度

這一步補齊腳本插件最常用的四類基礎能力：

- 更多設置類型；
- 當前時間和時區輸入；
- 包內只讀數據文件；
- 按時間點調度刷新。

另外把私有存儲從輸出裡拆出來，單獨放寬上限。分支為 `feat/plugin-settings-inputs`，共同規則見 [README](README.md)。

## 先讀

- `spike/local.rs`：`Field`、`valid_field`、`settings_values`、`render_input`、`tick_features`，以及 `Package::read` 讀包的部分。
- `spike/local_ui.rs`：插件設置的繪製，入口在 `settings()`。
- `src/extras.rs`：宿主的設置控件，包括開關、分段、數字和文本行。
- `docs/plugins/sdk-v1-design.md` 的設置說明，其中那一行寫著「腳本插件的開關、分段、數字 · 待宿主實現」。

## 要做的

### 設置類型

以下類型都複用宿主現有控件，放在所屬設置頁，標題旁帶「插件」小標。`default` 一律用字符串保存，保持和現有格式一致。

| type | 額外字段 | 保存格式 | 控件 |
| --- | --- | --- | --- |
| `switch` | 無 | `"true"` 或 `"false"` | 紫色開關 |
| `choice` | `choices:[{value,label}]`，2 到 6 項 | 選中的 value | 分段（不超過 4 項）或下拉 |
| `number` | `min`、`max`、`step`、`unit?`、`precision?` | 數字字符串 | 數字輸入加單位 |
| `time` | 無 | `"HH:MM"`，24 小時制 | 時間輸入 |
| `weekdays` | 無 | 由 `1` 到 `7` 組成、用逗號分隔，比如 `"1,2,3,4,5"` | 7 個可選小圓 |
| `list` | `item:[...]`，內含 1 到 3 個子字段（只能是 text、choice、number、date、time），`maxItems` 不超過 8 | JSON 數組字符串 | 可增刪排序的行 |

補充說明：

- 材質插件的 `number` 目前單獨校驗。改完後兩邊共用一套校驗，但材質原有的限制不能放寬。
- `list` 用於世界時鐘的城市列表、多個倒計時、多隻自選品種這類需求，這樣用戶不用把同一個插件裝好幾份。

### 時間與時區輸入

腳本 input 新增以下字段：

- `now`：Unix 毫秒時間戳。
- `tz`：`{offset_minutes, name}`，表示本機時區。
- `zones`：清單聲明瞭 `timezones:true` 時才提供。宿主為設置裡出現的 IANA 時區名，算出當前的 `offset_minutes`，並標出是否處於夏令時。Rust 側可以用 `chrono-tz`，但先檢查 `vendor/` 裡有沒有；沒有的話寫清楚引入它的影響，或者內置一張常用城市表。

原有的 `today` 保留不動。

### 包內只讀數據文件

- 清單新增 `data:["holidays-2026.json", ...]`。
- 每個文件必須是根目錄下的 UTF-8 JSON，單個不超過 64 KiB，合計不超過 256 KiB。
- 安裝時連同快照一起保存，腳本通過 `input.data["holidays-2026.json"]` 拿到解析好的對象。
- 不能包含鏈接或越出包目錄的路徑。打包命令 `--plugin-pack` 和測試集市 ZIP 都要帶上這些文件。

### 按時間點調度

- 輸出可以帶 `next_refresh_at`，單位是 Unix 秒。
- 宿主在這個時間點和正常週期之間取較早的那個，作為下一次調用的時間，但離現在至少 60 秒。
- 用途：實時工資在上班、下班時刷新；倒計時在零點刷新；節假日插件在假期開始時刷新。

### 私有存儲放寬

- `storage` 從 16 KiB 的輸出上限裡拆出來，單獨限制為 256 KiB。
- 寫滿時返回明確的錯誤，並在插件詳情裡提示。

### 樣例

- 在 `spike/examples/` 加「通用倒計時」，用到 `choice`、`switch`、`date`、`next_refresh_at`、待辦卡片和 `calendar.marker`。
- 再加「節假日和調休」，自帶 2026 年數據文件，用到 `data`、`calendar.marker`、待辦卡片。
- 2027 年的數據由作者每年發新版本更新，在文檔裡寫明。

## 驗收標準

- 單元測試至少覆蓋：
  - 每種設置類型的合法值、非法值和默認值；
  - `list` 的上限；
  - 舊插件包的設置照常讀取；
  - `now` 和 `tz` 的輸入；
  - `zones` 的偏移（拿一個有夏令時的地區測）；
  - 數據文件的大小限制、路徑限制和打包；
  - `next_refresh_at` 的取值（太近、太遠、已經過去）；
  - storage 的新上限。
- egui 界面測試：每種新設置控件都能改值並保存，標題旁有「插件」小標。
- 跑下面的命令，全部通過：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File spike/cargo-serial.ps1 test --locked --lib --features "sdk-js sdk-material" --target x86_64-pc-windows-msvc sdk_spike
```

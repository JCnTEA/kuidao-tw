# P0：插件倉投稿校驗支援新插件

插件倉分支 `feat/submission-status-canvas`。無依賴，第一週就做。共同規則見 [README](README.md)。

## 背景

`scripts/submission.py` 目前接受三類插件：

- 待辦、日程歸屬的腳本插件；
- 快捷切換資源包；
- 無線兼容資源包。

五個官方插件都用到狀態頁、畫布、常駐和聯網，現在的校驗會全部拒收。

## 要做的

- 接受 `owner: "status"` 的腳本插件。
- 接受以下能力：
  - `status.page`、`status.canvas`
  - `island.pinned`
  - `action`、`event`、`notice`
  - `timer`、`storage`、`settings`
  - `card`、`calendar.marker`
- 組合規則與島仔的 `spike/capabilities.rs` 保持一致：
  - `status.canvas` 和 `action` 都要求同時聲明 `status.page`；
  - 歸屬必須帶上自己的輸出。
- 接受清單的以下字段，並做基本類型檢查：
  - `statusLayout`、`network`、`data`、`timezones`、`minRefreshSeconds`、`attribution`、`events`。
- `network.hosts` 必須是完整域名，`sources` 的 URL 必須是 https，且主機在 `hosts` 裡。
- 包內允許出現的文件：
  - `plugin.json`、`main.js`、`cover.png`；
  - 清單 `data` 聲明的 JSON（單個 64 KiB，合計 256 KiB）；
  - 畫布引用的 PNG 和 SVG（合計 512 KiB）；
  - `LICENSE`、`README.md`；
  - `fixtures/` 和 `TESTING.md`。這兩項只用於開發，打包時排除。
- 字體包 `appearance.font`：
  - 允許 TTF、OTF，單個 8 MiB，整包 16 MiB；
  - 必須帶 `LICENSE` 或 `OFL.txt`。
- 在測試文件裡為每條新規則寫通過和拒收兩類用例。原有 18 項測試保持通過。

## 驗收

- `python -m pytest -q scripts` 全部通過。
- 用 `spike/examples/gold-price`、`live-pay`、`world-clock`、`sticky-notes`、`countdown` 的清單做輸入，`read_plugin` 都能通過。

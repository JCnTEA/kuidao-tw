# 第 2 步：聯網取數與憑據

開放受控聯網，一個插件最多可以有 3 個來源。密鑰由島仔託管，刷新間隔允許使用者調整，集市加版本撤回標記。分支為 `feat/plugin-https-source`，共同規則見 [README](README.md)。

## 先讀

- `spike/source_read.rs`：現有的文件和本機 HTTP 只讀來源。
- `spike/local.rs`：`source_kind`、`set_source_grant`，以及 `tick_features` 裡的後台讀取。
- `spike/local_ui.rs`：`source_dialog` 授權彈出視窗。
- `src/mail.rs` 的 `mod secrets`：已經在用 `keyring` 寫 Windows 憑據管理器。
- `spike/market.rs`、`spike/market_state.rs`。
- `docs/plugins/sdk-v1-design.md` 的第 5 節和第 12 節。

## 要做的

### 清單寫法

權限寫成 `"permissions":{"source.read":"https"}`。

清單新增 `network` 字段：

```json
"network": {
  "hosts": ["xaus.com", "api.example.com"],
  "sources": [
    {"key": "gold", "url": "https://xaus.com/api/spot?currency={settings.currency}"},
    {"key": "fx", "url": "https://api.example.com/rate?base=USD"}
  ],
  "credential": {"label": "API Key", "header": "x-access-token", "prefix": ""},
  "minRefreshSeconds": 60
}
```

字段規則：

- `hosts`：1 到 4 個完整域名。不接受通配符、IP、端口，也不接受 localhost。
- `sources`：1 到 3 個來源，`key` 在同一插件裡不能重複。`url` 裡可以寫 `{settings.<key>}` 佔位符，宿主做 URL 編碼後填進去。填完後的地址必須滿足四條：協議是 https；主機在 `hosts` 裡；不含帳號密碼段；端口是 443 或不寫。佔位符只能出現在路徑和查詢串裡，不能出現在主機部分。
- `credential`：可選。聲明後，宿主在每個來源的請求裡注入同一個請求頭。
- `minRefreshSeconds`：可選，範圍 60 到 86400，預設等於 `refreshSeconds`。

### 授權

- 新增 `SourceKind::Https`。授權彈出視窗列出全部來源的主機，使用者確認一次就覆蓋所有來源。
- 聲明瞭 `credential` 的插件，彈出視窗裡多一個密鑰輸入框，內容遮擋顯示，標題用 `credential.label`。
- 密鑰存進 Windows 憑據管理器，複用 `keyring`。服務名單獨起一個，鍵用插件 ID。
- 記錄 JSON 和日誌裡絕不能出現密鑰。
- 撤銷授權或卸載插件時，同時刪除密鑰。
- 插件更新後，如果 `hosts` 或 `sources` 變了，原授權失效，使用者要重新授權。

### 讀取

- 只發 GET，關閉環境代理。
- `redirects(0)`，收到 3xx 一律算失敗。
- 用自定義 resolver。解析出地址後，丟棄迴環、私網、鏈路本地、CGNAT、ULA、組播和未指定地址，剩下為空就失敗。
- 每個來源超時 10 秒，響應最多 256 KiB，必須是 UTF-8。
- 宿主按 `header` 加 `prefix` 注入密鑰，腳本拿不到密鑰。
- 每次取數都重新檢查地址規則。
- 腳本拿到 `sources:{gold:{status,text}, fx:{status,text}}`。舊的單一 `source` 輸入對本地文件和本機來源保持不變。
- 連續失敗時退避重試，間隔依次是 1、2、4、8 倍，上限 30 分鐘。成功一次就恢復正常間隔。

### 刷新間隔

清單聲明瞭 `minRefreshSeconds` 時，插件設定頁自動多出一項「刷新間隔」。使用者可以在 `minRefreshSeconds` 到 86400 秒之間調整，用來照顧免費接口的額度。

### 數據來源標註

- 清單新增 `attribution:{label, url?}`。`url` 的主機必須在 `hosts` 裡。
- 宿主在狀態頁和插件詳情裡顯示「數據來源 xxx」以及最近一次成功取數的時間。具體怎麼畫由第 3 步和驗收方定，你這邊負責把數據提供出來。

### 版本撤回

- 集市目錄的條目可以帶 `revoked:["1.0.1", ...]`。
- 已安裝的版本被撤回時：停用該插件，在「已安裝」裡用警示色標出；如果有更高的未撤回版本，提供更新。
- 被撤回的版本不能安裝。
- 補上 `spike/market.rs` 的解析和對應測試。

### 樣例

- 在 `spike/examples/` 加一個「金價」樣例：
  - 主來源 xaus.com，需要的話再加一個匯率來源；
  - storage 保留最近 48 個價格點，供第 3 步的迷你折線使用；
  - 使用者可以設一個提醒價位，漲過或跌破時用第 1 步的 `notice` 提醒；
  - 顯示數據來源；
  - 不給任何投資建議。
- 另寫一份帶 `credential` 的清單示例放進文檔，以 GoldAPI.io 為例。

## 驗收標準

- 單元測試至少覆蓋下面幾項，且不依賴外網，用本地假服務或直接測函數：
  - 地址校驗：非 https、IP、`hosts` 之外的主機、帶帳號密碼、非 443 端口、佔位符出現在主機部分；
  - 佔位符的 URL 編碼；
  - resolver 過濾私網和迴環地址；
  - 拒絕跳轉；
  - 超大響應；
  - 多來源裡一個失敗時其他來源照常；
  - 退避重試；
  - 刷新間隔的上下限；
  - 密鑰不出現在記錄 JSON 裡；
  - 撤銷後停止讀取；
  - 撤回版本被停用、禁止安裝。
- 下面這條命令全部通過：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File spike/cargo-serial.ps1 test --locked --lib --features "sdk-js sdk-material" --target x86_64-pc-windows-msvc sdk_spike
```

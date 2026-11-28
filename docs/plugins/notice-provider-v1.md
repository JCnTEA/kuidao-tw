# 本機通知來源接口 v1

本機接入端可向島仔發送短時通知、更新常駐狀態，以及接收和確認使用者點擊發出的操作。島仔負責呈現提醒、未讀標記和狀態詳情。接入端自行運行，負責取數和執行操作，例如打開原會話。

## 使用者授權

1. 在「設定 → 消息提醒 → 本機通知來源」填寫接入端提供的來源 ID，以及要顯示在島上的名稱。
2. 點「授權來源」，再點「複製憑據路徑」，把路徑交給接入端。憑據文件內含訪問令牌，只交給自己信任的接入端。
3. 開啟提醒總開關。要在島上閱讀接入端提供的標題和正文，再開啟「顯示通知正文」。

撤銷授權會清除該來源的通知、狀態和待處理操作，並移除憑據文件。沒有已授權來源時，島仔關閉本機接口監聽。

來源 ID 建議使用 `作者.接入端`，例如 `example.agent`，最長 64 個 ASCII 字符。各段允許小寫字母、數字和連字符，並以字母或數字開頭。每個接入端只讀取自己的憑據文件，不把憑據內容寫入日誌或上報。預設路徑是 `%APPDATA%\Daozai\plugin-notices\<來源 ID>.json`。設定中複製的路徑也適用於自定義數據目錄。

## 連接

憑據文件包含 `version`、`host`、`port`、`source`、`token`。當前版本為 `1`。每次連接島仔前重新讀取此文件，避免使用上次啟動的端口和令牌。

通過 TCP 連接憑據中的 `127.0.0.1` 和端口。每個連接發一行 UTF-8 JSON，以換行結束。島仔回一行 JSON 後關閉連接。每個請求均需帶憑據裡的 `version`、`source`、`token`，以及一個 `method`。成功響應含 `ok: true`，失敗響應含 `ok: false` 和可用於診斷的 `error`。

| method | 附加字段 | 行為 |
| --- | --- | --- |
| `status` | 無 | 返回 `preview`。為 `false` 時，接入端可省略可選的 `markdown`。 |
| `notify` | `id`、`title`，可選 `markdown`、`session`、`action_label` | 發送短時通知。彈出視窗收起後可從島上的未讀角標進入。 |
| `status_update` | 與 `notify` 相同，外加 `ttl_seconds` | 更新該來源唯一的狀態卡以及有效期。使用者可從島上的狀態角標打開詳情。更新狀態時不彈通知。 |
| `status_clear` | `id` | 撤下 ID 匹配的當前狀態和待處理動作。重複撤下或 ID 已變更時也返回成功。 |
| `complete` | `id`，可選 `session` 和 `markdown` | 兼容舊版的任務完成通知。 |
| `poll_open` | 無 | 取本來源的一次待處理操作。返回 `open: null` 或 `open: { "request_id": "...", "session": "..." }`。 |
| `ack_open` | `request_id`、`opened` | 操作結束後回執。確認成功才回 `opened: true`，例如確實已打開指定會話。失敗或無法確認時回 `false`。 |

`notify`、`status_update`、`status_clear` 需要包含這些方法的驗收版。協議版本仍為 `1`，舊接入端可繼續使用 `complete`。

## 通知和狀態

將示例中的 `<token>` 換成憑據文件中的令牌。每行是一個獨立請求。

```json
{"version":1,"source":"example.agent","token":"<token>","method":"notify","id":"notice-42","title":"導出完成","markdown":"可以打開原任務查看結果。","session":"task-42","action_label":"查看任務"}
{"version":1,"source":"example.agent","token":"<token>","method":"status_update","id":"job-7","title":"處理中 · 37%","session":"task-7","action_label":"查看進度","ttl_seconds":8}
{"version":1,"source":"example.agent","token":"<token>","method":"status_clear","id":"job-7"}
```

`id`、`title`、`session`、`markdown` 分別最多 128 字節、120 字節、512 字節、32 KiB，均按 UTF-8 計算。`action_label` 最多 6 個字符且不超過 24 字節，提供按鈕文案時需同時提供 `session`。整條請求最多 128 KiB。

- 同一通知重試時沿用原 `id`，不同通知使用新 ID。宿主按來源在有限範圍內去重。每個來源每秒最多發送 4 條新通知，隊列滿或觸及頻率限制時返回 `busy`，接入端應退避後重試。
- `ttl_seconds` 取 2 至 120 的整數。接入端刷新同一個狀態時沿用 `id`，操作目標變化時換新 ID。每個來源的狀態更新間隔至少 250 毫秒，過早更新會返回 `busy`。來源停止更新後，島仔在有效期結束時撤下卡片及待處理動作。

## 操作回執

`session` 是接入端自己定義的操作標識。提供它可讓使用者打開目標會話或觸發接入端支援的其他動作。島仔只把標識交回所授權的來源。接入端運行期間可定期調用 `poll_open`。收到請求後自行執行，在請求發出後 5 秒內提交 `ack_open`。超時或失敗時，使用者可從仍有效的角標重試。打開通知詳情或成功完成通知動作後，島仔會清除其未讀標記。

## 隱私和啟停

關閉正文預覽會隱藏接入端提供的標題、正文和按鈕文案。鎖屏時隱藏詳情。內容和操作標識不會寫入通知歷史，正文以文本渲染，支援常見的 Markdown 標題、列表和代碼塊，預覽最多顯示 128 行。

關閉提醒總開關會清除當前通知、狀態和操作隊列。新寫請求返回 `disabled`，`poll_open` 返回空結果，舊 `complete` 按兼容約定返回成功並丟棄該提醒。重新開啟後，接入端可發送新的狀態或通知。

## 本機示例

將 `example.agent` 授權為「接口示例」，然後把設定中複製的憑據路徑傳給下面的 Python 3 腳本。腳本只依賴 Python 標準庫。它會發出一條通知，更新一張模擬進度卡，並在您點「列印回執」後在控制檯顯示回執。

```powershell
python .\docs\plugins\examples\notice-provider.py "憑據文件路徑"
```

[源代碼](examples/notice-provider.py)

# 本地試裝與測試集市

在 Windows 上，用啟用了 `sdk-js` 的島仔打開「設定 → 插件」。已在使用島仔時，可先從託盤退出當前實例，再啟動試用版。可以雙擊[試用本地插件.cmd](../../spike/試用本地插件.cmd)編譯並啟動。等價的 PowerShell 命令在工作樹根目錄運行，試用數據放進 `target/local-trial`。

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File spike/cargo-serial.ps1 build --locked --features sdk-js --bin daozai --target x86_64-pc-windows-msvc
$env:DAOZAI_DATA_DIR = Join-Path (Get-Location) 'target/local-trial'
& './target/x86_64-pc-windows-msvc/debug/daozai.exe'
```

1. 在「已安裝」底部點「開發者工具」，再點「安裝本地插件…」，選擇[發薪倒計時清單](../../spike/examples/payday/plugin.json)。安裝後打開它的開關。用相同步驟裝上[旅行倒計時](../../spike/examples/trip/plugin.json)。
2. 點發薪插件下方的路徑「設定 › 待辦與置物 ›」，在「待辦與置物」修改標題和目標日期並儲存。展開島上的待辦頁，在原有待辦列表下方看卡片。有兩張卡時，點卡片可切換。
3. 停用旅行插件，只留下發薪卡。重啟同一試用版，旅行仍為停用。關閉「頁面管理 → 待辦」停止所屬插件的刷新，重開待辦後僅發薪卡恢復。
4. 卸載發薪插件並選「保留數據」。從原目錄重裝並啟用，核對上次儲存的日期。再卸載並選「刪除數據」，重裝後應顯示樣例預設日期。旅行插件獨立儲存自己的設定和狀態。

## 作者如何接入

複製一個樣例目錄，在 `plugin.json` 改插件 ID、名稱和設定預設值。宿主按清單的 `entry` 載入 JS。文本與日期字段由清單聲明，插件在 `refresh({ settings, today, storage })` 中得到已儲存的設定、本機日期和上次提交的私有 JSON 數據。返回的 `card` 是顯示文字，`storage` 是本次要儲存的快照。無外部權限。此試用實現支援歸屬待辦的本地卡片，刷新由宿主調度。

把封面儲存為包根目錄的 `cover.png`，並在清單寫入 `"cover": "cover.png"`。PNG 文件最大 256 KiB，寬高不超過 1920×1080，建議使用 16:9。缺圖或圖片無效時，島仔會給出校驗錯誤。

安裝將清單、入口腳本和封面複製到本地記錄，後續獨立於源碼目錄運行。要試修改後的包，可選擇保留數據卸載，從原清單路徑重裝。同一 ID 已安裝時會提示重複。保留數據用 ID 和本地清單路徑識別，僅適用於本人挑選的開發包。預設數據在島仔配置目錄的 `local-plugins` 子目錄。

每個包用同一份清單聲明 `timer` 和 `refreshSeconds`（60 到 86400 秒，省略時為 60）。每次刷新都從頭執行 main.js；下次還要用的值請放進返回的 storage。停用、關閉待辦或改完設定後，上一次計算即使返回也不會再顯示或儲存。action（按鈕）和 event（事件）仍待宿主實現。

重啟時島仔重新核對已安裝記錄的 ID、清單、腳本入口和設定。看到“需要重新安裝”時，在「開發者工具」重新選擇可信的原包，或點「移除記錄」。未通過檢查的記錄不會啟動；重裝會使用原包的預設設定。

需要顯示進度、指標和告警的插件可使用[結構化狀態頁](sdk-v1-design.md#結構化狀態卡和詳情頁)。安裝並啟用插件後，在「設定 → 頁面管理 → 狀態詳情」可打開或暫停該頁。授權文件和本機接口在插件的已安裝詳情中管理。

### 快捷切換插件

包根目錄放置 `plugin.json`、`shortcuts.json` 和 `cover.png`。清單使用 `runtime: "assets"`、`owner: "pages"`、`entry: "shortcuts.json"`、`capabilities: ["navigation.shortcuts"]`、`permissions: {}` 和 `settings: []`。

在 `shortcuts.json` 的 `colors` 中至少填一個頁面的選取顏色。顏色使用六位 `#RRGGBB`。頁面鍵支援 `music`、`focus`、`health`、`shelf`、`tasks`、`mail`、`quota`、`calendar`、`weather`、`performance` 和 `wireless`。

```json
{"colors":{"music":"#FA3E72","focus":"#FF8A30","weather":"#4C9BE8"}}
```

安裝後在「插件 → 已安裝」啟用，再到「設定 → 頁面管理」調整快捷切換圖標的顯示。圖標按島仔的頁面順序排列，點擊可切換到對應頁面。停用或卸載後恢復原有頁碼點。

## 測試集市

測試集市使用本地的 `catalog.json` 和同目錄 ZIP。用另一個插件 ID 或乾淨的試用配置做測試，方便分別驗證本地安裝和集市的安裝來源。

### 作者校驗、打包和提交

複製[發薪倒計時樣例](../../spike/examples/payday)到可編輯目錄。用島仔試用版執行三條命令。下面的示例從工作樹根目錄運行，以已編譯的 Debug 程序為例。

```powershell
$island = (Resolve-Path './target/x86_64-pc-windows-msvc/debug/daozai.exe').Path
$manifest = (Resolve-Path './spike/examples/payday/plugin.json').Path
$bundle = Join-Path (Get-Location).Path 'target/payday-trial.zip'
$catalog = Join-Path (Get-Location).Path 'target/test-market/catalog.json'
New-Item -ItemType Directory -Force (Split-Path $catalog) | Out-Null

& $island --plugin-test $manifest | Out-Host
& $island --plugin-pack $manifest $bundle | Out-Host
& $island --plugin-submit $bundle $catalog | Out-Host
```

`--plugin-test` 檢查清單、入口和封面。JavaScript 插件還會用預設設定試跑一張卡片。`--plugin-pack` 導出含封面的 ZIP。`--plugin-submit` 將已驗證的包放進測試目錄，隨後更新索引。命令失敗時查看輸出，並在修正後重新執行。

支援 GPU 材質的 Windows 版也可以按同樣步驟打包[液態玻璃樣例](../../spike/examples/liquid-glass)。`.wgsl` 入口須在包根目錄並通過著色器校驗。安裝後保持停用，首次啟用時需要確認螢幕底片權限，並開啟 Direct3D 12 兼容渲染。

提交到本地測試目錄後，可以用 `--plugin-avatar <目錄索引> <插件 ID> <GitHub 使用者名稱>` 同步作者頭像。例如 `& $island --plugin-avatar $catalog cayww.liquid-glass cayww`。同步後刷新測試集市目錄即可查看，插件沿用原版本和 ZIP。

要試更新，把樣例副本的 `version` 從 `1.0.0` 改成 `1.0.1`，同時給卡片文案加一處能看出版本差異的改動。保持插件 ID、設定鍵和數據版本。用新清單、新 ZIP 文件路徑再次校驗和打包，然後提交到同一個 `catalog.json`。

### 使用者安裝和更新

1. 在「設定 → 插件 → 已安裝 → 開發者工具」點「導入測試集市目錄…」，選擇 `catalog.json`。導入後集市切到「本地測試」，右上角可以在「官方」和「本地測試」之間切換。官方集市優先使用本機緩存，缺少緩存時自動載入，可點擊刷新按鈕手動同步目錄和統計。
2. 在本地測試集市找到插件並點「安裝」，切換到「已安裝」打開開關。
3. 點插件下方的路徑或島上插件卡片的小齒輪，在「待辦與置物」修改名稱和日期。停用後，這一組插件設定和卡片收起。暫停待辦模塊時，開關保留原啟用選擇，路徑變成琥珀色的「頁面管理 › 待辦與置物 ›」。
4. 作者提交更高版本後，在同一個測試目錄點「刷新目錄」和「更新」。重新打開卡片核對新文案，原名稱、日期和啟用狀態仍在。
5. 卸載時選擇保留數據，從原測試目錄重裝同 ID 插件並啟用，核對原設定。選擇刪除數據時，下次安裝使用樣包預設值。

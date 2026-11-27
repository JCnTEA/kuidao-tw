# 島仔插件 SDK v1

## 1. 插件如何工作

SDK 是島仔提供給插件作者的一組公開約定。宿主指運行插件的島仔。你提供一份清單和幾段函數，島仔負責調用、畫界面和儲存結果。

第一版採用**每次調用都從頭運行**的寫法。函數只根據輸入計算並返回結果，需要記住的內容放進返回值的 `storage`。兩次調用之間，腳本變量會消失。宿主負責安排刷新、讀取已授權的數據源和接收使用者點擊。當前 Windows 測試版可運行 QuickJS 待辦卡片、日程日期標註、AI 額度來源與獨立狀態頁。[運行環境實測](runtime-spike.md) · [本地安裝](local-install.md)

## 2. 我能寫哪些功能

先試[發薪倒計時](../../spike/examples/payday/plugin.json)和它的[完整入口腳本](../../spike/examples/payday/main.js)。複製整個目錄，修改清單中的 ID（識別插件的固定名稱）和顯示名稱，就能安裝第二份。`plugin.json` 是插件的清單。JSON 用字段名和值來記錄設定、刷新間隔和權限。

當前宿主按全局函數名調用 `refresh(input)`。輸入是 `{settings, today, storage}`，日期格式為 YYYY-MM-DD，首次調用的存儲值為 `null`。待辦卡片返回 `{card, storage}`，其中 card 為文字，storage 為本次完整的私有數據。下面這段可以直接替換樣例的 main.js。

```js
function refresh({ settings, today }) {
    const days = Math.round((Date.parse(settings.target) - Date.parse(today)) / 86400000);
    return { card: settings.title + " · " + days + " 天", storage: { lastDays: days } };
}
```

| 能力與進度 | 作者怎麼寫 | 使用者在哪裡用 |
| --- | --- | --- |
| 文本、日期設定 · 已實現 | 清單的 settings 列出字段，在上例讀取 settings.title 和 settings.target | 歸屬模塊的插件設定 |
| 材質數字參數 · 已實現 | 清單聲明 `type: "number"`、`min`、`max`、`step` 和字符串形式的 `default`，可選 `unit` 和 `precision`。著色器按清單順序用 `param(index)` 取值，最多 32 項 | 「設定 → 外觀 → 材質」，拖動滑塊實時應用 |
| 腳本插件設定 · 實驗 | text、date、switch、choice、number、time、weekdays、timezone、list。預設值和儲存值均為字符串 | 所屬設定頁 |
| 文字卡、私有存儲 · 已實現 | 上例返回 card 和 storage。首次的 storage 為 null，之後可用 `storage?.lastDays` 讀迴天數 | `owner: "tasks"` 顯示在待辦頁 |
| 日期標註 · 已實現 | 聲明 `calendar.marker`，`refresh` 返回 `marker`，空字表示當天不標註。舊版 `owner: "calendar"` 加 `card` 的寫法繼續有效，`card` 即標註 | 展開「日程 → 日曆」查看電腦當天日期下方的標註 |
| 一個插件多處輸出 · 測試版 | 在 capabilities 裡同時列出幾種輸出，例如 `["card","calendar.marker","timer"]`，一次返回 `{card, marker}`。同時有多種輸出時，某項返回空值表示這次不顯示 | 每處輸出只跟著自己所在模塊的開關，關日程只撤日曆標註；關掉插件歸屬的模塊則整個插件暫停 |
| 週期刷新 · 已實現 | 兩個樣例在清單聲明 `"refreshSeconds":60` 和 timer 能力。到期由島仔調用 refresh | 卡片按期更新 |
| 島上常駐位 · 實驗 | 聲明 `island.pinned`，通過 `refresh` 返回 `pinned:{text:"今日金價",value:"850 元/克",icon:"coin"}` | 收起時各插件一起顯示，使用者可調整順序 |
| 結構化狀態卡和獨立詳情頁 · 測試版 | 清單使用 `"owner":"status"` 和 `"capabilities":["status.page","timer"]`，`refresh` 返回 `status`。可用 `statusLayout` 選擇組件、排序和樣式。 | 在島上翻頁查看，在「設定 → 頁面管理 → 狀態詳情」單獨啟停 |
| 狀態頁組件與主題令牌 · 實驗 | 在狀態頁佈局中添加列表、迷你折線、多行說明和標籤，可設強調色、強調程度及島仔內置圖標 | 在狀態詳情頁查看 |
| 自繪畫布 · 實驗 | 同時聲明 `status.page` 和 `status.canvas`，腳本返回畫布的圖元與樣式 | 可填滿狀態詳情頁或與標準組件混排 |
| 實時狀態組件與敏感值 · 實驗 | 佈局選擇 `counter`、`clock`、`countdown`，由島仔計算實時讀數。金額等數據須標記 `sensitive:true`。 | 在狀態詳情頁查看，點擊暫時顯示敏感值 |
| 狀態詳情交互 · 實驗 | 同時聲明 `action` 和 `status.page`。可使用按鈕、文本表單、可點的列表項，以及宿主的打開連結和複製動作 | 在狀態詳情輸入內容或點擊操作 |
| 可編輯獨立模塊 · 待宿主實現 | 清單聲明 module 與按鈕 `actions:["save"]`。示意 `function action(i){return {page:{kind:"detail",text:i.form.text},storage:{note:i.form.text}}}` | 島上功能欄查看、編輯，側欄調偏好，頁面管理啟停 |
| 日程來源 · 待宿主實現 | `function refresh(i){return {calendar:[{id:"anniversary",title:i.settings.title,date:i.settings.target}]}}` | 日程裡的來源，按來源和條目 ID 更新 |
| 歌詞來源 · 待宿主實現 | `function refresh(i){return {lyrics:{trackId:i.track.id,lines:i.source.lines}}}`。換歌後宿主只接收當前曲目的結果 | 原音樂頁 |
| AI 額度來源 · 測試版 | 清單使用 `"owner":"quota"` 和 `"capabilities":["quota.source","timer"]`，`refresh` 返回 `quota`。 | 原 AI 額度面板中按插件名稱顯示來源頁籤 |
| 專注事件 · 實驗 | 聲明 event，清單頂層寫 "events":["focus.finished"]。專注計時完成時調用 event(input) | 啟用插件後完成一輪專注 |
| 一次性提醒 · Windows 實驗 | 聲明 notice，腳本返回 notice 對象 | 島上的通知與未讀，遵循系統內的通知和正文顯示開關 |
| 只讀數據源 · Windows 測試版 | 清單通過 `permissions.source.read` 聲明 `file`、`loopback` 或 `https`。宿主將已授權數據傳給 `refresh` 的 `source` 輸入。 | 「設定 → 插件 → 已安裝 → 授權數據源」 |
| 皮膚 · 測試版 | `runtime: "assets"`、`owner: "appearance"`、`appearance.skin`，入口 `theme.json` | 設定 → 外觀 → 皮膚 |
| 功能圖標 · 測試版 | `runtime: "assets"`、`owner: "appearance"`、`appearance.icon`，入口 `icons.json` | 設定 → 外觀 → 功能圖標，替換設定側欄及功能標題卡圖標 |
| 吉祥物表情 · 測試版 | `runtime: "assets"`、`owner: "appearance"`、`appearance.pet`，入口 `expressions.json` | 設定 → 外觀 → 吉祥物表情，沿用原動作的觸發時機與幀速 |
| 畫布字體 · 測試版 | `runtime: "assets"`、`owner: "appearance"`、`appearance.font`，入口 `fonts.json` | 在已安裝插件中啟用，畫布文字按名稱引用 |
| 底部快捷切換 · 已實現 | 聲明 `runtime: "assets"`、`owner: "pages"` 和 `navigation.shortcuts`，在 `shortcuts.json` 配置選取配色 | 展開島仔，點擊底部圖標直達已啟用頁面。設定歸屬「頁面管理」 |
| GPU 材質 · 已實現 | 清單用 `runtime: "material"`、`owner: "appearance"`、`appearance.material` 能力和 `screen.backdrop` 權限。WGSL 入口實現 `material()`，沿用[材質接口](gpu-material.md) | 「設定 → 外觀 → 材質」，同一時間選用一個材質，需開啟 Direct3D 12 兼容渲染 |

action 接收按鈕 ID 和表單值，event 接收事件名和事件數據。兩個入口沿用 refresh 的輸出與私有存儲格式。

## 3. 使用者去哪裡用，去哪裡調

島仔用一張能力表核對清單（[能力表](../../spike/capabilities.rs)）。每種能力寫明需要的運行環境、落在哪裡，以及能否和別的能力同時聲明。`owner` 只決定插件設定放在哪個設定頁，以及哪個模塊關掉時整個插件暫停。腳本插件的歸屬可選 tasks、calendar、quota、status，並且至少要帶上這個歸屬自己的輸出：tasks 帶 card，calendar 帶 card 或 calendar.marker，quota 帶 quota.source，status 帶 status.page。材質、皮膚、圖標、表情、快捷切換和無線設備兼容包各自獨佔一個包。外觀資源包只聲明一種能力，`permissions` 填 `{}`，`settings` 填 `[]`。

外觀資源包由 `plugin.json`、入口文件和 `cover.png` 組成，本地開發目錄也可帶說明文件。皮膚、圖標和表情的集市 ZIP 上限為 832 KiB，字體包另有文件限制。各類入口使用 UTF-8 JSON。

- 畫布字體包的 `fonts.json` 格式為 `{"fonts":[{"name":"站酷快樂體","file":"ZCOOLKuaiLe-Regular.ttf"}]}`。可列出 1 至 4 個名稱和文件各不相同的字型，支援 TTF 與 OTF，單個最多 8 MiB，整包最多 16 MiB。包根目錄須有 `LICENSE` 或 `OFL.txt`。啟用後，畫布文字可用 `font:"站酷快樂體"`，宿主以 `plugin:站酷快樂體` 註冊。缺字、停用或卸載時沿用島仔字體。 [站酷快樂體樣例](../../spike/examples/font-zh/plugin.json) · [Google Fonts 字體許可](https://github.com/google/fonts/blob/main/ofl/zcoolkuaile/OFL.txt)

- 皮膚包入口最多 4 KiB，例如 `{"base":"cream","colors":{"paper":"#F4F8F6","ink":"#20382F","accent":"#248161"}}`。經典 `classic` 和奶油 `cream` 開放 `paper`（底色）、`ink`（文字）和 `accent`（強調色）。毛玻璃 `glass_light`、`glass_dark` 和懸浮雲 `fluid` 開放 `accent`，保留原來的透明度和紋理。至少提供一個 `#RRGGBB` 顏色。缺少的顏色沿用基礎皮膚。可參考[青瓷皮膚樣例](../../spike/examples/celadon-skin/plugin.json)。
- 圖標包入口最多 128 KiB，格式為 `{"icons":{"music":"<svg …>…</svg>"}}`。功能頁鍵可用 `music`、`focus`、`health`、`shelf`、`tasks`、`mail`、`quota`、`calendar`、`weather`、`performance`、`wireless`。每個 SVG 最多 8 KiB，聲明 SVG 命名空間與 `viewBox`，使用路徑、基礎形狀和分組，可用十六進制顏色、`none` 或 `currentColor`。禁止腳本、樣式表、外部資源和 `foreignObject`。
- 表情包入口最多 768 KiB，格式為 `{"expressions":{"wink":"PNG 的 Base64 字符串"}}`。可替換 `wink`、`smug`、`blush`、`delighted`、`side-glance`、`praised`、`heart`、`giggle`、`shy-peek`、`scan`、`peek`、`search`。每張 PNG 為 144×1440，從上到下排列 16 幀，每幀 144×90，最多 64 KiB。缺少的圖標或表情沿用內置素材。

使用者習慣去原功能完成的事，可以往原設定頁加一項。皮膚進入外觀選擇器，歌詞來源進入音樂設定。只需要看一眼的內容可以做卡片，少量參數仍放在所屬頁。

使用者要反覆編輯便籤、管理紀念日，或專門查看列印進度、溫度和告警時，可以新增獨立模塊。作者選擇展示內容與樣式，島仔負責字體、縮放和點擊。正文和一次性輸入在功能頁編輯，預設排序等偏好在模塊設定頁修改。

狀態詳情頁通過 `owner: "status"` 和 `status.page` 接入，按照插件 ID 找到頁面和獨立開關。島上的齒輪與「已安裝」裡的頁面路徑都進入頁面管理。可編輯獨立模塊的設計使用 `owner: "self"` 和 `module`。

每個插件只有一個主要設定歸屬。「已安裝」每行統一提供路徑（前往設定）、開關和「···」選單（查看詳情、回饋問題、卸載）。列表按設定歸屬分組，材質組同時只開一個。頁面管理控制模塊運行。設定頁複用島仔已有的開關、選擇、數字、文本行和功能標題卡，保留必要設定與關鍵提醒，合併重複提示，內容多時才收起低頻選項。

插件提供的設定項在所屬設定頁使用統一樣式。標題旁顯示「插件」小標，插件開關開啟時用紫色，關閉時用灰色。複用宿主現有的小標和開關組件。頁面快捷切換等顯示偏好開關也使用該樣式，各自儲存對應的顯示選擇。

### 島上常駐位（實驗）

插件保留原有歸屬和對應輸出能力，並在清單增加 `island.pinned`。`pinned.text` 最長 12 字，`value` 可用數字、最多 24 字的文字、`{counter:{…}}` 或 `{countdown:{…}}`。實時參數與狀態頁共用，由宿主按需重繪。`icon` 可選 `clock`、`chart`、`coin`、`calendar`、`cup`、`cloud`。通過 `pinned.sensitive:true` 隱藏值，計數器自身的 `sensitive:true` 也會生效。

同時聲明 `status.canvas` 後，也可返回 `pinned.canvas` 對象或對象數組。同一插件有多張常駐畫布時，每八秒切換一張。

在插件所屬設定頁開啟「在島上常駐顯示」，再到「設定 › 頁面管理 › 島上顯示」調整順序。收起時各插件一起顯示，上下停靠時橫向並排，左右停靠時縱向排列。橫向島體按內容加寬，受顯示空間限制時由島體統一適配。通知彈出視窗（含接入為通知的來電）和島上歌詞優先顯示。展開、鎖屏、全屏隱藏或極簡模式時讓出位置。關閉插件或其所屬模塊會立即撤下內容。

## 4. 關閉後會發生什麼

「已安裝」的開關儲存使用者對插件的啟用選擇，只表示插件自己，不隨模塊狀態變色。「頁面管理」獨立儲存所屬模塊的啟用選擇，獨立模塊也遵循這條規則。兩個開關都開啟時，插件才有運行資格。安裝後預設停用。

| 插件總開關 | 所屬模塊開關 | 「已安裝」顯示 | 設定、島體與後台 |
| --- | --- | --- | --- |
| 關 | 任意 | 灰色開關，封面去色淡化 | 收起插件設定、島體內容和專屬功能入口，停止發起新的定時調用，保留數據 |
| 開 | 關 | 紫色開關，路徑變成琥珀色「頁面管理 › …」 | 收起插件輸出和設定，暫停定時調用，保留原啟用選擇，點路徑前往頁面管理 |
| 開 | 開 | 紫色開關，路徑「設定 › …」 | 恢復內容、設定入口和定時調用，使用已儲存的數據 |

兩個開關各自持久化。暫停模塊後，重新啟用插件總開關時模塊仍暫停。先停用插件再重開所屬模塊時，插件仍停用。顯示位置開關隻影響對應的顯示位置。

| 使用者操作 | 應看到的結果 |
| --- | --- |
| 在「已安裝」關掉今日農曆的開關 | 日曆裡的農曆和節氣標註消失，日程頁仍可使用 |
| 關日程，再開日程 | 關閉時暫停農曆插件。重新打開日程後，已啟用的農曆插件恢復顯示 |
| 紀念日同時在日曆和常駐欄展示 | 關日程只撤日曆記錄。關紀念日模塊時撤下功能入口、固定倒數及日曆記錄，「已安裝」保留總開關，路徑提示去頁面管理。兩項開關都開啟時找回原文、日期和固定選擇 |
| 奶油皮膚換成插件皮膚，期間把膠囊改成劉海，再停用或卸載皮膚包 | 回到奶油＋劉海，保留使用者設定的停靠位置。皮膚包重新啟用後可再次到外觀選擇 |
| 停用或卸載正在使用的圖標包或表情包 | 立即換回對應的內置素材 |
| 停用後重啟，或卸載 | 停用立即撤下輸出，宿主停止發起新調用。已經開始的一次計算即使稍後返回也丟棄結果。重啟保持停用，卸載可選擇保留或刪除數據 |

修改設定、關上級模塊或重新安裝時，同樣作廢此前發出的計算。插件重新啟用後按當前設定重新計算。單選來源由島仔記住，當前來源不可用時回到最近一次可用的選擇。未獲授權、執行出錯或超時的調用，其結果一律作廢。

## 5. 誰可以讀什麼

權限說明使用者允許島仔替這個插件做哪些事。安裝前看作者、歸屬和權限摘要，首次啟用確認具體用途，更新時新增的權限單獨確認。撤權後島仔停止取數並丟棄舊請求的結果。

JS 卡片可用空權限，或申請一個本地只讀數據源。材質插件首次啟用時單獨確認 `screen.backdrop` 權限，島背後的像素只在本機用於材質繪製，著色器可採樣底片紋理，像素不儲存、不上傳。

### 只讀數據源

待辦、日程、額度來源和狀態詳情插件可以申請一類數據源。在「設定 → 插件 → 已安裝」授權所選文件、本機 HTTP 地址或清單聲明的 HTTPS 來源。授權可隨時撤銷，卸載會清除授權。

- `file` 讀取系統選擇器選取的一個 UTF-8 文件，拒絕網路盤、目錄和重解析路徑。數據文件可以在原路徑更新。
- `loopback` 對使用者確認的完整地址做 GET，例如 `http://127.0.0.1:8080/status`。支援 `127.0.0.1`、`[::1]` 和 `localhost`，直接連接回環地址，設定 2 秒請求預算，拒絕重定向、URL 帳號密碼和非迴環地址。
- `file` 和 `loopback` 的讀取上限均為 128 KiB。
- `https` · 實驗。清單在 `network.hosts` 列出 1 至 4 個完整域名，由使用者一次授權 `network.sources` 聲明的 1 至 3 個 GET 來源。每個來源使用唯一 `key`，路徑和查詢串可以用 `{settings.設定鍵}` 引用清單中聲明的設定值，宿主按 UTF-8 百分號編碼。舊清單仍可使用 `network.url` 預填單一地址。
- HTTPS 每次請求都按白名單核對地址，只連接解析出的公網地址，使用 443 端口。每個來源禁用代理和重定向，超時 10 秒，響應最多 256 KiB UTF-8。
- 腳本收到狀態和響應文本。路徑、地址、密鑰及文件和網路操作留在宿主。

需要密鑰的清單可聲明 `credential`。宿主將密鑰儲存在 Windows 憑據管理器，按聲明的請求頭和前綴向每個來源發送。撤銷授權或卸載會刪除密鑰。更新時改動域名白名單、來源或憑據配置，需要重新確認授權。以下是使用 GoldAPI.io 的清單片段。

```json
{
  "permissions": {"source.read": "https"},
  "network": {
    "hosts": ["www.goldapi.io"],
    "sources": [{"key": "gold", "url": "https://www.goldapi.io/api/price/XAU/CNY"}],
    "credential": {
      "label": "GoldAPI.io API Key",
      "header": "x-access-token",
      "prefix": ""
    },
    "minRefreshSeconds": 300
  },
  "attribution": {"label": "GoldAPI.io", "url": "https://www.goldapi.io/price/XAU/CNY"}
}
```

[國際現貨金價樣例](../../spike/examples/gold-price)可直接試裝，演示迷你折線和閾值提醒。XAUS 的 `/api/v1/spot?currency=CNY&unit=gram` 可返回人民幣每克的國際現貨參考報價。XAUS 要求至少緩存 60 秒，高用量商業集成應按其公平使用說明聯繫服務方。[XAUS API 公平使用說明](https://xaus.com/api/#fair-use)

樣例把最近 48 個新報價存入 `storage.points` 並繪製趨勢。價格向上或向下穿越閾值時，島仔顯示一次性提醒，報價時刻用於提醒去重。獨立驗證本步 HTTPS 時可試裝[倉庫動態樣例](../../spike/examples/https-repository)。

`network.sources` 按清單中的 key 向腳本提供 `sources`，例如 `sources.gold`。文件、本機 HTTP 和舊版單一 HTTPS 仍使用 `source`。每個來源未授權為 `{status:"unconfigured"}`，無法讀取為 `{status:"offline"}`，讀取成功為 `{status:"ready",text:"…"}`。校驗和打包時傳入 `unconfigured`。單個來源失敗後獨立退避，間隔依次為正常值的 1、2、4、8 倍，退避上限為 30 分鐘，並遵守使用者設定的正常間隔。成功一次後恢復正常週期。

`timer` 按 `refreshSeconds` 調度。聲明 `minRefreshSeconds` 後，所屬插件設定頁提供「刷新間隔」，可在該值至 86400 秒之間選擇，未設定時使用 `refreshSeconds` 下限校正後的值。清單可選填 `attribution`，宿主在狀態頁和已安裝詳情顯示來源與最近成功取數時間。

```js
function refresh({ source }) {
  if (source.status === "unconfigured") return { card: "請授權數據源" };
  if (source.status === "offline") return { card: "數據源暫不可用" };
  return { card: source.text.trim().slice(0, 80) || "暫無數據" };
}
```

停用插件、關閉歸屬頁面、撤權或更換授權會停止舊來源的後續讀取，並丟棄已經發出的讀取結果。數據源失聯時隻影響該插件的輸入，下一次刷新可以恢復。

### AI 額度來源

在 JS 清單中聲明 `"owner":"quota"` 和 `"capabilities":["quota.source","timer"]`。需要取數時，加上上一節的 `source.read` 權限。插件的 `refresh` 返回 `{quota: ...}`，額度面板用插件清單中的名稱展示來源。

```js
function refresh({ source }) {
  if (source.status !== "ready") return { quota: null };
  const { remaining, total, resetAt } = JSON.parse(source.text);
  return { quota: { windows: [
    { label: "當前週期", remaining, limit: total, unit: "積分", reset_at: resetAt }
  ] } };
}
```

- `windows` 可提供 1 至 3 項，每項填寫 `label`。百分比使用 `remaining_pct`，範圍為 0 到 100。真實數值使用非負的 `remaining` 和 `unit`，可另填正數 `limit`，保留原單位。
- 每項可填 `period_secs` 和 `reset_at`，分別是週期秒數和重置 Unix 時間戳。整份額度可選 `expires_at`，用於顯示到期 Unix 時間戳。到期與重置各自獨立。
- 可返回 `quota: null` 撤下當前讀數。申請取數權限的插件，在未授權或讀取失敗時由宿主管理撤下；其他來源的數據和選取設定可繼續使用。暫停額度頁或停用插件時，該來源會撤下。面板手動刷新會同時觸發已啟用的額度插件。
- 額度輸出保留在本次運行中。若插件聲明瞭 `storage`，腳本可額外返回自己的私有存儲。打包校驗時源狀態為 `unconfigured`，入口應能處理該狀態。

### 結構化狀態卡和詳情頁

JS 清單使用 `"owner":"status"`，能力列出 `status.page`。可選的 `timer`、`settings` 和 `storage` 沿用現有約定。讀取文件或本機接口時使用同一套 `source.read` 授權。宿主會用清單的顯示名稱和固定插件 ID 註冊詳情頁，無需聲明頁面編號。

清單可用 `titleAccent` 指定 `name` 中要著色的末尾文字。例如 `"name":"現貨金價","titleAccent":"金價"`。顏色取 `statusLayout.style.accent`，省略時用當前皮膚的預設強調色。插件列表仍使用完整名稱。

獨立狀態頁插件可在 `plugin.json` 中聲明 `navigationIcon`。宿主在設定側欄和島上快捷導航繪製同一輪廓。側欄使用 `background` 的底色和白色描邊，快捷導航沿用當前的選取與懸停色。省略此字段時顯示宿主圖標。

```json
{
  "navigationIcon": {
    "path": "M5 6H19V18H5Z M8 10H16 M8 14H13",
    "background": "#587CA8"
  }
}
```

`path` 使用 24×24 座標，端點和控制點都在 0 至 24 之間。支援 `M/L/H/V/Q/C/Z` 及其小寫相對命令，必須包含可繪製的線段或曲線，最多 64 條命令和 1024 字節。`background` 使用 `#RRGGBB` 格式。

返回 `{status: ...}`，`note` 支援多行，其餘文字字段填單行可顯示文字。

| 字段 | 取值 |
| --- | --- |
| `state`、`label` | 必填。狀態選 `running`、`paused`、`succeeded`、`error`、`offline` 或 `idle`，標籤 1 至 40 字 |
| `summary`、`alert` | 可選的摘要與告警，各 1 至 120 字 |
| `progress_pct` | 可選，0 至 100 的數字 |
| `metrics` | 可選，最多 6 項。每項填 `label`（1 至 28 字）和 `value`（1 至 32 字），數值和單位可放在同一段文字，可用 `sensitive:true` 隱藏值 |
| `items` | 可選，最多 8 項。每項有 `label`（1 至 28 字），可選 `detail`（1 至 80 字）、`state`（同上）、`action`（第 1 步提供的點擊動作 ID）和 `sensitive:true`（隱藏整項） |
| `series` | 可選，2 至 48 個有限數字，按數組順序繪製。可同時提供 1 至 12 字的 `series_label` |
| `note` | 可選，1 至 300 字的多行說明，支援 LF 或 CRLF 換行 |
| `tags` | 可選，最多 4 個標籤，每個 1 至 8 字 |
| `counter` | 跳動數字，每項填 `start_value`、`per_second`、`start_at`（Unix 秒）及 `decimals`（0 至 4）。可用 `until`（Unix 秒）在終點停住，可加 `prefix`、`suffix`（各最多 12 字）及 `sensitive:true` |
| `clock` | 時鐘，每項填 `label`、`utc_offset_minutes`（–840 至 +840）及 `format`（`HH:mm` 或 `HH:mm:ss`） |
| `countdown` | 倒計時，每項填 `label`、`target_at`（Unix 秒）及 `format`（`d天 HH:mm` 或 `HH:mm:ss`）。分鐘格式向上取整，到時顯示 `done_label`，省略時顯示「已結束」 |
| `observed_at`、`expires_at` | 可選的 Unix 秒，分別表示觀察和過期時間。觀察時間須早於或等於過期時間，到了過期時間會撤下舊讀數 |

#### 自定義詳情頁佈局

在 `owner: "status"` 的插件清單中添加 `statusLayout`。缺省時使用原有預設模板。佈局從清單讀取，`refresh` 仍按上表返回 `status`。下例在狀態標題後顯示橫向進度條、摘要、指標和告警。

```json
{
  "statusLayout": {
    "components": [
      {"type": "state"},
      {"type": "progress", "variant": "bar"},
      {"type": "summary"},
      {"type": "metrics", "columns": 2},
      {"type": "alert"}
    ],
    "style": {"accent": "#5BADEF", "density": "compact", "surface": "card"}
  }
}
```

選擇 1 至 10 個組件，數組順序就是展示順序。普通組件每種最多一個，三種實時組件各可重複 4 次，每次計入佈局上限。實時數據接受單個對象或數組，佈局中第幾個同類組件對應數組的第幾項。有數據的組件按順序顯示，卡片內容溢出時可以在卡片內滾動。

有 `canvas` 組件和 `action` 能力時，可用 `statusLayout.canvasFields` 把最多三個單行輸入框放到畫布內。例如 `[{"key":"note","rect":[4,207,130,22],"placeholder":"記一筆…","action":"add"}]`。座標從畫布左上角起算，使用邏輯像素整數。返回同名 `status.fields` 和對應的 `status.actions` 後，使用者可直接在畫布內錄入，按回車提交。畫布內的底色和邊框由插件設定。錨點不可用時，仍可用佈局中的標準輸入框和按鈕操作。

| 組件 `type` | 數據與選項 |
| --- | --- |
| `state` | 顯示 `state` 和 `label` |
| `summary` | 顯示 `summary` |
| `progress` | 顯示 `progress_pct`。`variant` 可選 `ring`（預設，圓環）或 `bar`（橫向進度條） |
| `metrics` | 顯示 `metrics`。`columns` 可選 `1` 或 `2`（預設） |
| `list` | 顯示 `items`，用顏色點表達每項的狀態。每項的點擊和複製需要 `action` 能力 |
| `chart` | 將 `series` 繪成迷你折線，包含網格、面積填充和末點 |
| `text` | 按行顯示 `note`，超長部分用省略號提示 |
| `badge` | 顯示 `tags` |
| `alert` | 顯示 `alert` |
| `updated_at` | 顯示 `observed_at` |
| `actions` | 顯示 `actions` 中的按鈕，需要 `action` 能力 |
| `fields` | 顯示 `fields` 中的文字方塊，需要 `action` 能力 |
| `counter`、`clock`、`countdown` | 用對應的字段在宿主計算讀數。按秒或分鐘的邊界重繪，腳本按原週期刷新 |
| `canvas` | 繪製返回值中與 `status` 同級的 `canvas`，需要 `status.canvas` 能力 |

`style` 可以省略。強調色 `accent` 使用六位 `#RRGGBB`，島仔按實際底色校正到至少 3:1，省略時沿用島仔預設狀態色。間距 `density` 可選 `comfortable`（預設）或 `compact`。緊湊模式最多顯示 3 項列表、2 行說明、3 個標籤。背景 `surface` 可選 `card`（預設）或 `plain`。強調程度 `emphasis` 可選 `normal`（預設）或 `strong`，作用於狀態標題和折線。`icon` 可將島仔內置的 `battery`、`cup`、`clock`、`chart`、`cloud` 或 `calendar` 圖標放到狀態標題旁。

[列印機狀態樣例](../../spike/examples/printer-status/plugin.json) 通過模擬腳本演示列表、折線、說明、標籤和主題令牌。[實時工資](../../spike/examples/live-pay/plugin.json) 和 [世界時鐘](../../spike/examples/world-clock/plugin.json) 分別演示敏感實時金額、常駐上下班倒計時，以及最多四個可配置的城市時鐘。世界時鐘由宿主換算所選 IANA 時區。

敏感值預設打碼，點擊後顯示 30 秒，再次點擊可立即蓋上。敏感計數參數更新後重新打碼。鎖屏、島仔演示錄製及開啟「截圖和錄屏中隱藏島仔」時始終打碼。使用第三方錄屏前可開啟此設定。

返回 `{status:null}` 時顯示等待狀態。取數失敗會撤下舊讀數，在本頁顯示數據源暫不可用。來源正常返回 `state:"offline"` 時，按插件提供的設備離線狀態展示。宿主按插件 ID 分開刷新和撤下狀態。

全新的插件預設停用，首次啟用後詳情頁預設出現。可在「頁面管理 → 狀態詳情」儲存這個頁面自己的開關。關閉詳情頁後，島體撤下對應頁面和狀態卡，後台停止讀取該插件的數據源，正在進行的結果作廢。再次啟用按當前授權重新讀取。

#### 自繪畫布

JavaScript 插件在清單中同時聲明 `status.page` 和 `status.canvas`。`refresh`、`action`、`event` 都可以返回 `canvas`。返回畫布時，省略佈局或只配置 `{"type":"canvas"}` 會讓畫布佔滿頁內內容區。與標準組件混排時，在 `statusLayout` 中添加一次 `{"type":"canvas"}` 指定畫布位置，並通過同一份結果的 `status` 為標準組件提供數據。

```js
function refresh() {
    return {
        canvas: {
            height: 64,
            nodes: [
                { rect: [0, 0, 387, 64], r: 10, fill: "$card" },
                { text: "從這裡開始畫", at: [16, 38], size: 16, fill: "$text" }
            ]
        }
    };
}
```

座標單位為邏輯像素，原點在標題行下方的內容區左上角，寬 387。畫布高度 `height` 為 40 至 360。宿主隨渲染縮放繪製，靜止畫面複用緩存。

| 圖元 | 字段 |
| --- | --- |
| 矩形 | `rect:[x,y,w,h]`，可用 `r` 指定統一圓角或按左上、右上、右下、左下給出四個圓角 |
| 圓 | `circle:[cx,cy,r]` |
| 弧 | `arc:{c:[cx,cy],r,start,sweep}`，角度從水平向右開始，順時針為正 |
| 直線 | `line:[x1,y1,x2,y2]` |
| 折線 | `points:[[x,y],…]`，加 `area:y` 可填充曲線與水平線間的區域 |
| 路徑 | `path:"M0 0 L10 10 Q12 4 18 6 Z"`，支援 M、L、H、V、Q、C、Z 及其小寫相對命令 |
| 文字 | `text:"文字",at:[x,y],size:14`。`at` 為基線位置，可加 `weight`、`align`（left、center、right）、`max_width`、`lines`、`font` |
| 圖片 | `image:"badge.svg",rect:[x,y,w,h]`，用 `fit` 選擇 cover（裁切鋪滿）或 contain（完整顯示） |
| 分組 | `group:[…]`，可加 `clip:[x,y,w,h]`、`translate:[x,y]`、`rotate`（度）、`scale` |

`fill` 和 `stroke` 可以用 `#RRGGBB`、`#RRGGBBAA` 或跟隨皮膚的 `$accent`、`$text`、`$muted`、`$dim`、`$card`、`$track`、`$bg`。文字顏色也可以用 `color`。線性漸變寫作 `{linear:[[x1,y1],[x2,y2]],stops:[[0,"#FFE58A"],[1,"#FFE58A00"]]}`，徑向漸變將軸換成 `radial:[[cx,cy],r]`。漸變停靠位置按從 0 到 1 的順序排列。線型用 `width` 設定線寬、`cap` 選擇 round 或 butt、`dash:[實線長,間隔長]` 設定虛線。通用效果有 `opacity`（0 至 1）、`shadow`（sm、md、lg）和 `glow`（true 或一種顏色）。可用 `id` 為圖元提供穩定名稱。

本地安裝和 ZIP 都支援用包內相對路徑引用 PNG 和靜態 SVG，例如 `images/badge.svg`。資源隨包放置，直接在圖元中填寫路徑即可。`/` 分隔路徑層級，宿主拒絕絕對路徑、含 `..` 的路徑和符號連結。SVG 需要有效的 `viewBox`，使用路徑、矩形、圓等靜態矢量圖形。

畫布圖片每張最大 1024 × 1024，文件合計最多 512 KiB。集市封面獨立校驗。封面尺寸合規且畫布還有剩餘圖片額度時，可以用 `image:"cover.png"` 引用，並計入畫布圖片總量。

內置圖標節點寫作 `{"icon":"heart","rect":[0,0,22,22]}`，圖標名與列表設定的 `icon` 子項一致，可以指定填充和描邊樣式。

常駐小畫布同時需要 `island.pinned` 和 `status.canvas`，返回 `{"pinned":{"canvas":{"width":126,"nodes":[…]}}}`。高度固定為 22，寬度為 1–156，最多 60 個節點。可以使用綁定、數字翻動和橫向滾動文字。橫向滾動給 `text` 節點配置 `max_width` 與 `loop.kind:"flow"`，並放進帶 `clip` 的分組。多種常駐內容可將 `canvas` 寫成畫布數組，隨宿主每 8 秒的節奏輪換。[常駐畫布樣例](../../spike/examples/canvas-pinned-demo/plugin.json)

具有 `storage` 能力的插件可從「已安裝」的「···」選單導出或導入數據。備份 JSON 儲存 `id`、`dataVersion` 與 `storage`。導入會用備份替換當前數據，並核對插件 ID、數據版本及 256 KiB 的存儲限額。

座標須為 -2000 至 2000 的有限數字，字號為 6 至 96，文字總計最多 2000 字。畫布及其分組總計最多 1500 個圖元，嵌套 8 層，路徑命令合計最多 4000 個。畫布或其中的圖片引用不合規時，頁面顯示「插件畫面有誤」，開發者工具顯示原因。宿主先畫出完整首幀，後續繪製超過 4 毫秒預算時沿用緩存畫面，並在開發者工具記錄。

`canvas` 為對象時，展示結果上限為 64 KiB，私有存儲另計。宿主預設使用島仔字體，畫布隱私遵循島仔的「截圖和錄屏中隱藏」設定。

##### 實時綁定

把實時值寫在畫布的 `binds` 中。島仔計算值並按最近的可見變化安排重繪，腳本保持原有刷新週期。

| 來源 | 聲明與結果 |
| --- | --- |
| 計數 | `{counter:{start_value:0,per_second:0.02,start_at:1795400000,decimals:2}}`。同時提供數字和格式化文字，小數位為 0 至 4。開始前保持起始值，可用 `until` 指定停止時間 |
| 倒計時 | `{countdown:{target_at:1795431600,format:"HH:mm:ss"}}`。數字為剩餘秒數，也支援文字格式 `d天 HH:mm`。到點數字歸零，文字顯示「已結束」 |
| 進度 | `{progress:[1795392000,1795424400]}`。在開始到結束之間從 0 到 1，區間外保持端點值 |
| 指針 | `{clock_angle:{zone:"America/New_York",hand:"hour"}}`。分鐘粒度，`hand` 選擇 hour 或 minute，12 點為 0 度，順時針增加 |
| 現在 | `{now:"HH:mm",zone:"Asia/Shanghai"}`。提供時間文字，也支援 `HH:mm:ss`。省略 `zone` 時使用本機時區 |
| 日夜 | `{daylight:{zone:"Asia/Shanghai",hours:[6,18]}}`。當地時間前閉後開，區間內為 1，區間外為 0。結束小時小於開始小時表示隔天結束 |

時間戳用 Unix 秒，時區用 IANA 名稱。文字用 `{名字}` 插入格式化值，原樣保留未匹配的花括號內容。數字位置可用 `{bind:"work"}` 讀取數字，也可用 `{bind:"work",map:[0,210]}` 將 0 至 1 映射到給定區間。顏色可用 `{bind:"day",pick:["#2E64B5","#FFC43D"]}`，數值達 0.5 時使用第二項。比如在 `rect` 的寬度綁定進度時，島仔可以按顯示像素精度更新，而 `now:"HH:mm"` 到下一分鐘才更新。

##### 動畫與操作

- 文字節點添加 `roll:true` 後，變化的數字在 220 毫秒內向上翻動。給節點添加穩定的 `id`，宿主可跨輸出匹配同一節點。
- `animate:{ms:300,ease:"spring"}` 可讓位置、尺寸、角度、透明度和顏色平滑過渡，新增或移除節點會淡入淡出。時長 120 至 1200 毫秒，緩動支援 linear 和 spring。
- `loop:{kind:"pulse",period_ms:1600}` 支援 pulse、flow、bob、spin。週期至少 800 毫秒，一個畫布最多 8 個循環節點，循環最多 30 幀每秒。帶 `id` 的節點可用 `burst:"confetti"` 播放一次彩紙。
- `{"pet":{"at":[226,24],"size":28,"mood":"expressions","motion":"run","id":"runner"}}` 由宿主繪製並沿用使用者的表情包。尺寸為 12 至 96，`at` 是角色中心並支援綁定。表情可選輪換（`expressions`）、睡覺（`sleeping`）、開心（`happy`）或現有表情名。動作為原地跑跳（`run`）、浮動（`bob`）或靜止（`none`）。每個展開畫布最多一隻，活動時計入循環預算。
- `hit:{action:"select"}`、`hit:{open:"https://example.com"}`、`hit:{copy:"複製這段"}` 三選一，複用標準組件交互規則。需要聲明 `action` 能力，連結須在清單的 `network.hosts` 內，複製最多 2000 字。每畫布最多 64 個點按區域，輸出更新後舊點擊失效。
- 節點可用 `hover:{opacity:1,lift:2,fill:"$accent",hint:"複製"}` 調節懸停外觀，並顯示最多 40 字的單行提示。提示限制在畫布可見區域內。折線可用 `tips:[{x:140,label:"12:40",value:"4,106.2"}]` 聲明宿主的懸停讀數。
- 帶 `clip` 的分組可加 `scroll:{axis:"y",content:420}`。滾輪在區域內移動內容，插件刷新會保留滾動位置。

[畫布圖元樣例](../../spike/examples/canvas-demo/plugin.json)展示圖元、漸變、包內圖片，以及畫布與標準組件混排的方式。腳本的 `canvas` 字段格式見[清單格式檢查表的 `$defs.canvas`](sdk-v1.schema.json)。

## 6. 數據能留多久

使用者可以停用插件一段時間再回來。宿主根據插件 ID 分開儲存設定和私有數據，每次成功調用只替換本插件的數據。保留數據卸載後，從原清單位置重裝開發包，或從原測試集市目錄重裝同 ID 試用包，可以找回原值。

本地版把安裝快照放在配置目錄的 local-plugins。正式版擬把只讀程序放在 `%APPDATA%\Daozai\plugins\<ID>\<版本>`，使用者內容放在同級 plugin-data 的對應 ID 目錄。升級時在副本上按 dataVersion 轉換舊數據，成功才切換，失敗繼續用舊包和舊數據。在模塊設定頁導出的文件包含使用者內容和數據版本，密鑰和文件授權留在本機。卸載撤銷授權、刪除憑據，選擇保留的使用者內容由通過身份核對的同一插件認領。

## 7. 插件多久刷新一次

作者在清單填寫間隔，宿主負責安排工作。當前定時卡支援 60 至 86400 秒，舊樣例缺省為 60 秒。首次啟用或修改設定會重新計算。界面讀取最近結果，每個插件同時最多一次調用，全局最多兩個後台計算。停用時畫面立即撤下，稍後返回的結果作廢。

QuickJS 每次計算限時 30 毫秒，腳本可分配的記憶體上限為 4 MiB。單次展示輸出最多 16 KiB，包含畫布時為 64 KiB，私有存儲另計，最多 256 KiB。普通刷新週期至少 60 秒，實時前台內容按需開放 1 秒。HTTPS 每插件最多 3 個來源，每次請求最長 10 秒，響應最多 256 KiB。擬保留 3 個固定插件卡位置和 1 個臨時位置，滿時讓使用者選擇替換。

連續三次腳本執行失敗後暫停並提示重試，是待實現的正式版規則。監控程序沒啟動時應提示打開依賴，設備離線時顯示上次更新時間並降低查詢頻率。兩種情況均屬於數據來源狀態，由宿主另行處理。

## 8. 作者交付哪些文件

從[示例目錄](../../spike/examples/payday)開始，複製清單、入口和封面。清單中的 `entry` 指向包內入口。ID 使用小寫作者名與插件名，以點分隔，最長 64 個字符。

version 是插件版本，api 是需要的接口主版本，dataVersion 是存儲格式版本。`api` 高於宿主支援的版本時，安裝會提示「需要更新島仔」。owner 決定設定歸屬，capabilities 列出使用的能力，permissions 列出授權事項。字段類型和可選值見[清單格式檢查表](sdk-v1.schema.json)。該表也描述設計中的能力，本地試用目前接受的組合見[本地安裝說明](local-install.md)。

每個插件都必須提交一張封面。清單必須聲明 `"cover": "cover.png"`，圖片放在包根目錄，使用有效 PNG，最大 256 KiB，最大 1920×1080。界面以 16:9 居中裁切，建議作者直接使用 16:9 並將主體放在畫面中間。缺失或無效的封面會阻止試裝、打包、提交和更新。舊版已裝插件繼續運行，原來缺圖的條目顯示佔位，升級後顯示新包的封面。

### 腳本設定與輸入（實驗）

`switch` 儲存為 `"true"` 或 `"false"`。`choice` 聲明 2 至 6 個 `{value,label}`，儲存所選 value。`number` 聲明 min、max、step，可加 unit、precision。`time` 使用 24 小時制 `HH:MM`。`weekdays` 使用逗號分隔的 1 至 7，從星期一開始。空字符串表示未選擇星期。

`text` 可聲明 `multiline: true`。輸入框隨內容長高，最多顯示 6 行，超出可滾動。Enter 換行，Ctrl+Enter 儲存設定。

`timezone` 設定用中文城市名、國家或地區名、IANA 時區代碼搜索，例如「紐約」「美國」「America/New_York」。儲存 IANA 時區名，例如 `America/New_York`。城市與地區數據沿用 Unicode CLDR 的中文名稱和 IANA 時區數據庫。

`list` 聲明 1 至 7 個子設定 `item`，用 `maxItems` 指定最多 20 行。子設定支援 text、choice、number、date、time、switch、timezone、icon、color。儲存為 JSON 數組字符串，每行對象的值仍為字符串，例如 `[{"city":"北京","zone":"Asia/Shanghai"}]`。腳本可用 `JSON.parse(settings.cities)` 讀取。

`icon` 與 `color` 用於列表子項，儲存名字字符串。`icon` 可選 `heart`（愛心）、`cake`（蛋糕）、`ring`（戒指）、`plane`（飛機）、`star`（星星）、`gift`（禮物）、`flag`（旗子）、`calendar`（日曆）、`coffee`（咖啡）、`coin`（硬幣）、`sun`（太陽）、`moon`（月亮）。

| `color` 值 | 對應顏色 |
| --- | --- |
| red | `#E26663` |
| orange | `#EF9A4F` |
| yellow | `#EBC253` |
| green | `#79AD67` |
| teal | `#4DADA5` |
| blue | `#6398E5` |
| purple | `#A07CD4` |
| pink | `#D880A6` |

`input.now` 是 Unix 毫秒，`input.tz` 提供本機 `{offset_minutes,name}`。清單聲明 `timezones:true` 後，`input.zones` 包含設定中出現的有效 IANA 時區，值為 `{offset_minutes,dst}`。SDK 複用項目原有的 `chrono-tz` 依賴，在本機換算偏移和夏令時。

清單 `data` 可列出包根目錄的 UTF-8 JSON，單個 64 KiB、合計 256 KiB。腳本通過 `input.data[文件名]` 讀取。安裝、測試集市和 `--plugin-pack` 都包含已聲明的數據。節假日示例自帶 2026 年數據，2027 年起由作者隨新版本按年更新。

帶 `timer` 的插件可返回 Unix 秒 `next_refresh_at`。下一次刷新取正常週期和指定時刻裡較早的時間，至少間隔 60 秒。`storage` 單獨限 256 KiB，其餘輸出限 16 KiB，包含畫布時為 64 KiB。寫入超限時保留原有數據，在插件詳情提示錯誤。

宿主檢查字段、ID、預設值、版本、入口和封面，並拒絕越出包目錄的文件。重啟時重新檢查安裝快照，出現無效記錄提示重新安裝。測試集市 ZIP 包含包根的 `plugin.json`、`cover.png`、清單指定的入口和數據文件。聲明 `status.canvas` 時，`--plugin-pack` 也會打包符合要求的 PNG 和 SVG，保留包內相對路徑。普通 ZIP 上限為 832 KiB，畫布版 ZIP 上限為 1376 KiB，畫布圖片仍須遵守 512 KiB 合計上限。宿主核對目錄中的大小與 SHA-256，直接在記憶體中讀取包內容。正式資源包和獨立模塊的字段見清單格式檢查表。

材質包包含 `plugin.json`、`cover.png`、一個大小不超過 64 KiB 的 `.wgsl` 入口，以及可選的文本說明文件。預設值必須是範圍內的有限數字，步長為正且不大於範圍寬度。`unit` 最多 4 個字符，僅顯示在滑塊數值後。`precision` 為 0 至 3 的整數，省略時按 `step` 的小數位數顯示。顯示格式不改變參數值。控件彈簧剛度範圍為 0.2 至 3，阻尼範圍為 0.1 至 1。

## 9. 島仔升級後還能用嗎

建議先穩定插件身份、基本設定、卡片、存儲的含義。新增可選字段要給舊包保留預設值。更改單位或刪除已承諾的字段，就提升接口主版本，讓新舊規則分別處理對應的包。

獨立模塊、來源、實時歌詞和事件先標實驗。遇到不兼容的實驗能力時，島仔應列出受影響的部分。更新申請更多權限時先保留可用舊版，使用者同意後切換。六個月的新舊主版本並行期建議只適用於已定稿的穩定接口，期限待確認。發版前用舊版官方插件的原包，在新宿主實測安裝、數據恢復、舊設定、主題和啟停。遇到不支援的版本時保留數據並提示升級。

## 10. 作者如何試裝和檢查

當前「開發者工具」支援選擇本地清單安裝。兩個樣例可分別啟用，在所屬設定頁改日期，重啟檢查，再卸載並選擇保留或刪除數據。作者改腳本後可從原位置重新安裝。[可執行的試用步驟](local-install.md)

開發包與集市成品分別識別。Windows 上啟用 `sdk-js` 的試用版提供 `--plugin-test`、`--plugin-pack` 和 `--plugin-submit`。使用相同的清單規則，並用預設參數試跑一次插件。校驗失敗給出錯誤並返回非零退出碼。作者可先在「開發者工具」試裝，在改動後從原清單位置重新安裝。自動重載和不同外觀的預覽是後續工作。[測試集市試用步驟](local-install.md#測試集市)

## 11. 怎樣把原有代碼拆出來

### 本地開發調試（實驗）

在「設定 → 插件 → 開發者工具」打開「監視文件變化」。儲存本地安裝的插件清單、入口或包內數據文件後，島仔會自動重載並保留私有數據。腳本可調用 `console.log(...values)`，每次最多 20 行、每行 200 字。開發者工具保留最近 100 行，日誌只在本次運行期間可見。

運行 `daozai.exe --plugin-test plugin.json` 可試跑 JavaScript 插件的 `refresh`，查看返回值、運行耗時、報錯位置及能力自檢。追加 `--input mock.json` 傳入可選的 `settings`、`storage`、`now`（Unix 毫秒）和 `sources`。只讀 `input.source` 的插件可用 `{"sources":{"default":{"status":"ready","text":"模擬響應"}}}` 試跑。

先保留業務算法和測試，再把輸入、展示結果和存儲寫成函數。喝水提醒原有設定限制為 5 至 180 分鐘，可在清單的數字設定中聲明範圍，函數讀取使用者輸入並返回展示內容。

```js
function refresh({settings}) {
    const minutes = Math.min(180, Math.max(5, settings.minutes));
    return {card: "提醒間隔 " + minutes + " 分鐘", storage: {minutes}};
}
```

舊額度草案已標記廢棄。遷移時從舊命令中提取餘額計算，把接入端輸出接到使用者授權的本地文件或迴環接口，把計算改成 refresh 並返回額度數據。舊憑據由接入端繼續管理。[舊草案說明](README.md)

可直接交給 AI 的提示詞：

> 閱讀我的 PR、使用場景和島仔 SDK。列出可複用的業務邏輯和已有測試。選擇一個設定歸屬，把定時、按鈕和事件寫進清單。用 refresh、action 或 event 接收輸入並返回結果。生成最小包和模擬數據，標清哪些能在當前宿主試裝、哪些需要新接口。驗證啟停、重啟和卸載恢復。保留原作者與素材許可。

#### 狀態頁交互、專注事件和提醒

在清單中同時聲明 `action` 和 `status.page`。`status.actions` 最多 3 個按鈕，每個按鈕填寫 `{id, label}`，標籤 1 至 6 字。`status.fields` 最多 3 個輸入框，每個填寫 `{key, label, value?, max, multiline?}`，標籤 1 至 28 字，`max` 為 1 至 200 的整數，`value` 是可選初始值。每組的 ID 或鍵須唯一，由小寫英文字母開頭，最多 40 個字符，可用小寫英文字母、數字、下劃線和連字符。自定義佈局用 `actions`、`fields` 選擇控件位置，預設佈局將控件放在讀數下方。

狀態頁字段預設單行。`multiline: true` 允許 Enter 換行，最多顯示 6 行，超出可滾動。Ctrl+Enter 觸發第一個交由腳本處理的 `status.actions` 按鈕，表單會保留換行。

列表最多 8 項，每項可寫 `{label, detail?, state?, action?, open?, copy?}`。`action` 是腳本動作 ID。按鈕與列表項都可加 `open` 或 `copy`，二者只能選一個。`open` 打開清單 `network.hosts` 聲明的 HTTPS 域名下的連結。`copy` 將最多 2000 字寫入剪貼板。當行既有腳本動作又有複製內容時，點正文由腳本處理，點行尾「複製」由宿主處理。

點擊腳本動作時，島仔在後台調用 `action({settings, storage, today, source?, action:{id, form}})`。`form` 是字段鍵到文字的映射，未填的字段為 `""`。按鈕和列表項只在生成它們的狀態結果仍有效時接受點擊。狀態刷新、停用插件、修改設定或關閉所屬模塊都會使對應的舊結果失效。

清單聲明 `event` 並列出 `"events":["focus.finished"]`。每次專注計時結束只投遞一次。同一個插件已有調用時，事件按順序等候。插件停用或所屬模塊關閉期間的事件不補發。全局函數 `event(input)` 收到與 `refresh` 相同的通用字段及 `event:{name, id, at, minutes}`，`at` 為 Unix 秒，`minutes` 為本輪配置的專注分鐘數。

聲明 `notice` 後，三個腳本入口都可返回 `notice:{id, title, body?}`。ID 為 1 至 128 字，標題為 1 至 40 字，正文可省略，最多 120 字。每個插件同一 ID 只顯示一次，60 秒內最多發出一條。提醒顯示和正文遵循島仔現有的通知開關。

三個腳本入口共用每插件單次運行的限制，以及 30 ms、4 MiB 和 16 KiB 的展示輸出上限，包含畫布時展示上限為 64 KiB。可在本地試裝[便籤樣例](../../spike/examples/sticky-notes/plugin.json)。

## 12. 投稿怎樣進入集市

### 測試集市

當前用本機目錄試用作者打包、提交，和使用者安裝、更新。作者校驗開發包、生成 ZIP，再提交到 `catalog.json` 所在目錄。每個插件 ID 在目錄索引中列一個最新版本。已經提交的版本 ZIP 保持原文件，發佈新版要遞增三段式版本號。

索引為 UTF-8 JSON，頂層 `format: 1`、`plugins` 數組。每項包含 `id`、`name`、`author`、`version`、`size`（字節數）和 `sha256`（小寫十六進制）。ZIP 文件名為 `plugin-<id>-<version>.zip`，和索引放在同一目錄。作者命令負責填充元數據和校驗值。

條目可選填 `revoked:["1.0.1"]` 標記撤回版本。宿主同步目錄時會停用已安裝的撤回版本，在「已安裝」提示撤回，並阻止從該目錄安裝版本。有更高的可用版本時可更新，更新後可重新啟用。

使用者在「設定 → 插件 → 已安裝 → 開發者工具」導入目錄，集市切到「本地測試」。安裝後在「已安裝」管理。升級從同一目錄領取同 ID、版本更高的包。更新驗證通過後才替換安裝快照，保留原啟用選擇、已儲存的設定和私有數據。新增字段使用新版預設值。校驗、試跑或兼容檢查失敗時保留舊包。

本地測試集市接受合規的 QuickJS 插件、外觀資源包、快捷切換插件、無線設備兼容包以及 GPU 材質。申請本地數據源的 JS 插件仍需使用者安裝後單獨授權。索引及 ZIP 應來自使用者信任的本機目錄。SHA-256 核對包是否與索引一致。

官方無線設備兼容包通過插件集市分發，在「已安裝」管理。ZIP 包含 `plugin.json`、`wireless-receivers.json` 和 `cover.png`，只為本體補充核實過的 2.4G USB 接收器型號。

### 正式分發

作者向獨立插件倉庫提交 PR，提供源碼、清單、封面、版本、變更、測試方法、許可和鏡像分發授權。維護者審核併合並後打包。發佈先把不可覆蓋的 ZIP 上傳到 Gitee，確認匿名下載的文件一致，再更新同目錄的 `catalog.json`。[插件投稿倉](https://github.com/Renly1994/xiaotang-daozai-plugins)

使用者打開「設定 → 插件 → 集市」時，島仔優先顯示本機儲存的目錄和統計，缺少緩存時自動載入一次。切換標籤和關閉詳情直接使用當前數據。點擊「刷新目錄」可重新檢查 Gitee 目錄和統計。斷網或刷新失敗時繼續顯示上次有效數據。安裝前核對文件大小和 SHA-256，升級驗證通過後才換包，保留已有數據和啟用選擇。驗收版讀取 `preview`，發佈版讀取 `stable`。[Gitee 下載鏡像](https://gitee.com/xiaotangtoken/xiaotang-daozai-plugins)

發佈者簽名仍待實現。

### 詳情、收藏與統計

集市索引可以為每項附上 `presentation`，由集市維護者填寫。插件簡介用 `description`，更新日期用 `updated_on`（`YYYY-MM-DD`），維護者帳號用 `maintainer`。官方發佈填 `official`，驗收樣例填 `sample`。官方插件顯示「官方」，舊版官方清單的作者「島仔」顯示為「小唐」。頭像可用同目錄的 `avatar-*.png`，在 `author_avatar` 中填寫文件名。先發布頭像文件，再更新索引。缺頭像時顯示作者名稱首字。

使用者可從集市或詳情收藏插件，在集市的「收藏」篩選裡查看和取消。本機收藏按插件 ID 儲存，後續版本和插件啟停沿用同一收藏。歷史安裝條目如果沒有封面，管理頁顯示佔位，啟停、設定和卸載仍可用。

集市的下載量和收藏量取自統計服務，預覽與正式目錄分別計數。下載量從服務啟用起統計通過校驗的安裝包下載，同一事件補報只計一次。收藏量按匿名安裝的當前收藏狀態彙總，取消後扣減，已有本機收藏隨目錄同步補報。上報成功後，集市直接使用返回的統計數據。斷網時仍可管理本機收藏，待聯網後補報。集市尚未收到統計時顯示「—」。上架新插件或版本時須在統計服務登記對應頻道。

### 插件回饋

從詳情或「已安裝」的「···」打開「插件回饋」。普通問題和建議會在獨立插件倉打開 GitHub 草稿，預填插件 ID、插件版本和島仔版本，使用者補充訊息後自行提交。安裝或更新異常由島仔維護者跟進。沒有 GitHub 帳號時可複製回饋訊息轉交。安全或隱私問題選擇「複製並打開私密報告」，登錄 GitHub 後粘貼插件訊息，補充並提交給有權限的維護者。[GitHub 私密報告](https://github.com/Renly1994/xiaotang-daozai-plugins/security/advisories/new)

插件上架時，倉庫維護者在 `.github/plugin-maintainers.json` 登記作者或接手維護的 GitHub 帳號。普通問題單提交後，倉庫按插件 ID 自動提及對應帳號。作者修復時提高版本號並提交關聯原問題的 PR。維護者審核、發佈和同步鏡像後，在原問題下告知可用版本。

## 13. 需要小唐決定的事

這三項決定的是正式對外承諾，當前開發包按本文的全新調用方式運行。

| 決定 | 可選方式與建議 |
| --- | --- |
| 函數如何運行 | 每次全新運行，或為每個插件長期保留環境。建議前者，恢復只依賴輸入和已儲存數據，首版文檔與樣例統一採用 |
| 怎樣讀取使用者指定的本機軟體狀態 | 監控可導出文件供使用者選擇，也可提供固定的迴環 GET 接口。拓竹等接入端需說明依賴和接口地址，端口更改後由使用者重新授權 |
| 舊額度命令如何退出 | 直接廢棄，或為已有使用者限期提供遷移期。建議停止接受新命令包，已有作者遷往受控來源接口。兼容期長度由小唐決定 |

生產階段還需確認穩定接口的支援期限、併發與空間上限、獨立進程、卸載預設選擇、換維護者時的數據認領及市場簽名密鑰管理。

### 後續工作

以下內容還需要實際開發和試裝。

可編輯的獨立頁面用便籤驗證。後續試驗臨時卡到期撤回、日程跨來源去重、切歌時清舊歌詞，以及重複通知只提醒一次。用資源換膚包、紀念日驗證切換恢復與跨模塊輸出。評估獨立低權限進程對故障隔離和啟動的影響。

### 名詞解釋

讀清單或查看錯誤提示時，可以在這裡對照字段的含義。

- **宿主**指安裝和運行插件的島仔。**SDK** 是作者接入島仔所用的約定、樣例和工具。
- **純函數**指只用輸入算出結果的函數。兩次調用之間要儲存的內容放在 `storage`。
- **QuickJS** 是宿主內運行 JavaScript 的程序。**KiB / MiB** 是容量單位，1 KiB 等於 1024 字節，1 MiB 等於 1024 KiB。
- **清單**是作者提交的 plugin.json。**ID** 是插件的固定名稱。
- **接口**是宿主和插件約定的輸入輸出。**主版本**用來區分不兼容的規則。
- **JSON** 是由文字、數值、列表等組成的數據格式。**Schema** 是機器可讀的字段檢查表。
- **PR** 是向倉庫提交代碼或插件的審核請求。**SHA-256** 用來檢查文件內容是否改變。**簽名**用來確認發佈者身份。

# A：畫布核心

分支 `feat/plugin-canvas`，從本地 `main` 開出。共同規則見 [README](README.md)。

本任務交付「能畫出來」：解析和校驗畫布、靜態繪製、主題令牌、包內圖片、和標準組件混排。時間綁定、動畫、點按由 B 做，但數據結構要在這裡一次定好，B 只補求值和行為。

## 先讀

- `src/plugin_status.rs`：`PluginStatus`、`StatusLayout`、`StatusComponent` 的解析和上限。
- `src/plugin_status_ui.rs`：狀態頁如何按佈局繪製，`draw_layout`、`component_height`。
- `src/plugin_status_components.rs`、`src/plugin_status_realtime.rs`：現有組件和實時組件。
- `spike/local.rs`：`Output` 解析、`render_input`、輸出大小上限。
- `spike/capabilities.rs`：能力表。
- `spike/appearance_assets.rs`：資源包裡 SVG 和 PNG 的讀取與校驗，圖片部分可以複用。

## 清單和輸出

- 新能力 `status.canvas`，運行環境 js，作為輸出加進能力表，要求同時聲明 `status.page`。
- `refresh`、`action`、`event` 的返回值新增字段 `canvas`，與 `status` 同級。
- `statusLayout.components` 新增類型 `{"type":"canvas"}`。它在哪個位置出現，畫布就畫在哪裡；如果佈局裡只有它，畫布佔滿整頁內容區。
- 畫布的整體結構如下：

```json
{
  "height": 196,
  "nodes": [ … ],
  "binds": { … }
}
```

- `height`：40 到 360，決定內容區高度，島的展開高度隨之變化。
- `binds`：本任務只解析並保存，不求值，詳見 B。
- 含畫布的輸出上限從 16 KiB 放寬到 64 KiB，其餘輸出仍是 16 KiB。

## 圖元

每個節點是一個對象，鍵名決定類型。

| 類型 | 寫法 | 說明 |
| --- | --- | --- |
| 矩形 | `{"rect":[x,y,w,h], "r":8}` | `r` 可以是數字，也可以是 `[左上,右上,右下,左下]` |
| 圓 | `{"circle":[cx,cy,r]}` | |
| 弧 | `{"arc":{"c":[x,y],"r":40,"start":-90,"sweep":210}}` | 角度單位為度，0 度朝右，順時針為正 |
| 直線 | `{"line":[x1,y1,x2,y2]}` | |
| 折線 | `{"points":[[x,y],…], "area":118}` | `area` 給出時，填充到這條水平線，用於面積圖 |
| 路徑 | `{"path":"M0 0 L10 10 Q… C… Z"}` | 只支持 M、L、H、V、Q、C、Z，大小寫都要支持 |
| 文字 | `{"text":"距 元旦","at":[x,y],"size":11,"weight":700,"align":"left"}` | `at` 是基線位置；可選 `max_width`、`lines`（最多幾行）、`font`（見 C）|
| 圖片 | `{"image":"cover.png","rect":[x,y,w,h],"fit":"cover"}` | 只能引用包內 PNG 或 SVG，`fit` 可選 `cover`、`contain` |
| 分組 | `{"group":[…], "clip":[x,y,w,h], "translate":[x,y], "rotate":15, "scale":1.2}` | 變換作用於整組 |

通用樣式字段：

- `fill`、`stroke`：顏色或漸變。漸變寫成 `{"linear":[[x1,y1],[x2,y2]],"stops":[[0,"#FFE58A"],[1,"#00000000"]]}`，`radial` 寫法同理。
- `width`：線寬。`cap` 可選 `round` 或 `butt`。`dash`：`[實線長, 間隔長]`。
- `opacity`：0 到 1。
- `shadow`：`sm`、`md`、`lg` 三檔，由宿主統一繪製。
- `glow`：`true` 或顏色，宿主用同色模糊外發光繪製，參考性能頁的發光圓環。
- `id`：可選。B 做動畫和點按時用它對應前後兩次輸出裡的同一個節點。

## 繪製

- 用 egui 的 `Painter` 和 epaint 實現。路徑和貝塞爾曲線走 epaint 的曲線類型；漸變用頂點著色的網格；文字走 galley，字號和字重自由。
- 座標按縮放比例換算。100% 到 200% 縮放下線條不能發虛，文字不能越出 `max_width`。
- 主題令牌按當前皮膚取色，深淺主題都要正確。
- 圖片在安裝時讀入並緩存成紋理。每個包的圖片合計不超過 512 KiB，單張不超過 1024×1024。
- 靜態節點按「畫布內容 + 縮放比例」緩存細分結果，內容不變時不重新計算。

## 校驗與上限

宿主拒收不合規的畫布，在開發調試面板寫明原因，頁面顯示「插件畫面有誤」佔位，不能崩潰。

- 節點總數（含分組內）最多 1500，嵌套最多 8 層。
- 路徑命令合計最多 4000 個，文字合計最多 2000 字。
- 所有數字必須是有限值，座標在 -2000 到 2000 之間，字號在 6 到 96 之間。
- 只能引用包內存在的圖片。路徑裡出現 `..` 或絕對路徑直接拒收。
- 單幀繪製超過 4 毫秒時，改用上一幀的緩存圖像，並在開發調試面板提示作者。

## 樣例與文檔

- 在 `spike/examples/canvas-demo/` 加一個演示插件，用到上面所有圖元、漸變、陰影、發光和一張包內圖片。
- 在 `sdk-v1-design.md` 新增「自繪畫布」小節，在 `sdk-v1.schema.json` 補上字段定義。

## 驗收標準

- 單元測試覆蓋：
  - 每種圖元的解析；
  - 每條上限和非法值的拒收；
  - 主題令牌取色；
  - 圖片路徑越界拒收；
  - 混排佈局的高度計算。
- egui 繪製測試：演示插件在兩種以上皮膚、深淺主題、100% 和 200% 縮放下渲染，不 panic，文字不越界。
- 現有 statusLayout 插件不用改就能照常顯示。
- 靜止的畫布頁不產生持續重繪。

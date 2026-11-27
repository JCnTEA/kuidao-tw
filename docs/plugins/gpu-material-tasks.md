# GPU 材質能力：分階段任務

設計見 [gpu-material.md](gpu-material.md)。各階段按順序做，每個階段開一個新對話，上一階段審查通過後再開下一階段。開新對話時直接發：

> 按 `D:\A-TangHao\REA\Windows小工具\01-專項工程\GPU材質\docs\plugins\gpu-material-tasks.md` 做第 N 階段。

## 所有階段通用

- 工作樹：`D:\A-TangHao\REA\Windows小工具\01-專項工程\GPU材質`，分支 `feat/gpu-material`。只在這個工作樹裡改，不碰 `main` 和其他工作樹。
- PR #235 原始代碼在分支 `test/pr235-liquid-glass`，用 `git show test/pr235-liquid-glass:<路徑>` 閱讀，或打開只讀參考工作樹 `D:\daozai-build\tmp\pr235-preview`。不要在參考工作樹裡改代碼或提交。
- 編譯、測試遵守根目錄 AGENTS.md：持 `Local\Daozai.Cargo` 鎖排隊，`CARGO_BUILD_JOBS=1`。release 編譯需要 `RUST_MIN_STACK=67108864`。
- 插件相關代碼受 `sdk-js` 特性控制，檢查和測試都加 `--features sdk-js`。材質相關新代碼放在新的 `sdk-material` 特性後面，測試時兩個特性一起開。
- 設計稿裡的接口（清單字段、`Frame` 佈局、prelude 函數）是實驗接口，本輪按現有內容實現。需要改動時先在回報裡說明原因，不要自行改設計稿。
- 每個階段結束：跑完相關測試並提交。在最後一條回覆裡列出提交號、改動文件、測試命令和結果、沒做完或有疑問的地方，交給審查。
- 是否出包按各階段說明，不按 AGENTS.md 的預設出包。

## 第 1 階段：同步 main，移植透明交換鏈和抓屏底座

目標：本體具備抓取島體背後底片和透明交換鏈的能力，沒有材質時完全不運行，外觀和行為與 `main` 一致。

1. 把 `main` 合進 `feat/gpu-material`。插件分支和 main 的衝突，參考 `test/acceptance` 上提交 `a3f5c8d` 的解決方式：插件設定頁用 `SETTINGS_TAB_PLUGINS = 17`，`render_page` 和 `set_page_visible` 兩邊都保留。
2. 移植 vendor 補丁，從 `test/pr235-liquid-glass` 取：
   - `vendor/wgpu-hal`（整個目錄，與上游 24.0.4 只差 `auxil/dxgi/conv.rs`、`dx12/adapter.rs`、`dx12/mod.rs`）；
   - `vendor/egui-winit`（與上游 0.31.1 只差 `src/lib.rs` 的無重定向位圖那幾行）；
   - `vendor/egui-wgpu/src/winit.rs`、`src/lib.rs`、`Cargo.toml` 的 DirectComposition 改動，以及 `vendor/eframe/src/native/run.rs`；
   - 根 `Cargo.toml` 裡對應的 `[patch]`、路徑依賴和 windows 特性。

   main 上可能有 #246 相關的 eframe 改動，合併時保留 main 的版本，再疊加 PR 的改動。
3. 新建 `src/material/backdrop.rs`：移植 PR 的 `src/wgc.rs` 和 `src/gpu_glass.rs` 裡的抓屏部分（`capture`、`grab_raw`、乾淨底片 `CleanPlate`、`compose_with_plate`、背景亮度、捕獲排除），整理成 `Backdrop` 對外接口。去掉 `DAOZAI_LIQUID_*` 調試環境變量，診斷日誌寫到 `diagnostics/material.log`。
4. 透明交換鏈只在材質生效時啟用。PR 裡用 `DAOZAI_DCOMP` 環境變量控制，改成由 `material` 模塊在啟動時根據「兼容渲染已開且有生效的材質」決定。沒有材質時不設無重定向位圖，走原來的路徑。
5. 測試：
   - 現有測試全部通過（`cargo test --lib --features sdk-js`，以及加上 `sdk-material` 再跑一遍）；
   - 給底片幾何換算、尺寸變化不復用舊紋理、失敗後退回這幾處補單元測試。
6. 不出包。

## 第 2 階段：通用材質管線和控件登記

目標：本體能載入一段符合接口的 WGSL，疊加在任何皮膚上繪製。用內置調試材質驗證。

1. 新建 `src/material/prelude.wgsl`：按設計稿定義 `Frame`、綁定和 `param`、`rr_sdf`、`notch_sdf`、`body_sdf`、`sample_backdrop`。函數體從 PR 著色器搬，計算保持一致。
2. 新建 `src/material/pipeline.rs`：
   - 拼接 prelude 和插件代碼，在 `push_error_scope` 裡編譯；
   - 建綁定組、打包 `Frame`（`#[repr(C)]`，每項 16 字節對齊，和 WGSL 佈局逐項對應，補一個佈局大小的單元測試）；
   - 掛 egui paint callback，預乘 alpha 混合方式與 PR 相同。
3. 新建 `src/material/controls.rs`：
   - 頁面繪製時調用 `material::control(id, rect, radius)` 登記控件玻璃；
   - 宿主按清單的剛度和阻尼算彈簧位移、懸停和按下狀態。
   
   PR 在 `app.rs` 的 `Ctl` 枚舉裡列出的 13 個控件（播放、切歌、音量、歌詞、齒輪、翻頁箭頭、進度圓鈕、添加帳號、信箱去添加、日程編輯等），逐個在對應頁面的繪製代碼裡登記。
4. `src/material/mod.rs`：
   - 當前材質的載入、啟停、每幀入口；
   - 靜止 15 Hz、2 分鐘後 1 Hz 深休眠；
   - 連續 120 幀材質通道超過 4 ms 自動停用；
   - 任一步失敗永久退回普通繪製並記日誌。
5. 內置調試材質 `src/material/debug.wgsl`：玻璃內原樣輸出底片，控件區域描一圈邊，用來檢查抓屏對齊和控件位置。只能通過測試或 `DAOZAI_MATERIAL_DEBUG=1` 啟用，不出現在設定裡。
6. 測試：
   - 著色器拼接後能通過 naga 校驗（單元測試裡用 wgpu 的 naga 解析 prelude 加調試材質）；
   - 寫錯的著色器被拒絕且不崩潰；
   - 關閉材質時渲染結果與 main 相同（沿用現有 UI 截圖測試）。
7. 出包：合進 `test/acceptance`，編測試版替換桌面測試版。回報裡寫明怎樣用 `DAOZAI_MATERIAL_DEBUG=1` 打開調試材質查看對齊效果。

## 第 3 階段：接入插件 SDK

目標：材質能作為插件安裝、啟用、調參和停用。

0. 第 2 階段審查遺留：停靠方向目前用 `@binding(3)` 的 `DaozaiHost` 傳入，插件著色器也能讀到它，等於多了一個未寫進接口的字段。改為放進 `Frame.count.y`（0 頂部、1 左側、2 右側），刪除 `binding(3)` 和 `DaozaiHost`，`Frame` 仍為 1024 字節。同步更新設計稿 `Frame` 註釋，`prelude.wgsl` 裡 `body_sdf` 改讀 `frame.count.y`，補一條單元測試確認插件聲明 `binding(3)` 也會被拒絕。

1. 清單：
   - `spike/local.rs` 的校驗接受 `runtime: "material"`、`owner: "appearance"`、`capabilities` 含 `appearance.material`；
   - 新增 `material` 字段（`controls`、`spring`）和 `number` 設定類型（`min`、`max`、`step`，最多 32 個）；
   - 包內只允許 `plugin.json`、一個 `.wgsl` 入口（≤ 64 KB）和說明文件。
   
   同步更新 `docs/plugins/sdk-v1.schema.json` 和 `docs/plugins/sdk-v1-design.md` 的能力表。
2. 啟用流程：
   - 首次啟用時確認 `screen.backdrop` 權限，說明文字見設計稿；
   - 兼容渲染沒開時提示開啟並重啟，期間插件顯示為等待狀態，基礎皮膚不變；
   - 同一時間只啟用一個材質，啟用新的會停用舊的。
3. 設定：
   - 「設定 → 外觀」裡在基礎皮膚下面加「材質」一項，列出已安裝的材質插件和「無」；
   - 選取後按清單生成參數滑塊，改動實時生效；
   - 「我的插件」頁同樣顯示材質插件。
   - 遵守 AGENTS.md 和使用者偏好：不刪現有設定項和說明，狀態用顏色和動效表示，不加文字標籤。
4. 截圖排除：`material/backdrop.rs` 的 `CaptureExclusion` 目前直接調用 `SetWindowDisplayAffinity`，和 `native.rs` 裡 #197 的 `capture::Exclusion` 管的是同一個窗口標記。改為統一經由 `capture::Exclusion`，避免兩處互相覆蓋。材質生效期間強制排除捕獲，「截圖和錄屏隱藏島仔」開關顯示為已開啟並附說明，停用後恢復使用者原來的選擇。這一條是設計稿「待確認的決策」第 1 條的建議方案，回報時單獨列出實現方式供審查。
5. 測試：
   - 清單校驗（合法、缺字段、超範圍、非 number 設定、包內多餘文件）；
   - 啟停後設定恢復；
   - 權限未確認時不啟用；
   - 兼容渲染關閉時不啟用；
   - 截圖排除的恢復。
6. 不出包，第 4 階段一起出。

## 第 4 階段：液態玻璃插件包和效果對比

目標：液態玻璃作為第一個材質插件，效果與 PR #235 預覽版一致。

1. 新建 `spike/examples/liquid-glass/`：
   - `plugin.json`：作者「輕九九九（@cayww）」，參數和預設值取自 PR #235 `model.rs` 的 `liquid_*` 預設值：

     | 參數 | 預設值 |
     |---|---|
     | 折射 | 48 |
     | 色散 | 8 |
     | 內部模糊 | 1.5 |
     | 折射頻寬 | 87 |
     | 白邊寬度 | 1.5 |
     | 白邊不透明度 | 0.9 |
     | 形變 | 1.0 |

     彈簧剛度 1.0、阻尼 0.5。
   - `material.wgsl`：從 PR 的 `gpu_glass.rs` 著色器移植 `fs` 及其輔助函數，改為 `material()` 入口，專用 uniform 改從 `param()` 讀取，幾何和底片改用 prelude 提供的函數。
2. 效果對比：
   - 同一壁紙、同一頁面、同一參數下，分別用參考工作樹裡的預覽版（`D:\daozai-build\tmp\打開液態玻璃預覽.cmd`）和測試版截圖；
   - 至少覆蓋音樂頁展開、收起膠囊、控件按下三種狀態；
   - 差異明顯時先修正移植，並在回報裡附對比圖路徑。
3. 測試：
   - 插件包通過清單校驗並能安裝、啟用、停用；
   - 著色器通過 naga 校驗；
   - 關閉插件後外觀回到原皮膚。
4. 出包：合進 `test/acceptance`，編測試版替換桌面測試版。在回報裡寫明安裝插件包的步驟，讓使用者在測試版裡試用。插件包只用於本機驗收，不發佈到插件市場。
5. 接口回饋：回報裡單列「接口問題」一節，作為定插件技術規範的依據。寫明移植中遇到的不夠用或彆扭的地方，例如：
   - 控件 16 塊、參數 32 個是否夠用；
   - `Frame` 缺少或多餘的字段；
   - prelude 函數是否需要增減；
   - 彈簧參數放在清單裡是否合適；
   - 參數滑塊的類型和範圍表達是否夠用。
   
   每條寫現狀、問題和建議，不在本階段自行改接口。

## 第 5 階段：補齊與預覽版的效果差距

目標：液態玻璃插件與 PR #235 預覽版在投影、按鈕和文字樣式、動態形變、明暗切換、各控件透鏡倍率上一致。設計稿已補充 `material.shadow`、`material.foreground`、`Frame.motion` 和控件角色，本階段按設計稿實現。

1. 先把 `main` 合進 `feat/gpu-material`。`main` 新增了底部停靠，按 `test/acceptance` 上合併 `41e8b6f` 之後的 `prelude.wgsl` 補齊底部停靠的輪廓換算，讓功能分支和驗收分支一致。
2. 投影：
   - 清單校驗接受 `material.shadow`（0 至 1.5）；
   - 宿主繪製島體投影時，有材質且聲明瞭投影強度就用它，否則沿用基礎皮膚。參照 PR `app.rs` 中 `skin_shadow` 和 `shadow_k` 的算法，PR 液態皮膚的係數是 1.0；
   - 液態玻璃清單寫 `"shadow": 1.0`。
3. 前景樣式：
   - 清單校驗接受 `material.foreground`（`"skin"` 或 `"glass"`）；
   - `"glass"` 時：
     - 宿主不畫基礎皮膚底色；
     - 頁面按鈕、卡片走玻璃畫法。把 PR 中 `glassy()` 為真時各頁面的分支（`pages.rs` 約 12 處、`paint.rs` 的 `glass_chip` 等）移植為宿主的通用前景樣式，判斷條件改為「當前材質聲明瞭 `foreground: "glass"`」，不出現 `Liquid` 字樣；
     - 文字和圖標的調色板隨 `motion.y` 在 `GLASS_DARK` 和 `GLASS_LIGHT_READABLE` 之間切換，對應 PR 的 `skin_palette`；
   - 顯示器拓撲剛變化、抓屏處於穩定期時，按 PR `paint_body` 的做法臨時畫純色玻璃底；
   - 液態玻璃清單寫 `"foreground": "glass"`。
4. `Frame.motion`：
   - 末尾追加 `motion`，`Frame` 改為 1040 字節，更新佈局測試；
   - `motion.x` 由宿主根據展開程度逐幀求導得出；
   - `motion.y` 用 PR `liquid_k` 的滯回和平滑算法計算。原 `liquid_k` 是 `App` 字段，改為材質模塊持有。
5. 控件角色：
   - 在 `material/controls.rs` 定義角色編號，至少覆蓋 PR `Ctl` 枚舉的 13 個控件，寫入 `ctl_spring.w`；
   - 控件超過 16 塊時按設計稿的優先級保留；
   - 把編號表作為附錄追加到設計稿末尾。
6. 著色器：按 PR `gpu_glass.rs` 補回第 4 階段近似處理的部分：
   - 展開速度驅動的形變（讀 `motion.x`）；
   - 按控件角色區分的透鏡放大倍率和折射倍率，取值照 PR `glass_controls` 裡各控件的 `mag`、`refract`；
   - 背景明暗相關的材質深淺（讀 `motion.y`）。
   
   著色器仍不超過 64 KB。
7. 效果對比：
   - 重做第 4 階段的三態對比（音樂頁展開、收起膠囊、按住齒輪），另加兩組：
     - 展開動畫中途一幀；
     - 淺色壁紙下的音樂頁，檢查文字轉為深色。
   - 對比時基礎皮膚任選。`foreground: "glass"` 生效後，結果不應再受基礎皮膚影響。
   - 在回報裡附對比圖路徑，逐項說明是否一致。仍有差異的寫明原因。
8. 測試：
   - 清單校驗覆蓋 `shadow`、`foreground` 的合法值和非法值；
   - `Frame` 佈局 1040 字節；
   - 明暗滯回不在閾值附近來回跳；
   - 控件超過 16 塊時的保留順序；
   - 停用材質後前景樣式、投影、底色全部回到基礎皮膚，沿用第 4 階段的停用對比；
   - 完整迴歸通過。
9. 出包：合進 `test/acceptance`，編測試版替換桌面測試版。回報裡寫明液態玻璃插件包需要重新安裝（清單有變化）。

## 第 6 階段：落地 v1 定稿新增項

目標：設計稿已改為《GPU 材質接口規範 v1》定稿。本階段實現定稿時新增或明確的幾項，讓代碼與規範逐條一致。規範是本階段的唯一依據，實現中發現規範寫不通時，先在回報裡說明，不自行改規範。

1. 先把 `main` 合進 `feat/gpu-material`。
2. 清單：
   - `settings[]` 支援可選的 `unit`（最多 4 個字符）和 `precision`（0 至 3）；
   - 設定頁滑塊按 `precision` 顯示小數位，不寫時按 `step` 的小數位，數值後顯示 `unit`；
   - 插件 `api` 大於宿主支援的版本時，拒絕安裝並提示「需要更新島仔」，與其他校驗失敗的提示區分開。
3. 著色器：
   - `prelude.wgsl` 增加 `body_sdf_exact`（不收縮的輪廓），覆蓋劉海、展開和四個停靠方向；
   - 插件代碼聲明 `@vertex`、`@fragment`、`@compute` 入口或任何綁定時拒絕載入；
   - `Frame` 的保留位恆為 0。
4. `motion.z`：
   - 按 PR #235 原版對形變速度的逐幀衰減方式計算平滑展開速度，寫入 `motion.z`；
   - 液態玻璃的展開形變改讀 `motion.z`，補上第 5 階段回報裡「收尾節奏仍有差異」的部分。
5. 邊緣：
   - 檢查液態玻璃在 `body_sdf_exact > 2 px` 的區域輸出透明，不符合就修正著色器；
   - 補一個 Direct3D 12 離屏測試：輪廓外 2 px 以外 alpha 為 0。
6. 液態玻璃插件升到 `1.0.2`：
   - 參數補 `unit` 和 `precision`。像素類參數（折射、色散、內部模糊、折射頻寬、白邊寬度）寫 `"px"`，係數類參數（白邊不透明度、形變）不寫單位；
   - 顯示精度按各自 `step`。
7. 文檔：同步更新 `docs/plugins/sdk-v1.schema.json` 和 `docs/plugins/sdk-v1-design.md` 中與材質相關的字段說明。
8. 測試：
   - 清單校驗（`unit`、`precision` 的合法值和非法值，`api: 2` 的提示）；
   - 著色器校驗（多餘入口、多餘綁定）；
   - `body_sdf_exact` 在四個停靠方向上與窗口可見邊緣一致；
   - 保留位為 0；
   - 邊緣透明的離屏測試；
   - 完整迴歸通過。
9. 效果對比：重做第 5 階段的「展開中途」一組，附收起動畫連續幾幀的對比，說明收尾節奏是否與預覽版一致。
10. 出包：合進 `test/acceptance`，編測試版替換桌面測試版。回報裡寫明液態玻璃需要重新安裝 1.0.2。

第 6 階段之後，功能代碼完成。合進 `main` 前，還需要使用者完成規範文末「合進 main 前的補測」三項。

# AI 額度適配（開發中，未發佈）

實現基線：`main` `39413b3c65d0c91d598507bc6d1ab3be0c6d1105`，2026-11-08 核實。

## 入口與數據含義

- OpenCode Go：套餐 API Key，固定官方 usage 地址。顯示官方滾動／周／月使用率，不冒充 Zen 按量餘額。未證實的週期長度不推斷。
- MiniMax：中國區 / 國際區必須手選，分別使用 `api.minimax.cn` / `api.minimax.io`。查詢 Coding Plan，優先官方當前 `general` 額度池，兼容 `MiniMax-M*` 舊池；不會相加共享池。剩餘百分比、舊 remaining count 語義、周倍率獨立處理。
- GLM：中國區 `open.bigmodel.cn` / 國際區 `api.z.ai`，查詢 Coding Plan，不查詢推理 API 錢包。使用原始 Authorization Key 格式，保留套餐名、週期和重置時間，工具額度不能單獨代替編程額度。添加入口為「GLM / ZCode」，使用用戶填寫的套餐 Key。舊 ZCode 本機連接保留帳號設置，並提示重新連接。Start Plan 體驗額度在 ZCode 官方客戶端查看。「獲取密鑰」按地區打開官方套餐入口；按 HTTP 和業務錯誤區分認證、權限、套餐狀態及限流，不回顯服務端原文，不跨地區重試。
- New API：輸入 HTTPS 站點地址 + API Key，默認自動識別，也可在高級設置指定協議。僅展示接口明確返回的帳戶額度，保持原單位；不把 Key 額度或無限額狀態當成帳戶餘額，不擅自換算美元。
- One API：獨立讀取 `/dashboard/billing/subscription` 和 `/dashboard/billing/usage`。兼容計費接口不能證明數據屬於帳戶還是 Key，當前顯示「餘額暫不可查詢」，不冒充帳戶餘額；保留原單位及無限額哨兵校驗。
- Sub2API：獨立 `/v1/usage`，錢包顯示餘額，訂閱週期顯示剩餘百分比；只提供 Key 限額時顯示「餘額暫不可查詢」。Key 限額不進入主卡片、收起狀態或設置明細。到期時間不是重置時間。
- Kimi Code：只讀使用本機訂閱登錄，查詢時檢查憑據有效期。登錄過期時請在 Kimi Code 中重新登錄，島仔會在下一次定時刷新時重讀登錄文件。
- 豆包：讀取網頁會員的非官方 `subscription/quota/summary` 接口，支持 5 小時、每週及活動額度，不查詢火山方舟 API 餘額。瀏覽器登錄優先識別 Windows 默認的 Chrome／Edge，再查找本機已安裝的 Chrome／Edge，使用獨立臨時目錄及隨機本地調試端口，不依賴瀏覽器擴展；僅採集適用於豆包額度接口的會話，結束或取消後關閉本次啟動的瀏覽器進程並清理臨時目錄。也可只讀導入本機豆包客戶端當前配置的登錄，數據庫佔用或加密格式未知時引導官網登錄；不讀取常用 Chrome／Edge 的帳號目錄。登錄使用當前 Windows 用戶的 DPAPI 加密，配置和緩存僅保存隨機引用及額度快照。取消或關閉連接窗口不保存登錄，重新連接保留帳號名稱、排序、常駐與配色。缺失、無效及零總額度不推斷為剩餘 100%；活動及其他權益池保留在完整明細中。

中轉地址要求站點根地址，原樣保留反向代理部署前綴（包括路徑中的 `/v1`），不會猜測或刪除 API 後綴。拒絕 HTTP（含 localhost）、用戶名/密碼、查詢參數和 fragment。自動識別僅嘗試當前站點的已知額度接口，不向其他站點、廠商或地區發送 Key。

## 安全與性能

- 鬼島只調用指定額度查詢接口，但不會縮減 Key 本身已有的權限；不要把普通 API Key 宣傳為只讀 Key。
- Key 只進入 Windows 憑據管理器 `kuidao.ApiQuota`。配置和緩存只儲存隨機憑據引用、協議與地址/地區；日誌及錯誤不回顯響應正文或 Key。
- 驗證異步執行且最多一項在途；取消後仍等待當前有界請求結束，不累計線程。取消阻止後續 One API 請求和 Key 儲存。切換服務清空輸入中的 Key。連接/移除先持久化配置，再清理舊憑據。清理日誌僅存隨機引用，寫入失敗不繼續修改；清理失敗保留引用供重試。啟動清理必須嚴格確認已儲存配置可讀、結構有效，不把配置損壞視為空帳號。
- 共享既有代理設置，禁用重定向，解碼後響應上限 1 MiB。自動識別總預算 15 秒，單次連接和請求使用剩餘預算且不超過 5 秒；界面到期即取消並丟棄結果。系統 DNS 阻塞可能延遲在途線程退出，仍保持單項驗證，不累計線程。手動指定及日常刷新沿用 15 秒請求超時，One API 兩次順序請求總耗時可能約 30 秒。
- 每個新增協議有獨立單線程隊列；最新刷新覆蓋尚未開始的舊請求，不累積無界線程，不等待其他協議完成。原有瀏覽器類服務維持同組串行。
- 429 / 臨時服務錯誤按獨立帳號退避，最長退避 1 小時。重連、修改地址和地區後舊請求結果按帳號、協議、憑據引用、來源校驗丟棄。
- 刷新失敗保留上次數據並顯示錯誤/陳舊提示。重啟緩存按協議、憑據及地址/地區綁定；舊緩存缺少新增字段仍能讀入。未標明額度範圍的舊中轉緩存等待刷新，不恢復為餘額或 Key 限額展示。
- 未知、無限、零額度分開處理；不把缺失字段當 0。OpenCode Go、MiniMax、GLM 採用 LobeHub 品牌標識，來源和 MIT 許可見第三方聲明；三種中轉協議共用密鑰標識。

## 當前界面與驗收邊界（2026-11-10）

- 中轉站新增帳號默認自動識別，手動類型選擇置於高級設置；最多 4 次只讀額度查詢，總預算 15 秒，單次請求最多 5 秒。不查詢模型、不讀取 Cookie、不跨站點或移除部署子目錄；認證失敗、限流、重定向、網絡錯誤和已識別協議的數據損壞均停止探測。
- 按 New API、Sub2API、兼容計費接口順序識別協議；協議識別成功不代表已取得帳戶餘額。展示只使用範圍明確的帳戶或訂閱數據，其他情況明確提示暫不可查詢；不因字段名含 USD 就換算美元。識別成功後儲存具體協議，刷新和既有帳號重新連接直接沿用原協議。
- 本輪 Windows MSVC 全目標迴歸 891 項通過、0 失敗；連接時限補修後額度專項 140 項通過，代理專項 3 項通過。深淺主題、三週期、耗盡限制、統一連接入口和完整明細已導出實際渲染圖檢查。
- 三種中轉協議統一到「中轉站」入口，仍由用戶選擇站點類型、填寫 HTTPS 網站地址和 API Key。PR #101 的網頁登錄憑據、Cookie 和自動探測不在本輪範圍。
- 主卡片展示 OpenCode 的三個週期；超過三個週期時優先保留最緊週期。中轉卡片展示帳戶餘額或套餐剩餘百分比，已耗盡時標橙色；「明細」打開設置並定位對應帳號，保留帳戶及套餐數據，不展示 Key 限額配置或無限額狀態。
- 關閉連接窗口或整個設置窗口時取消 API Key 驗證、清空輸入並丟棄晚到結果，不因界面關閉而在後台繼續儲存新憑據。
- #95 的本地依賴補丁已經使用，並復跑 HTTPS 隧道、明文 HTTP 代理和無代理三項迴歸。#46 有合併衝突，本輪將其需要的 API Key 連接與現有 ZCode 解析整合，沒有直接合並 PR。
- Windows 憑據儲存、讀取和刪除使用隨機獨立測試槽位驗證，不讀取真實帳號密鑰。超時、限流、重定向、認證錯誤及響應大小限制使用本機合成服務器驗證。
- TraxNode 已在真實帳號下連接並刷新，當前僅返回 Key 數據，未取得帳戶餘額；Dragon3 尚未連接。OpenCode Go、MiniMax、GLM、One API、Sub2API 尚無真實測試帳號。合成測試和界面預覽不代表真實帳號餘額驗收通過。

## 暫不啟用

- Qoder：官方國區 `@qodercn-ai/qodercn-agent-sdk` 與國際 SDK 各有 `noPrompt` / `getUsageInfo`。尚未在隔離運行時驗證登錄複用、無額外消費、取消及 Windows 兼容性；不自動安裝，不解密私有憑據。
- 火山方舟：Agent Plan `GetAFPUsage` 需要 AK/SK 簽名，不等於推理 Key；Coding Plan 官方 `arkcli usage plan --product coding-plan --format json` 的 SSO、響應契約及權限待驗證。不隱式擴大權限，不把兩產品混為一談。

## 驗證狀態

已進行：官方源代碼協議核對、兩輪模塊交叉靜態審查、`git diff --check`、Linux library 編譯。用戶隨後授權安裝官方 Rust stable 1.99.1，僅安裝在獨立雲工作區，項目依賴嚴格使用 Cargo.lock；未安裝系統包。

新增 34 個合成測試全部通過，覆蓋套餐解析、金額/單位、未知和無限、響應錯誤、地址安全、緩存來源綁定、舊緩存兼容、隊列合併及晚到結果過濾。

原始實現的 Linux `cargo build --locked --bin kuidao` 被 main.rs 兩處既有 Windows-only `system_state` 引用阻擋；`--lib` 測試被既有 WorkBuddy 手工登錄測試引用 Windows-only login 模塊阻擋。為繼續驗證，另建一次性副本，僅為以上三處增加平台條件；這些平台兼容調整沒有加入本補丁。

最終修正版在該驗證副本執行 `cargo test --locked --all-targets --features record --no-fail-fast -- --test-threads=1`，其中 library 測試：663 通過、4 失敗、58 忽略。四個失敗已在乾淨 main 的獨立基線副本復現，位置和斷言值一致：
- 日曆日期橫向留白（ui_tests.rs:3469）
- 郵件報告儲存/失敗路徑（ui_tests.rs:4556）
- 既有單額度環 100% 的文字邊界（ui_tests.rs:1756）
- 設置主題時段文字垂直居中（ui_tests.rs:4430）

隨後單獨補跑二進制與全部 Rust 集成測試目標：另 10 項通過，0 失敗；全部 example 測試目標運行成功（各含 0 項測試）。`calendar_credentials` 和 `performance` 在 Linux 含 0 項可運行測試，不代表 Windows 專項通過。總體不是全綠，不把已復現的基線失敗隱藏或算作通過。

**尚未驗證**：Windows 編譯與 Credential Manager 實測、Windows 原生 GUI、真實帳號請求、安裝包及發佈流程。無帳號憑據使用，無推送、發佈或用戶電腦修改。

Windows 隔離環境仍需執行倉庫要求：

```
cargo build --locked --bin kuidao
cargo test --locked --all-targets --features record -- --test-threads=1
```

Windows 驗收另用臨時 `kuidao_DATA_DIR`；不運行 ignore 的私人帳號測試。重點測試添加/取消/關閉/重複點擊/切換協議和地區、重連失敗與配置寫失敗、無權限/超時/限流/重定向、慢中轉與舊服務同時刷新、緩存冷啟動、全部現有額度供應商及皮膚/縮放佈局。真實帳號驗收需要單獨授權。

## 協議來源

- OpenCode Go: https://github.com/anomalyco/opencode/blob/d21b0790a6c146815f28fea604e41e922f009590/packages/console/app/src/routes/zen/go/v1/usage.ts
- MiniMax 官方 CLI: https://github.com/MiniMax-AI/cli/blob/38aba5f26173dd1065b115b221d79c9ddd87c6bb/src/client/endpoints.ts （同提交 schema、types、quota 工具）
- GLM 官方插件: https://github.com/zai-org/zai-coding-plugins/blob/main/plugins/glm-plan-usage/skills/usage-query-skill/scripts/query-usage.mjs
- New API: https://github.com/QuantumNous/new-api/blob/main/controller/token.go
- One API: https://github.com/songquanpeng/one-api/blob/main/controller/billing.go
- Sub2API: https://github.com/Wei-Shaw/sub2api/blob/main/backend/internal/handler/gateway_handler.go
- Qoder 國區 SDK: https://docs.qoder.cn/cli/sdk/cost-usage
- 方舟 Agent Plan: https://docs.volcengine.com/docs/ark/get-afp-usage-api?lang=zh

## 獨立複審修正

第二組未參與實現的審查者發現並複核了以下修正：完整展示第三窗口及工具額度、保留中轉部署路徑、顯示限制最緊的餘額及範圍、移除帳號先提交配置、保留失敗憑據清理引用、損壞配置下清理失敗關閉、限制取消後在途驗證數量。新版詳情浮層曾觸發更新時間佈局迴歸，已縮小為額度環範圍並重新運行迴歸。

環境目錄及儲存配置已完整查詢：只有用戶桌面和當前 Linux 雲環境，沒有隔離 Windows 環境，也沒有儲存的雲環境配置。本輪不連接用戶桌面、不安裝更多軟件；Windows 編譯和憑據失敗恢復實測是阻斷完成驗收的剩餘條件。現有舊刷新結果可能暫時清除新一輪加載標記，屬於既有低優先級展示問題；不會繞過憑據/來源校驗或丟失排隊刷新。

## Windows main-project validation (2026-11-08/09)

Integrated into the user's main project after verifying the manually downloaded patch SHA-256: `cb2217a2350cb8e6c248c9293464707070c34aec5cbdc83eef108a852b7cae36`. Existing user edits were backed up and preserved. No quota patch file overlapped the user's six modified files. Later concurrent messenger UI tests were preserved as well.

- Existing Rust 1.96.1 MSVC toolchain, Windows SDK and Build Tools only; no software installed. Jobs limited to one. Test data isolated with `kuidao_DATA_DIR`.
- `cargo build --locked --bin kuidao --target x86_64-pc-windows-msvc`: passed (debug build).
- Quota-focused library tests with `--features record`: 115 passed, 0 failed, 13 ignored. All 34 new adapter tests passed.
- Full `--all-targets --features record --no-fail-fast -- --test-threads=1` run: 808 passed, 1 failed, 86 ignored across targets. Two calendar credential integration tests explicitly skipped to avoid system credential operations.
- The one failure was `ui_tests::ignored_mail_event_hides_stops_reminders_persists_and_restores`, index out of bounds at line 4306. It creates an event at now + 600 seconds but queries today's date. The run and first focused retry occurred before local midnight; the same compiled test passed immediately after midnight with no code change. This is a pre-existing test date-boundary issue, not a clean full-suite pass. The test was not modified because the file contains concurrent user work.
- Source hashes remained unchanged throughout the completed full regression. `git diff --check` passed. Logs and integration backups are in `target/ai-quota-audit-20261108-233602`.

Still not verified: live provider responses using real accounts, native interactive add/cancel/retry flows, Windows Credential Manager write/delete/failure recovery, installer/release build and runtime performance with real accounts. No real keys were accessed. No push, release, running-app replacement or desktop shortcut change was performed. Qoder and Ark remain deferred as described above.
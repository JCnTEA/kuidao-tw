# 第 4 步：皮膚、表情、圖標資源包

開放不含代碼的資源包，皮膚必須做，表情和圖標看現狀能做多少做多少。分支用 `feat/plugin-asset-packs`，共同規則見 [README](README.md)。

## 先摸清現狀

- **內置皮膚**：在 `src/` 裡搜 skin 和 theme，看皮膚怎麼定義、儲存和切換。重點看三處：
  - `src/app.rs`
  - `src/extras.rs` 的外觀設定頁
  - `src/model.rs` 的配置儲存
- **表情與圖標**：看倉庫裡有沒有「表情」或桌寵素材，以及模塊圖標從哪裡來（例如 `assets/settings-icons`）。
- **已有的資源型插件**：參考 `navigation.shortcuts` 是怎麼接的，涉及三處：
  - `spike/local.rs` 的 `is_page_shortcuts`
  - `spike/local.rs` 的 `validate_page_shortcut_colors`
  - `spike/capabilities.rs` 裡的 `Role::Exclusive`

## 要做的

### `appearance.skin`（必須做）

**清單與包內容**

- 清單寫 `runtime:"assets"`、`owner:"appearance"`，這是獨佔能力。入口文件為 `theme.json`。
- 只開放現有皮膚結構裡能安全替換的配色令牌，顏色一律用 `#RRGGBB`。未列出的令牌回退到所選的基礎內置皮膚。
- 包裡不能有腳本，文件大小設上限。

**使用者怎麼用**

- 安裝並啟用後，插件皮膚出現在外觀的皮膚選擇器裡，旁邊帶「插件」小標。
- 同一時間只能選用一個插件皮膚。
- 皮膚形狀（膠囊或劉海）和停靠位置仍由使用者掌管，插件不改。
- 停用或卸載正在使用的插件皮膚時，回到使用者上次用的內置皮膚，期間使用者改過的形狀保留。

### `appearance.icon`（看現狀決定）

- 模塊圖標有統一入口的話，允許用 SVG 圖標包替換部分模塊圖標，缺的回退到內置圖標。
- SVG 要校驗：設大小上限，不能有腳本、外鏈或 foreignObject。
- 沒有統一入口、要大改才能接的，就不做，在回報裡說明原因。

### `appearance.pet`（表情）

倉庫裡沒有表情或桌寵系統的話，不要新造一套，在回報裡說明。

### 收尾

- 在 `CAPABILITIES` 裡補上對應的行。
- 在 `spike/examples/` 加一個皮膚包樣例，封面可以複製 payday 的。
- 更新文檔：
  - `docs/plugins/sdk-v1-design.md` 裡皮膚、表情、圖標那一行；
  - 同一文件第 4 節的換膚場景。

## 驗收標準

- 單元測試至少覆蓋：
  - `theme.json` 校驗：壞顏色、未知令牌、超大文件、帶腳本。
  - 能力組合。
  - 換膚場景：從奶油換成插件皮膚，期間把膠囊改成劉海，停用插件後顯示為奶油加劉海。
  - 同一時間只能選用一個插件皮膚。
  - 卸載後回退到內置皮膚。
- 下面這條命令全部通過，你改到的外觀相關定向測試也全部通過：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File spike/cargo-serial.ps1 test --locked --lib --features "sdk-js sdk-material" --target x86_64-pc-windows-msvc sdk_spike
```

- 回報裡寫清哪些能力做了，哪些沒做，沒做的說明原因。

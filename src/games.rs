//! 「遊戲庫」頁：讀取本機 Steam 收藏庫，並列出 Riot Client 的常用捷徑。
//! 只在使用者打開頁面或按下重新掃描時讀取，不背景輪詢，也不上傳任何資料。

use crate::app::GAME_H;
use crate::paint::{Card, H};
use crate::theme::{self, W};
use eframe::egui::{self, Color32, Rect};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Steam,
    Riot,
}

#[derive(Clone)]
enum Launch {
    Steam(u32),
    Riot { exe: PathBuf, args: Vec<String> },
}

#[derive(Clone)]
pub struct Game {
    pub name: String,
    pub subtitle: String,
    pub kind: Kind,
    /// 實際程式或 Steam 快取圖示；找不到時退回字母磚。
    pub icon: Option<PathBuf>,
    launch: Launch,
    last_played: u64,
}

pub struct Library {
    pub games: Vec<Game>,
    /// 這次檢視中按 × 或長按移除的項目；重新掃描仍保留，按「顯示全部」恢復。
    pub hidden: Vec<String>,
    /// 長按圖示後正在等待確認移除的項目。
    pub menu: Option<String>,
    press: Option<(String, f64)>,
    pub scanned: bool,
    pub steam_count: usize,
    pub riot_count: usize,
    pub error: String,
    pub scanned_at: String,
}

impl Default for Library {
    fn default() -> Self {
        Self {
            games: Vec::new(),
            hidden: Vec::new(),
            menu: None,
            press: None,
            scanned: false,
            steam_count: 0,
            riot_count: 0,
            error: String::new(),
            scanned_at: String::new(),
        }
    }
}

impl Library {
    pub fn load() -> Self {
        Self {
            hidden: std::fs::read(hidden_path())
                .ok()
                .and_then(|raw| serde_json::from_slice(&raw).ok())
                .unwrap_or_default(),
            ..Self::default()
        }
    }

    /// 重新讀取 Steam 與 Riot。Steam 讀不到時仍保留 Riot 捷徑。
    pub fn refresh(&mut self) {
        let (mut steam, steam_error) = scan_steam();
        let mut riot = riot_games();
        self.steam_count = steam.len();
        self.riot_count = riot.len();
        steam.sort_by_key(|game| std::cmp::Reverse(game.last_played));
        riot.sort_by(|a, b| riot_rank(&a.name).cmp(&riot_rank(&b.name)));
        self.error = steam_error;
        self.menu = None;
        self.press = None;
        self.games = riot;
        self.games.append(&mut steam);
        self.scanned = true;
        self.scanned_at = chrono::Local::now().format("%H:%M").to_string();
    }

    /// 清空本次掃描結果，但保留使用者已移除的捷徑清單。
    /// `scanned` 保持 true，避免設定頁清空後下一幀又自動掃回來。
    pub(crate) fn clear_scan(&mut self) {
        self.games.clear();
        self.menu = None;
        self.press = None;
        self.steam_count = 0;
        self.riot_count = 0;
        self.error.clear();
        self.scanned_at.clear();
        self.scanned = true;
    }

    pub(crate) fn reset_hidden(&mut self) {
        self.hidden.clear();
        self.save_hidden();
    }

    pub(crate) fn hide(&mut self, name: &str) {
        if !self.hidden.iter().any(|hidden| hidden == name) {
            self.hidden.push(name.to_string());
            self.save_hidden();
        }
    }

    pub(crate) fn save_hidden(&self) {
        let path = hidden_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(raw) = serde_json::to_vec_pretty(&self.hidden) {
            let _ = std::fs::write(path, raw);
        }
    }
}

fn hidden_path() -> PathBuf {
    crate::model::config_dir().join("games-hidden.json")
}

fn riot_rank(name: &str) -> usize {
    match name {
        "Riot Client" => 0,
        "英雄聯盟" => 1,
        "聯盟戰棋" => 2,
        "VALORANT" => 3,
        _ => 9,
    }
}

fn quoted(text: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut current = String::new();
    let mut inside = false;
    let mut escaped = false;
    for ch in text.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
        } else if inside && ch == '\\' {
            current.push(ch);
            escaped = true;
        } else if ch == '"' {
            if inside {
                values.push(std::mem::take(&mut current));
            }
            inside = !inside;
        } else if inside {
            current.push(ch);
        }
    }
    values
}

fn unescape(value: &str) -> String {
    value.replace("\\\\", "\\")
}

fn value_after(tokens: &[String], key: &str) -> Option<String> {
    tokens
        .iter()
        .position(|token| token.eq_ignore_ascii_case(key))
        .and_then(|index| tokens.get(index + 1))
        .map(|value| unescape(value))
}

fn human_size(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.1} GB", bytes as f64 / 1_000_000_000.0)
    } else if bytes >= 1_000_000 {
        format!("{:.0} MB", bytes as f64 / 1_000_000.0)
    } else {
        String::new()
    }
}

fn played_text(seconds: u64) -> String {
    if seconds == 0 {
        return "從未啟動".into();
    }
    match chrono::DateTime::from_timestamp(seconds as i64, 0) {
        Some(at) => format!(
            "{} 玩過",
            at.with_timezone(&chrono::Local).format("%Y-%m-%d")
        ),
        None => "玩過".into(),
    }
}

#[cfg(windows)]
fn registry_string(
    root: windows::Win32::System::Registry::HKEY,
    key: windows::core::PCWSTR,
    value: windows::core::PCWSTR,
) -> Option<String> {
    use windows::Win32::System::Registry::{RegGetValueW, RRF_RT_REG_SZ};
    let mut size = 0u32;
    unsafe {
        if RegGetValueW(root, key, value, RRF_RT_REG_SZ, None, None, Some(&mut size)).is_err()
            || size < 2
        {
            return None;
        }
    }
    let mut buffer = vec![0u16; (size as usize) / 2 + 1];
    unsafe {
        if RegGetValueW(
            root,
            key,
            value,
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
        .is_err()
        {
            return None;
        }
    }
    let end = buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len());
    Some(String::from_utf16_lossy(&buffer[..end]))
}

fn dedup_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = std::collections::HashSet::new();
    paths
        .into_iter()
        .filter(|path| {
            let key = std::fs::canonicalize(path)
                .unwrap_or_else(|_| path.clone())
                .to_string_lossy()
                .to_ascii_lowercase();
            seen.insert(key)
        })
        .collect()
}

#[cfg(windows)]
fn steam_roots() -> Vec<PathBuf> {
    use windows::core::w;
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    let mut roots = Vec::new();
    if let Some(path) = registry_string(
        HKEY_CURRENT_USER,
        w!("Software\\Valve\\Steam"),
        w!("SteamPath"),
    ) {
        roots.push(PathBuf::from(path.replace('/', "\\")));
    }
    for key in [
        w!("Software\\WOW6432Node\\Valve\\Steam"),
        w!("Software\\Valve\\Steam"),
    ] {
        if let Some(path) = registry_string(HKEY_LOCAL_MACHINE, key, w!("InstallPath")) {
            let path = PathBuf::from(path.replace('/', "\\"));
            if path.exists() {
                roots.push(path);
            }
        }
    }
    for variable in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Ok(base) = std::env::var(variable) {
            let path = Path::new(&base).join("Steam");
            if path.exists() {
                roots.push(path);
            }
        }
    }
    roots.sort();
    dedup_paths(roots)
}

#[cfg(not(windows))]
fn steam_roots() -> Vec<PathBuf> {
    Vec::new()
}

fn steam_libraries(root: &Path) -> Vec<PathBuf> {
    let mut libraries = vec![root.to_path_buf()];
    let vdf = root.join("steamapps").join("libraryfolders.vdf");
    if let Ok(text) = std::fs::read_to_string(vdf) {
        let tokens = quoted(&text);
        let mut known = Vec::new();
        for (index, token) in tokens.iter().enumerate() {
            let looks_like_path = token.len() > 2 && token.as_bytes().get(1) == Some(&b':');
            if token.eq_ignore_ascii_case("path") || looks_like_path {
                if let Some(value) = if token.eq_ignore_ascii_case("path") {
                    tokens.get(index + 1)
                } else {
                    Some(token)
                } {
                    let path = PathBuf::from(unescape(value).replace('/', "\\"));
                    if path.exists() {
                        known.push(path);
                    }
                }
            }
        }
        libraries.append(&mut known);
    }
    libraries.sort();
    dedup_paths(libraries)
}

fn steam_icon(root: &Path, library: &Path, appid: u32) -> Option<PathBuf> {
    let dirs = [
        root.join("appcache").join("librarycache").join(appid.to_string()),
        library.join("appcache").join("librarycache").join(appid.to_string()),
    ];
    for dir in dirs {
        if !dir.is_dir() {
            continue;
        }
        let mut named = None;
        for name in ["library_600x900.jpg", "header.jpg", "logo.png"] {
            let path = dir.join(name);
            if path.is_file() {
                named = Some(path);
            }
        }
        let mut small = None;
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let image_file = path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "jpg" | "jpeg" | "png"));
                if image_file
                    && image::image_dimensions(&path)
                        .is_ok_and(|(width, height)| width == height && width <= 48)
                {
                    small = Some(path);
                    break;
                }
            }
        }
        if small.is_some() {
            return small;
        }
        if named.is_some() {
            return named;
        }
    }
    None
}

fn scan_steam() -> (Vec<Game>, String) {
    let roots = steam_roots();
    if roots.is_empty() {
        return (
            Vec::new(),
            "找不到 Steam；可從登錄檔 SteamPath 確認安裝位置".into(),
        );
    }
    let mut games = Vec::new();
    let mut manifests = 0usize;
    for root in &roots {
        for library in steam_libraries(root) {
            let apps = library.join("steamapps");
            let Ok(entries) = std::fs::read_dir(&apps) else {
                continue;
            };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                if !name.starts_with("appmanifest_") || !name.ends_with(".acf") {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(entry.path()) else {
                    continue;
                };
                let tokens = quoted(&text);
                let Some(appid) =
                    value_after(&tokens, "appid").and_then(|value| value.parse::<u32>().ok())
                else {
                    continue;
                };
                let Some(title) = value_after(&tokens, "name").filter(|name| !name.is_empty())
                else {
                    continue;
                };
                if appid == 228980 || title.eq_ignore_ascii_case("Steamworks Common Redistributables")
                {
                    continue;
                }
                let installed = apps
                    .join("common")
                    .join(value_after(&tokens, "installdir").unwrap_or_default())
                    .exists();
                if !installed {
                    continue;
                }
                manifests += 1;
                let last = value_after(&tokens, "LastPlayed")
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(0);
                let size = value_after(&tokens, "SizeOnDisk")
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(0);
                let detail = [played_text(last), human_size(size)]
                    .into_iter()
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join(" · ");
                games.push(Game {
                    name: title,
                    subtitle: if detail.is_empty() {
                        "Steam 已安裝".into()
                    } else {
                        format!("Steam · {detail}")
                    },
                    kind: Kind::Steam,
                    icon: steam_icon(root, &library, appid),
                    launch: Launch::Steam(appid),
                    last_played: last,
                });
            }
        }
    }
    let error = if manifests == 0 {
        "Steam 收藏庫沒有可啟動的已安裝遊戲".into()
    } else {
        String::new()
    };
    (games, error)
}

#[cfg(windows)]
fn riot_client() -> Option<PathBuf> {
    use windows::core::w;
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    let mut candidates = Vec::new();
    for key in [
        w!("Software\\Riot Games\\Riot Client"),
        w!("Software\\WOW6432Node\\Riot Games\\Riot Client"),
    ] {
        for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
            for value in [w!("InstallLocation"), w!("InstallDir"), w!("Path")] {
                if let Some(path) = registry_string(root, key, value) {
                    let path = PathBuf::from(path.replace('/', "\\"));
                    candidates.push(
                        if path
                            .extension()
                            .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
                        {
                            path
                        } else {
                            path.join("RiotClientServices.exe")
                        },
                    );
                }
            }
        }
    }
    candidates.push(PathBuf::from(
        r"C:\Riot Games\Riot Client\RiotClientServices.exe",
    ));
    for variable in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
        if let Ok(base) = std::env::var(variable) {
            candidates.push(
                Path::new(&base)
                    .join("Riot Games")
                    .join("Riot Client")
                    .join("RiotClientServices.exe"),
            );
        }
    }
    candidates.into_iter().find(|path| path.is_file())
}

#[cfg(not(windows))]
fn riot_client() -> Option<PathBuf> {
    None
}

fn riot_product_exe(client: &Path, product: &str) -> Option<PathBuf> {
    let base = client.parent().and_then(Path::parent)?;
    let candidates: &[&str] = match product {
        "league_of_legends" => &["League of Legends\\LeagueClient.exe"],
        "valorant" => &["VALORANT\\live\\VALORANT.exe", "VALORANT\\VALORANT.exe"],
        _ => &[],
    };
    candidates
        .iter()
        .map(|candidate| base.join(candidate))
        .find(|path| path.is_file())
}


fn riot_games() -> Vec<Game> {
    let Some(exe) = riot_client() else {
        return Vec::new();
    };
    let product = |name: &str, product: &str, detail: &str| -> Option<Game> {
        let icon = riot_product_exe(&exe, product)?;
        Some(Game {
            name: name.into(),
            subtitle: format!("Riot · {detail}"),
            kind: Kind::Riot,
            icon: Some(icon),
            launch: Launch::Riot {
                exe: exe.clone(),
                args: vec![
                    "--launch-product".into(),
                    product.into(),
                    "--launch-patchline".into(),
                    "live".into(),
                ],
            },
            last_played: 0,
        })
    };
    [
        Some(Game {
            name: "Riot Client".into(),
            subtitle: "Riot · 啟動器".into(),
            kind: Kind::Riot,
            icon: Some(exe.clone()),
            launch: Launch::Riot {
                exe: exe.clone(),
                args: Vec::new(),
            },
            last_played: 0,
        }),
        product("英雄聯盟", "league_of_legends", "League of Legends"),
        product("聯盟戰棋", "league_of_legends", "從英雄聯盟客戶端開啟"),
        product("VALORANT", "valorant", "特戰英豪"),
    ]
    .into_iter()
    .flatten()
    .collect()
}

#[cfg(windows)]
fn open_target(target: &str, args: &[String]) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        core::PCWSTR,
        Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
    };
    let wide = |text: &str| {
        std::ffi::OsStr::new(text)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<u16>>()
    };
    let operation = wide("open");
    let target = wide(target);
    let parameters = if args.is_empty() {
        Vec::new()
    } else {
        wide(
            &args
                .iter()
                .map(|arg| {
                    if arg.contains(' ') {
                        format!("\"{arg}\"")
                    } else {
                        arg.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join(" "),
        )
    };
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(operation.as_ptr()),
            PCWSTR(target.as_ptr()),
            if parameters.is_empty() {
                PCWSTR::null()
            } else {
                PCWSTR(parameters.as_ptr())
            },
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if result.0 as usize <= 32 {
        Err("Windows 沒有回應啟動要求".into())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn open_target(_target: &str, _args: &[String]) -> Result<(), String> {
    Err("遊戲庫目前只支援 Windows".into())
}

impl Game {
    fn launch(&self) -> Result<(), String> {
        match &self.launch {
            Launch::Steam(appid) => open_target(&format!("steam://rungameid/{appid}"), &[]),
            Launch::Riot { exe, args } => open_target(&exe.to_string_lossy(), args),
        }
    }

    fn tile_color(&self) -> Color32 {
        match self.kind {
            Kind::Steam => Color32::from_rgb(44, 82, 140),
            Kind::Riot => Color32::from_rgb(156, 50, 62),
        }
    }

    fn badge(&self) -> &'static str {
        match self.kind {
            Kind::Steam => "S",
            Kind::Riot => "R",
        }
    }
}

impl crate::app::App {
    pub(crate) fn page_games(&mut self, c: &Card) {
        if !self.games.scanned {
            self.games.refresh();
        }
        let hidden=self.games.hidden.clone();
        let games=self.games.games.iter().filter(|game|!hidden.contains(&game.name)).cloned().collect::<Vec<_>>();
        let total = games.len();
        let steam_visible = games.iter().filter(|game| game.kind == Kind::Steam).count();
        let riot_visible = games.iter().filter(|game| game.kind == Kind::Riot).count();
        let summary = if !self.games.error.is_empty() && total == 0 {
            self.games.error.clone()
        } else {
            let mut parts = vec![format!("Steam {steam_visible} 款")];
            if riot_visible > 0 {
                parts.push(format!("Riot {riot_visible} 項"));
            }
            if !self.games.scanned_at.is_empty() {
                parts.push(format!("{} 掃描", self.games.scanned_at));
            }
            parts.join(" · ")
        };
        c.text(
            15.0,
            41.0,
            &c.fit(&summary, 9.5, W::Regular, 292.0),
            9.5,
            W::Regular,
            if total == 0 { theme::GRAY } else { theme::DIM },
            H::Left,
        );
        if !self.games.hidden.is_empty() {
            let show_all = c.hit(312.0, 29.0, 46.0, 18.0, "games-show-all");
            let (hover, _) = c.hover_t(&show_all);
            c.text(
                356.0,
                41.0,
                "顯示全部",
                9.5,
                W::Regular,
                paint_mix(theme::GRAY, theme::WHITE, hover),
                H::Right,
            );
            if show_all.on_hover_text("恢復所有已移除的遊戲捷徑").clicked() {
                self.games.reset_hidden();
                self.say("已恢復全部遊戲捷徑".to_string());
            }
        }
        let refresh = c.hit(360.0, 29.0, 44.0, 18.0, "games-refresh");
        let (hover, _) = c.hover_t(&refresh);
        c.text(
            404.0,
            41.0,
            "重新掃描",
            9.5,
            W::Regular,
            paint_mix(theme::GRAY, theme::WHITE, hover),
            H::Right,
        );
        if refresh
            .on_hover_text("重新讀取 Steam 收藏庫與 Riot Client，不會恢復已移除的捷徑")
            .clicked()
        {
            self.games.refresh();
            self.say(format!("已重新掃描：{} 個項目", self.games.games.len()));
        }
        if total == 0 {
            c.text(
                20.0,
                92.0,
                if self.games.games.is_empty() {"找不到可啟動的遊戲"}else{"已移除全部捷徑"},
                12.0,
                W::Semibold,
                theme::WHITE,
                H::Left,
            );
            c.text(
                20.0,
                110.0,
                if self.games.games.is_empty() {"請先從 Steam 安裝遊戲，或確認 Riot Client 已安裝。"}else{"按右上角「顯示全部」即可恢復。"},
                9.5,
                W::Regular,
                theme::GRAY,
                H::Left,
            );
            return;
        }

        let bounds = Rect::from_min_max(egui::pos2(12.0, 48.0), egui::pos2(416.0, GAME_H - 8.0));
        let mut content = egui::Ui::new(
            c.ui.ctx().clone(),
            egui::Id::new(("games-list", c.live)),
            egui::UiBuilder::new()
                .max_rect(c.rect(bounds.left(), bounds.top(), bounds.width(), bounds.height()))
                .layer_id(c.painter.layer_id())
                .style(c.ui.style().clone()),
        );
        content.set_clip_rect(
            c.rect(bounds.left(), bounds.top(), bounds.width(), bounds.height())
                .intersect(c.painter.clip_rect()),
        );
        content.set_opacity(c.painter.opacity());
        if !c.live {
            content.disable();
        }
        content.spacing_mut().item_spacing.y = 0.0;
        content.spacing_mut().scroll.bar_width = 4.0 * c.s;
        content.spacing_mut().scroll.floating = false;
        let row_h = 38.0;
        egui::ScrollArea::vertical()
            .id_salt("games-scroll")
            .max_height(bounds.height() * c.s)
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
            .show_rows(
                &mut content,
                row_h * c.s,
                games.len(),
                |ui, range| {
                    for index in range {
                        let game = games[index].clone();
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(bounds.width() * c.s, row_h * c.s),
                            egui::Sense::hover(),
                        );
                        let mut row = Card::new(ui, rect.min, ui.clip_rect());
                        row.s = c.s;
                        row.live = c.live;
                        row.boost = c.boost;
                        let hit = row.hit(
                            38.0,
                            0.0,
                            bounds.width() - 48.0,
                            row_h - 1.0,
                            ("game-row", index, game.name.as_str()),
                        );
                        let icon_hit = row.hit(
                            8.0,
                            6.0,
                            28.0,
                            28.0,
                            ("game-icon", index, game.name.as_str()),
                        );
                        let remove = row.hit(
                            29.0,
                            3.0,
                            14.0,
                            14.0,
                            ("game-remove", index, game.name.as_str()),
                        );
                        let now = ui.input(|input| input.time);
                        let elapsed = self
                            .games
                            .press
                            .as_ref()
                            .filter(|(name, _)| name == &game.name)
                            .map(|(_, start)| now - *start)
                            .unwrap_or(0.0);
                        let mut menu_open = self.games.menu.as_deref() == Some(game.name.as_str());
                        if icon_hit.is_pointer_button_down_on() {
                            if elapsed <= 0.0 {
                                self.games.press = Some((game.name.clone(), now));
                            } else if elapsed >= 0.55 {
                                self.games.menu = Some(game.name.clone());
                                self.games.press = None;
                                menu_open = true;
                            }
                        } else if elapsed > 0.0 {
                            self.games.press = None;
                        }
                        let hovered = ui.ctx().pointer_hover_pos().is_some_and(|point| {
                            rect.contains(point) && ui.clip_rect().contains(point)
                        });
                        if hovered {
                            row.fill(
                                0.0,
                                0.0,
                                bounds.width() - 10.0,
                                row_h - 1.0,
                                6.0,
                                Color32::from_white_alpha(14),
                            );
                        }
                        let icon_rect = row.rect(9.0, 7.0, 26.0, 26.0);
                        row.fill(8.0, 6.0, 28.0, 28.0, 8.0, game.tile_color());
                        let mut painted = false;
                        if let Some(path) = game.icon.as_ref() {
                            if let Some(texture) = self.icons.get(c.ui.ctx(), path).and_then(|preview| preview.image) {
                                row.painter.image(texture.id(), icon_rect, game_icon_uv(&texture), Color32::WHITE);
                                painted = true;
                            }
                        }
                        if !painted {
                            row.text(
                                22.0,
                                26.0,
                                game.badge(),
                                13.0,
                                W::Bold,
                                theme::WHITE,
                                H::Center,
                            );
                        }
                        if hovered && !menu_open {
                            row.circle(35.5, 7.0, 6.0, Color32::from_black_alpha(205));
                            row.text(35.5, 10.4, "×", 10.0, W::Bold, theme::WHITE, H::Center);
                        }
                        row.text(
                            46.0,
                            19.0,
                            &row.fit(&game.name, 11.5, W::Semibold, 330.0),
                            11.5,
                            W::Semibold,
                            if hovered {
                                theme::WHITE
                            } else {
                                Color32::from_gray(232)
                            },
                            H::Left,
                        );
                        row.text(
                            46.0,
                            32.0,
                            &row.fit(&game.subtitle, 9.0, W::Regular, 332.0),
                            9.0,
                            W::Regular,
                            theme::GRAY,
                            H::Left,
                        );
                        if menu_open {
                            row.text(336.0, 24.0, "移除", 9.5, W::Semibold, Color32::from_rgb(255,128,128), H::Right);
                            row.text(388.0, 24.0, "取消", 9.5, W::Semibold, theme::GRAY, H::Right);
                        } else {
                            row.text(
                                392.0,
                                24.0,
                                "啟動",
                                9.5,
                                W::Semibold,
                                if hovered { theme::BLUE } else { theme::DIM },
                                H::Right,
                            );
                        }
                        let remove = remove.on_hover_text("從快速開啟移除；按「顯示全部」可恢復");
                        let menu_remove = row.hit(300.0, 14.0, 40.0, 20.0, ("game-menu-remove", index, game.name.as_str()));
                        let menu_cancel = row.hit(360.0, 14.0, 32.0, 20.0, ("game-menu-cancel", index, game.name.as_str()));
                        if remove.clicked() || (menu_open && menu_remove.clicked()) {
                            self.games.hide(&game.name);
                            self.games.menu = None;
                            self.say(format!("已從快速開啟移除 {}", game.name));
                        } else if menu_open && menu_cancel.clicked() {
                            self.games.menu = None;
                        } else if !menu_open
                            && (icon_hit
                                .on_hover_text(format!("點一下啟動；長按可移除（{}）", game.name))
                                .clicked()
                                || hit.on_hover_text(format!("啟動 {}", game.name)).clicked())
                        {
                            match game.launch() {
                                Ok(()) => self.say(format!("正在啟動 {}", game.name)),
                                Err(error) => self.say(format!("無法啟動 {}：{error}", game.name)),
                            }
                        }
                        if index + 1 < total {
                            row.fill(
                                0.0,
                                row_h - 1.0,
                                bounds.width() - 10.0,
                                1.0,
                                0.0,
                                Color32::from_white_alpha(9),
                            );
                        }
                    }
                },
            );
    }
}

fn paint_mix(a: Color32, b: Color32, t: f32) -> Color32 {
    crate::paint::mix(a, b, t)
}

fn game_icon_uv(texture: &egui::TextureHandle) -> egui::Rect {
    let size = texture.size();
    let width = size[0] as f32;
    let height = size[1] as f32;
    if width <= 0.0 || height <= 0.0 {
        return egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
    }
    let aspect = width / height;
    if aspect > 1.0 {
        let inset = (1.0 - 1.0 / aspect) * 0.5;
        egui::Rect::from_min_max(egui::pos2(inset, 0.0), egui::pos2(1.0 - inset, 1.0))
    } else if aspect < 1.0 {
        let inset = (1.0 - aspect) * 0.5;
        egui::Rect::from_min_max(egui::pos2(0.0, inset), egui::pos2(1.0, 1.0 - inset))
    } else {
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::Library;

    #[test]
    fn clearing_scan_resets_results_without_losing_hidden_shortcuts() {
        let mut library = Library::default();
        library.hidden = vec!["Forza Horizon 5".into()];
        library.steam_count = 3;
        library.riot_count = 4;
        library.scanned_at = "12:34".into();
        library.error = "Steam 路徑失效".into();

        library.clear_scan();

        assert!(library.games.is_empty());
        assert_eq!(library.steam_count, 0);
        assert_eq!(library.riot_count, 0);
        assert!(library.scanned_at.is_empty());
        assert!(library.error.is_empty());
        assert_eq!(library.hidden, vec!["Forza Horizon 5"]);
        assert!(library.scanned, "清除後應停在空白狀態，不立即自動重掃");
    }
}
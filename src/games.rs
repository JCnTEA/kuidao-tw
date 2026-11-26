//! 「遊戲庫」頁：從本機 Steam、Riot、Epic、GOG、Ubisoft、EA、Battle.net 與
//! Amazon 留下的 manifest、登入資料與安裝資料夾整理已安裝遊戲。
//! 只在使用者打開頁面或按下重新掃描時讀取，不背景輪詢，也不上傳任何資料。

use crate::app::GAME_H;
use crate::paint::{Card, H};
use crate::theme::{self, W};
use eframe::egui::{self, Color32, Rect};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Steam,
    Riot,
    Epic,
    Gog,
    Ubisoft,
    Ea,
    BattleNet,
    Amazon,
}

impl Kind {
    pub const COUNT: usize = 8;
    pub const SUMMARY: [Kind; Self::COUNT] = [
        Kind::Steam,
        Kind::Riot,
        Kind::Epic,
        Kind::Gog,
        Kind::Ubisoft,
        Kind::Ea,
        Kind::BattleNet,
        Kind::Amazon,
    ];

    fn index(self) -> usize {
        self as usize
    }

    fn label(self) -> &'static str {
        match self {
            Kind::Steam => "Steam",
            Kind::Riot => "Riot",
            Kind::Epic => "Epic",
            Kind::Gog => "GOG",
            Kind::Ubisoft => "Ubisoft",
            Kind::Ea => "EA",
            Kind::BattleNet => "Battle.net",
            Kind::Amazon => "Amazon",
        }
    }

    fn unit(self) -> &'static str {
        if self == Kind::Riot {
            "項"
        } else {
            "款"
        }
    }
}

fn kind_rank(kind: Kind) -> usize {
    match kind {
        Kind::Riot => 0,
        Kind::Steam => 1,
        Kind::Epic => 2,
        Kind::Gog => 3,
        Kind::Ubisoft => 4,
        Kind::Ea => 5,
        Kind::BattleNet => 6,
        Kind::Amazon => 7,
    }
}

#[derive(Clone)]
enum Launch {
    Steam(u32),
    Uri(String),
    Exe { exe: PathBuf, args: Vec<String> },
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
    /// 每個來源掃到幾款；索引對應 `Kind::index()`。
    pub counts: [usize; Kind::COUNT],
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
            counts: [0; Kind::COUNT],
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

    /// 重新讀取所有支援平台留在本機的資料。任一來源失敗都不影響其他來源。
    pub fn refresh(&mut self) {
        let riot = riot_games();
        let steam = scan_steam();
        let mut groups = vec![riot, steam];
        #[cfg(windows)]
        {
            groups.push(scan_epic());
            groups.push(scan_other_launchers());
        }
        let mut games = groups.into_iter().flatten().collect::<Vec<_>>();
        games.sort_by(|a, b| {
            kind_rank(a.kind)
                .cmp(&kind_rank(b.kind))
                .then_with(|| b.last_played.cmp(&a.last_played))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        let mut seen = std::collections::HashSet::new();
        games.retain(|game| seen.insert(game.name.trim().to_lowercase()));

        self.counts = [0; Kind::COUNT];
        for game in &games {
            self.counts[game.kind.index()] += 1;
        }
        self.error = if games.is_empty() {
            "找不到可啟動的本機遊戲".into()
        } else {
            String::new()
        };
        self.menu = None;
        self.press = None;
        self.games = games;
        self.scanned = true;
        self.scanned_at = chrono::Local::now().format("%H:%M").to_string();
    }

    /// 清空本次掃描結果，但保留使用者已移除的捷徑清單。
    /// `scanned` 保持 true，避免設定頁清空後下一幀又自動掃回來。
    pub(crate) fn clear_scan(&mut self) {
        self.games.clear();
        self.menu = None;
        self.press = None;
        self.counts = [0; Kind::COUNT];
        self.error.clear();
        self.scanned_at.clear();
        self.scanned = true;
    }

    pub fn source_summary(&self) -> String {
        compact_sources(&self.games)
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

fn compact_sources(games: &[Game]) -> String {
    let mut parts = Vec::new();
    for kind in Kind::SUMMARY {
        let count = games.iter().filter(|game| game.kind == kind).count();
        if count > 0 {
            parts.push(format!("{} {} {}", kind.label(), count, kind.unit()));
        }
    }
    match parts.len() {
        0 => "沒有找到本機遊戲".into(),
        1..=4 => parts.join(" · "),
        _ => format!("{} · +{} 來源", parts[..4].join(" · "), parts.len() - 4),
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

#[cfg(windows)]
fn wide(text: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(text)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(windows)]
fn registry_subkeys(
    root: windows::Win32::System::Registry::HKEY,
    path: &str,
) -> Vec<String> {
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::System::Registry::{
        RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, HKEY, KEY_READ,
    };
    let wide_path = wide(path);
    let mut key = HKEY(std::ptr::null_mut());
    unsafe {
        if RegOpenKeyExW(root, PCWSTR(wide_path.as_ptr()), 0, KEY_READ, &mut key).is_err() {
            return Vec::new();
        }
    }
    let mut names = Vec::new();
    let mut index = 0u32;
    loop {
        let mut name = [0u16; 512];
        let mut length = name.len() as u32;
        let status = unsafe {
            RegEnumKeyExW(
                key,
                index,
                PWSTR(name.as_mut_ptr()),
                &mut length,
                None,
                PWSTR::null(),
                None,
                None,
            )
        };
        if status.is_err() {
            break;
        }
        if length > 0 {
            names.push(String::from_utf16_lossy(&name[..length as usize]));
        }
        index += 1;
    }
    unsafe {
        let _ = RegCloseKey(key);
    }
    names
}

#[cfg(windows)]
fn registry_string_at(
    root: windows::Win32::System::Registry::HKEY,
    path: &str,
    value: &str,
) -> Option<String> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegGetValueW, RegOpenKeyExW, HKEY, KEY_READ, RRF_RT_REG_SZ,
    };
    let wide_path = wide(path);
    let wide_value = wide(value);
    let mut key = HKEY(std::ptr::null_mut());
    unsafe {
        if RegOpenKeyExW(root, PCWSTR(wide_path.as_ptr()), 0, KEY_READ, &mut key).is_err() {
            return None;
        }
    }
    let mut size = 0u32;
    let found = unsafe {
        RegGetValueW(
            key,
            PCWSTR::null(),
            PCWSTR(wide_value.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut size),
        )
        .is_ok()
            && size >= 2
    };
    let result = if !found {
        None
    } else {
        let mut buffer = vec![0u16; (size as usize) / 2 + 1];
        let read = unsafe {
            RegGetValueW(
                key,
                PCWSTR::null(),
                PCWSTR(wide_value.as_ptr()),
                RRF_RT_REG_SZ,
                None,
                Some(buffer.as_mut_ptr().cast()),
                Some(&mut size),
            )
        }
        .is_ok()
        && size >= 2;
        if read {
            let end = buffer
                .iter()
                .position(|unit| *unit == 0)
                .unwrap_or(buffer.len());
            Some(String::from_utf16_lossy(&buffer[..end]))
        } else {
            None
        }
    };
    unsafe {
        let _ = RegCloseKey(key);
    }
    result
}

#[cfg(windows)]
fn registry_dword_at(
    root: windows::Win32::System::Registry::HKEY,
    path: &str,
    value: &str,
) -> Option<u32> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegGetValueW, RegOpenKeyExW, HKEY, KEY_READ, RRF_RT_REG_DWORD,
    };
    let wide_path = wide(path);
    let wide_value = wide(value);
    let mut key = HKEY(std::ptr::null_mut());
    unsafe {
        if RegOpenKeyExW(root, PCWSTR(wide_path.as_ptr()), 0, KEY_READ, &mut key).is_err() {
            return None;
        }
    }
    let mut data = 0u32;
    let mut size = 4u32;
    let ok = unsafe {
        RegGetValueW(
            key,
            PCWSTR::null(),
            PCWSTR(wide_value.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut data as *mut u32).cast()),
            Some(&mut size),
        )
        .is_ok()
    };
    unsafe {
        let _ = RegCloseKey(key);
    }
    ok.then_some(data)
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

fn scan_steam() -> Vec<Game> {
    let roots = steam_roots();
    if roots.is_empty() {
        return Vec::new();
    }
    let mut games = Vec::new();
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
    games
}

#[cfg(windows)]
fn program_data() -> PathBuf {
    std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
}

#[cfg(windows)]
fn scan_epic() -> Vec<Game> {
    let dir = program_data()
        .join("Epic")
        .join("EpicGamesLauncher")
        .join("Data")
        .join("Manifests");
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut games = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("item"))
        {
            continue;
        }
        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };
        if let Some(game) = epic_from_value(&value) {
            games.push(game);
        }
    }
    games
}

#[cfg(windows)]
fn epic_from_value(value: &serde_json::Value) -> Option<Game> {
    if value
        .get("bIsIncompleteInstall")
        .and_then(|flag| flag.as_bool())
        == Some(true)
    {
        return None;
    }
    if value.get("bIsApplication").and_then(|flag| flag.as_bool()) == Some(false) {
        return None;
    }
    let app = value.get("AppName")?.as_str()?.trim();
    if app.is_empty() {
        return None;
    }
    let name = value
        .get("DisplayName")
        .and_then(|name| name.as_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(app)
        .trim()
        .to_string();
    if is_framework_name(&name) {
        return None;
    }
    let install = PathBuf::from(value.get("InstallLocation")?.as_str()?.trim());
    if !install.is_dir() {
        return None;
    }
    let icon = value
        .get("LaunchExecutable")
        .and_then(|exe| exe.as_str())
        .map(|exe| install.join(exe.replace('/', "\\")))
        .filter(|exe| exe.is_file())
        .or_else(|| find_game_exe(&install));
    let command = value
        .get("LaunchCommand")
        .and_then(|command| command.as_str())
        .map(str::trim)
        .filter(|command| command.starts_with("com.epicgames.launcher://"))
        .map(str::to_string)
        .unwrap_or_else(|| {
            format!("com.epicgames.launcher://apps/{app}?action=launch&silent=true")
        });
    let bytes = value
        .get("InstallSize")
        .and_then(|size| {
            size.as_u64()
                .or_else(|| size.as_str().and_then(|text| text.parse::<u64>().ok()))
        })
        .unwrap_or(0);
    let size = human_size(bytes);
    let subtitle = if size.is_empty() {
        "Epic · 已安裝".to_string()
    } else {
        format!("Epic · 已安裝 · {size}")
    };
    Some(Game {
        name,
        subtitle,
        kind: Kind::Epic,
        icon,
        launch: Launch::Uri(command),
        last_played: 0,
    })
}

#[cfg(windows)]
fn is_framework_name(name: &str) -> bool {
    let lower = name.trim().to_lowercase();
    [
        "prerequisite",
        "redistributable",
        "easy anti-cheat",
        "battleye",
        "directx",
        "visual c++",
        "unreal engine",
        "steamworks common",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

#[cfg(windows)]
fn is_launcher_name(name: &str) -> bool {
    let lower = name.trim().to_lowercase();
    lower == "steam"
        || lower == "steamvr"
        || lower.starts_with("epic games launcher")
        || lower.starts_with("gog galaxy")
        || lower.starts_with("ubisoft connect")
        || lower.starts_with("ea app")
        || lower.starts_with("origin")
        || lower.starts_with("battle.net")
        || lower.starts_with("blizzard battle.net")
        || lower.starts_with("amazon games")
        || lower.starts_with("riot client")
}

#[cfg(windows)]
fn install_source(name: &str, install: &str, publisher: &str) -> Option<Kind> {
    if is_launcher_name(name) || is_framework_name(name) {
        return None;
    }
    let path = install.replace('/', "\\").to_lowercase();
    let publisher = publisher.trim().to_lowercase();
    if path.contains("\\steamapps\\common\\") {
        return None; // Steam 已由 appmanifest 掃描，避免重複。
    }
    if path.contains("\\epic games\\") && !path.contains("\\epic games\\launcher") {
        return Some(Kind::Epic);
    }
    if path.contains("\\gog games\\") || path.contains("\\gog galaxy\\games\\") {
        return Some(Kind::Gog);
    }
    if path.contains("\\ubisoft\\")
        && !path.contains("\\ubisoft game launcher")
        || path.contains("\\ubisoft game launcher\\games\\")
    {
        return Some(Kind::Ubisoft);
    }
    if path.contains("\\ea games\\") || path.contains("\\origin games\\") {
        return Some(Kind::Ea);
    }
    if path.contains("\\battle.net\\") && !path.contains("\\battle.net\\battle.net") {
        return Some(Kind::BattleNet);
    }
    if path.contains("\\amazon games\\library\\") || path.contains("\\amazon games\\") {
        return Some(Kind::Amazon);
    }
    if publisher.contains("electronic arts") {
        return Some(Kind::Ea);
    }
    if publisher.contains("ubisoft") {
        return Some(Kind::Ubisoft);
    }
    if publisher.contains("blizzard entertainment") {
        return Some(Kind::BattleNet);
    }
    if publisher.contains("gog.com") || publisher.contains("gog sp. z o.o") {
        return Some(Kind::Gog);
    }
    if publisher.contains("epic games") && !is_framework_name(name) {
        return Some(Kind::Epic);
    }
    if publisher.contains("amazon") && path.contains("game") {
        return Some(Kind::Amazon);
    }
    None
}

#[cfg(windows)]
fn find_game_exe(dir: &Path) -> Option<PathBuf> {
    fn walk(dir: &Path, depth: usize, budget: &mut usize, found: &mut Vec<PathBuf>) {
        if depth > 2 || *budget == 0 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            if *budget == 0 {
                break;
            }
            *budget -= 1;
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_file() {
                if path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
                    && !is_helper_exe(&path)
                {
                    found.push(path);
                }
            } else if file_type.is_dir() {
                walk(&path, depth + 1, budget, found);
            }
        }
    }

    let mut found = Vec::new();
    let mut budget = 800usize;
    walk(dir, 0, &mut budget, &mut found);
    let folder = dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .replace([' ', '-', '_'], "")
        .to_lowercase();
    found.sort_by_key(|path| {
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default()
            .replace([' ', '-', '_'], "")
            .to_lowercase();
        let name_match = usize::from(stem != folder);
        (name_match, path.components().count())
    });
    found.into_iter().next()
}

#[cfg(windows)]
fn is_helper_exe(path: &Path) -> bool {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .to_lowercase();
    [
        "unins",
        "setup",
        "vc_redist",
        "dxsetup",
        "directx",
        "easyanticheat",
        "eac",
        "battleye",
        "crash",
        "report",
        "update",
        "install",
    ]
    .iter()
    .any(|prefix| stem.starts_with(prefix))
}

#[cfg(windows)]
fn display_icon(value: &str) -> Option<PathBuf> {
    let raw = value.split(',').next()?.trim().trim_matches('"');
    if raw.is_empty() {
        return None;
    }
    let path = PathBuf::from(raw.replace('/', "\\"));
    path.is_file().then_some(path)
}

#[cfg(windows)]
fn scan_other_launchers() -> Vec<Game> {
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    const KEYS: [(&str, &str); 3] = [
        (
            "HKLM64",
            "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
        ),
        (
            "HKLM32",
            "Software\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
        ),
        (
            "HKCU",
            "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
        ),
    ];
    let mut games = Vec::new();
    for (root_name, path) in KEYS {
        let root = if root_name == "HKCU" {
            HKEY_CURRENT_USER
        } else {
            HKEY_LOCAL_MACHINE
        };
        for subkey in registry_subkeys(root, path) {
            let key = format!("{path}\\{subkey}");
            let Some(name) = registry_string_at(root, &key, "DisplayName")
                .map(|name| name.trim().to_string())
                .filter(|name| !name.is_empty())
            else {
                continue;
            };
            let location = registry_string_at(root, &key, "InstallLocation")
                .map(|path| path.trim().trim_matches('"').to_string())
                .unwrap_or_default();
            let display = registry_string_at(root, &key, "DisplayIcon")
                .unwrap_or_default();
            let publisher =
                registry_string_at(root, &key, "Publisher").unwrap_or_default();
            let icon = display_icon(&display);
            let install = if !location.is_empty() {
                let path = PathBuf::from(location.replace('/', "\\"));
                path.is_dir().then_some(path)
            } else {
                icon.as_deref()
                    .and_then(Path::parent)
                    .filter(|parent| parent.is_dir())
                    .map(Path::to_path_buf)
            };
            let Some(install) = install else {
                continue;
            };
            let install_text = install.to_string_lossy();
            let Some(kind) = install_source(&name, &install_text, &publisher) else {
                continue;
            };
            let launch_icon = icon
                .as_deref()
                .filter(|path| {
                    path.extension()
                        .and_then(|extension| extension.to_str())
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
                })
                .map(Path::to_path_buf);
            let Some(exe) = launch_icon
                .filter(|path| !is_helper_exe(path))
                .or_else(|| find_game_exe(&install))
            else {
                continue;
            };
            let icon = icon.or_else(|| Some(exe.clone()));
            let bytes = registry_dword_at(root, &key, "EstimatedSize")
                .map(|kb| u64::from(kb) * 1024)
                .unwrap_or(0);
            let size = human_size(bytes);
            let subtitle = if size.is_empty() {
                format!("{} · 已安裝", kind.label())
            } else {
                format!("{} · 已安裝 · {size}", kind.label())
            };
            games.push(Game {
                name,
                subtitle,
                kind,
                icon,
                launch: Launch::Exe {
                    exe,
                    args: Vec::new(),
                },
                last_played: 0,
            });
        }
    }
    games
}

#[cfg(not(windows))]
fn scan_epic() -> Vec<Game> {
    Vec::new()
}

#[cfg(not(windows))]
fn scan_other_launchers() -> Vec<Game> {
    Vec::new()
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
            launch: Launch::Exe {
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
            launch: Launch::Exe {
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
            Launch::Uri(uri) => open_target(uri, &[]),
            Launch::Exe { exe, args } => open_target(&exe.to_string_lossy(), args),
        }
    }

    fn tile_color(&self) -> Color32 {
        match self.kind {
            Kind::Steam => Color32::from_rgb(44, 82, 140),
            Kind::Riot => Color32::from_rgb(156, 50, 62),
            Kind::Epic => Color32::from_rgb(46, 46, 58),
            Kind::Gog => Color32::from_rgb(94, 56, 140),
            Kind::Ubisoft => Color32::from_rgb(36, 56, 112),
            Kind::Ea => Color32::from_rgb(126, 60, 48),
            Kind::BattleNet => Color32::from_rgb(24, 58, 112),
            Kind::Amazon => Color32::from_rgb(126, 74, 34),
        }
    }

    fn badge(&self) -> &'static str {
        match self.kind {
            Kind::Steam => "S",
            Kind::Riot => "R",
            Kind::Epic => "E",
            Kind::Gog => "G",
            Kind::Ubisoft => "U",
            Kind::Ea => "EA",
            Kind::BattleNet => "B",
            Kind::Amazon => "A",
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
        let summary = if !self.games.error.is_empty() && total == 0 {
            self.games.error.clone()
        } else {
            let mut summary = compact_sources(&games);
            if !self.games.scanned_at.is_empty() {
                summary.push_str(&format!(" · {} 掃描", self.games.scanned_at));
            }
            summary
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
            .on_hover_text("重新掃描 Steam、Epic、GOG、Ubisoft、EA、Battle.net 與 Amazon 的本機資料，不會恢復已移除的捷徑")
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
            if self.games.games.is_empty() {
                c.text(
                    20.0,
                    110.0,
                    "已檢查 Steam、Epic、GOG、Ubisoft、EA、Battle.net 與 Amazon。",
                    9.5,
                    W::Regular,
                    theme::GRAY,
                    H::Left,
                );
                c.text(
                    20.0,
                    126.0,
                    "請安裝遊戲後按右上角「重新掃描」。",
                    9.5,
                    W::Regular,
                    theme::GRAY,
                    H::Left,
                );
            } else {
                c.text(
                    20.0,
                    110.0,
                    "按右上角「顯示全部」即可恢復。",
                    9.5,
                    W::Regular,
                    theme::GRAY,
                    H::Left,
                );
            }
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
    use super::{compact_sources, Kind, Library};

    fn game(name: &str, kind: Kind) -> super::Game {
        super::Game {
            name: name.into(),
            subtitle: String::new(),
            kind,
            icon: None,
            launch: super::Launch::Uri(String::new()),
            last_played: 0,
        }
    }

    #[test]
    fn clearing_scan_resets_results_without_losing_hidden_shortcuts() {
        let mut library = Library::default();
        library.hidden = vec!["Forza Horizon 5".into()];
        library.counts[Kind::Steam.index()] = 3;
        library.counts[Kind::Riot.index()] = 4;
        library.counts[Kind::Epic.index()] = 2;
        library.scanned_at = "12:34".into();
        library.error = "Steam 路徑失效".into();

        library.clear_scan();

        assert!(library.games.is_empty());
        assert_eq!(library.counts, [0; Kind::COUNT]);
        assert!(library.scanned_at.is_empty());
        assert!(library.error.is_empty());
        assert_eq!(library.hidden, vec!["Forza Horizon 5"]);
        assert!(library.scanned, "清除後應停在空白狀態，不立即自動重掃");
    }

    #[test]
    fn source_summary_keeps_first_four_and_counts_remaining() {
        let games = vec![
            game("A", Kind::Steam),
            game("B", Kind::Steam),
            game("C", Kind::Riot),
            game("D", Kind::Epic),
            game("E", Kind::Gog),
            game("F", Kind::Ubisoft),
        ];
        let summary = compact_sources(&games);
        assert!(summary.contains("Steam 2 款"));
        assert!(summary.contains("Riot 1 項"));
        assert!(!summary.contains("Ubisoft"));
        assert!(summary.ends_with("+1 來源"));
    }

    #[cfg(windows)]
    #[test]
    fn classifies_install_folders_without_treating_launchers_as_games() {
        use super::install_source;
        assert_eq!(
            install_source("Fortnite", r"C:\Program Files\Epic Games\Fortnite", ""),
            Some(Kind::Epic)
        );
        assert_eq!(
            install_source("Cyberpunk 2077", r"C:\Program Files (x86)\GOG Galaxy\Games\Cyberpunk 2077", "CD PROJEKT RED"),
            Some(Kind::Gog)
        );
        assert_eq!(
            install_source("Battlefield 1", r"D:\EA Games\Battlefield 1", "Electronic Arts"),
            Some(Kind::Ea)
        );
        assert_eq!(install_source("Epic Games Launcher", r"C:\Program Files (x86)\Epic Games\Launcher", "Epic Games, Inc."), None);
        assert_eq!(
            install_source("Forza Horizon 5", r"D:\SteamLibrary\steamapps\common\ForzaHorizon5", "Xbox Game Studios"),
            None
        );
    }
}
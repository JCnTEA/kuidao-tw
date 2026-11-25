//! 社群訊息來源：從 Windows 通知辨識 Threads／Messenger／Facebook／LINE／Instagram，
//! 只記住「有幾則」與應用程式開關，不保存訊息內容。
use crate::paint::Card;
use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Social {
    Line,
    PhoneLink,
    Threads,
    Messenger,
    Facebook,
    Instagram,
}

impl Social {
    pub const ALL: [Self; 6] = [
        Self::Line,
        Self::PhoneLink,
        Self::Threads,
        Self::Messenger,
        Self::Facebook,
        Self::Instagram,
    ];
    pub fn key(self) -> &'static str {
        match self {
            Self::Line => "line",
            Self::PhoneLink => "phonelink",
            Self::Threads => "threads",
            Self::Messenger => "messenger",
            Self::Facebook => "facebook",
            Self::Instagram => "instagram",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Line => "LINE",
            Self::PhoneLink => "手機連結",
            Self::Threads => "Threads",
            Self::Messenger => "Messenger",
            Self::Facebook => "Facebook",
            Self::Instagram => "Instagram",
        }
    }
    pub fn url(self) -> &'static str {
        match self {
            Self::Line => "https://line.me/",
            Self::PhoneLink => "ms-settings:mobile-devices",
            Self::Threads => "https://www.threads.net/",
            Self::Messenger => "https://www.messenger.com/",
            Self::Facebook => "https://www.facebook.com/",
            Self::Instagram => "https://www.instagram.com/",
        }
    }
    /// 預設順序：LINE 與手機連結先並排，其餘依原本習慣往右排。
    pub fn default_order() -> Vec<String> {
        Self::ALL
            .iter()
            .map(|social| social.key().to_owned())
            .collect()
    }
    /// 設定檔存的是逗號分隔的 key；未知或重複的項目忽略，缺少的補在最後。
    pub fn parse_order(value: &str) -> Vec<String> {
        let mut order = Vec::new();
        for key in value
            .split(',')
            .map(str::trim)
            .filter(|key| !key.is_empty())
        {
            if Self::ALL.iter().any(|social| social.key() == key)
                && !order.iter().any(|item: &String| item == key)
            {
                order.push(key.to_owned());
            }
        }
        for social in Self::ALL {
            if !order.iter().any(|item| item == social.key()) {
                order.push(social.key().to_owned());
            }
        }
        order
    }
    /// 依使用者排序回傳所有來源；左邊優先。
    pub fn ordered(order: &[String]) -> Vec<Self> {
        let mut list: Vec<Self> = order
            .iter()
            .filter_map(|key| Self::ALL.iter().copied().find(|social| social.key() == key))
            .collect();
        for social in Self::ALL {
            if !list.contains(&social) {
                list.push(social);
            }
        }
        list
    }
    /// 通知來源的 Windows AppID；有它才能開原生程式，而不是把 LINE 丟到瀏覽器。
    pub(crate) fn app_target(self, id: Option<&str>) -> Option<&str> {
        id.map(str::trim)
            .filter(|id| !id.is_empty() && *id != self.key())
    }
    /// 優先喚起發出通知的桌面程式；LINE 另外還原被收進系統匣的主視窗。
    /// 沒有可用 AppID 或系統拒絕時，才退回官網。
    /// 優先開啟通知帶的深層連結；LINE 的 `line://` 會直接回到同一個聊天室，
    /// 其他社群則是通知指向的貼文或對話。連結失效才退回啟動主程式。
    pub fn open(self, id: Option<&str>, launch: Option<&str>) -> Result<(), String> {
        if let Some(launch) = launch
            .map(str::trim)
            .filter(|launch| is_safe_launch(launch))
        {
            if open_uri(launch).is_ok() {
                if self == Self::Line {
                    focus_line_window();
                }
                return Ok(());
            }
        }
        #[cfg(windows)]
        if let Some(id) = self.app_target(id) {
            if open_app_id(id).is_ok() {
                if self == Self::Line {
                    focus_line_window();
                }
                return Ok(());
            }
        }
        open_uri(self.url())
    }
    /// 以 (x,y) 為中心畫 20×20 的圓角標誌。
    pub fn paint(self, c: &Card, x: f32, y: f32) {
        self.paint_sized(c, x, y, 20.0);
    }
    /// 指定邊長的圓形圖示；未讀膠囊用較大的 26px，讓圖示與數字徽章比例一致。
    pub fn paint_sized(self, c: &Card, x: f32, y: f32, size: f32) {
        let rect = c.rect(x - size / 2.0, y - size / 2.0, size, size);
        let icon = match self {
            Self::Line => egui::Image::new(egui::include_image!("../assets/social/line.png")),
            Self::PhoneLink => {
                egui::Image::new(egui::include_image!("../assets/social/phonelink.png"))
            }
            Self::Threads => egui::Image::new(egui::include_image!("../assets/social/threads.png")),
            Self::Messenger => {
                egui::Image::new(egui::include_image!("../assets/social/messenger.png"))
            }
            Self::Facebook => {
                egui::Image::new(egui::include_image!("../assets/social/facebook.png"))
            }
            Self::Instagram => {
                egui::Image::new(egui::include_image!("../assets/social/instagram.png"))
            }
        };
        icon.corner_radius(size / 2.0).paint_at(c.ui, rect);
    }
    fn index(self) -> usize {
        match self {
            Self::Line => 0,
            Self::PhoneLink => 1,
            Self::Threads => 2,
            Self::Messenger => 3,
            Self::Facebook => 4,
            Self::Instagram => 5,
        }
    }
}

#[cfg(windows)]
fn open_app_id(id: &str) -> Result<(), String> {
    use windows::{
        core::{w, PCWSTR},
        Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
    };
    let target = format!("shell:AppsFolder\\{id}");
    let wide = target.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(wide.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize <= 32 {
        return Err(format!("無法開啟通知來源 {id}"));
    }
    Ok(())
}
#[cfg(not(windows))]
fn open_app_id(_id: &str) -> Result<(), String> {
    Err("社群原生程式暫僅支援 Windows".into())
}

/// 只放行聊天與內容類協定，避免通知內容被誘導去開本機檔案或命令。
fn is_safe_launch(launch: &str) -> bool {
    let Some((scheme, _)) = launch.split_once(':') else {
        return false;
    };
    matches!(
        scheme.trim().to_ascii_lowercase().as_str(),
        "line"
            | "ms-phone"
            | "ms-settings"
            | "https"
            | "http"
            | "fb"
            | "facebook"
            | "fb-messenger"
            | "fb-messenger-share"
            | "instagram"
            | "instagram-stories"
            | "threads"
            | "messenger"
            | "msteams"
            | "teams"
            | "spotify"
    )
}

#[cfg(windows)]
fn open_uri(url: &str) -> Result<(), String> {
    use windows::{
        core::{w, PCWSTR},
        Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
    };
    let wide = url.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(wide.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize <= 32 {
        return Err("無法開啟社群連結，請檢查 Windows 預設應用程式設定".into());
    }
    Ok(())
}
#[cfg(not(windows))]
fn open_uri(_url: &str) -> Result<(), String> {
    Err("社群連結暫僅支援 Windows".into())
}

/// LINE 最小化或收進系統匣時，Shell 的 AppID 喚起不一定會還原主視窗，
/// 這裡只做「還原既有主視窗並切到前景」，不模擬滑鼠鍵盤，也不碰聊天內容。
#[cfg(windows)]
fn focus_line_window() {
    use windows::Win32::{
        Foundation::{BOOL, HWND, LPARAM, RECT},
        System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        },
        UI::WindowsAndMessaging::*,
    };
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return;
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut pid = None;
        let mut more = Process32FirstW(snapshot, &mut entry).is_ok();
        while more {
            let len = entry
                .szExeFile
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(entry.szExeFile.len());
            if String::from_utf16_lossy(&entry.szExeFile[..len]).eq_ignore_ascii_case("LINE.exe") {
                pid = Some(entry.th32ProcessID);
                break;
            }
            more = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = windows::Win32::Foundation::CloseHandle(snapshot);
        let Some(pid) = pid else { return };
        struct Find {
            pid: u32,
            best: Option<(i64, HWND)>,
        }
        unsafe extern "system" fn visit(hwnd: HWND, data: LPARAM) -> BOOL {
            let find = &mut *(data.0 as *mut Find);
            let mut owner = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut owner));
            let mut class = [0u16; 64];
            let len = GetClassNameW(hwnd, &mut class);
            let class = String::from_utf16_lossy(&class[..len.max(0) as usize]);
            if owner == find.pid && class == "AllInOneWindow" && GetWindow(hwnd, GW_OWNER).is_err()
            {
                let mut rect = RECT::default();
                let _ = GetWindowRect(hwnd, &mut rect);
                let area = i64::from(rect.right - rect.left) * i64::from(rect.bottom - rect.top);
                if area > 0 && find.best.is_none_or(|(old, _)| area > old) {
                    find.best = Some((area, hwnd));
                }
            }
            true.into()
        }
        let mut find = Find { pid, best: None };
        let _ = EnumWindows(Some(visit), LPARAM((&mut find as *mut Find) as isize));
        if let Some((_, window)) = find.best {
            let _ = ShowWindowAsync(
                window,
                if IsIconic(window).as_bool() {
                    SW_RESTORE
                } else {
                    SW_SHOW
                },
            );
            let _ = SetForegroundWindow(window);
        }
    }
}
#[cfg(not(windows))]
fn focus_line_window() {}

/// 由通知的應用程式名稱與 AUMID 判斷來源；Messenger 先於 Facebook，避免誤判。
pub fn detect(app: &str, id: &str) -> Option<Social> {
    let text = format!("{} {}", app, id).to_lowercase();
    let app = app.trim().to_lowercase();
    let id = id.to_lowercase();
    if text.contains("messenger") {
        return Some(Social::Messenger);
    }
    if text.contains("phonelink")
        || text.contains("phone link")
        || text.contains("yourphone")
        || text.contains("your phone")
        || app.contains("手機連結")
    {
        return Some(Social::PhoneLink);
    }
    if text.contains("threads") {
        return Some(Social::Threads);
    }
    if text.contains("instagram") || text.contains("insta gram") {
        return Some(Social::Instagram);
    }
    if text.contains("naver.line")
        || app == "line"
        || id.ends_with(".line")
        || id.contains("line.messenger")
    {
        return Some(Social::Line);
    }
    if text.contains("facebook") {
        return Some(Social::Facebook);
    }
    None
}

/// 瀏覽器把網站通知送進 Windows 通知中心時，AUMID 只會是 Chrome／Edge，
/// 因此改看標題（通知正文第一段）來判斷是哪一個社群網站。
pub fn detect_browser_content(app: &str, text: &str) -> Option<Social> {
    let app = app.trim().to_lowercase();
    let browser = app.contains("chrome")
        || app.contains("chromium")
        || app.contains("edge")
        || app.contains("firefox")
        || app.contains("brave")
        || app.contains("opera")
        || app.contains("vivaldi");
    if !browser {
        return None;
    }
    let head = text
        .split(" · ")
        .next()
        .unwrap_or(text)
        .trim()
        .to_lowercase();
    let title = if head.is_empty() {
        text.to_lowercase()
    } else {
        head
    };
    if title == "threads" || title.starts_with("threads ") || title.contains("threads.com") {
        return Some(Social::Threads);
    }
    if title == "messenger" || title.contains("messenger.com") {
        return Some(Social::Messenger);
    }
    if title == "facebook" || title.starts_with("facebook ") || title.contains("facebook.com") {
        return Some(Social::Facebook);
    }
    if title == "instagram" || title.starts_with("instagram ") || title.contains("instagram.com") {
        return Some(Social::Instagram);
    }
    if title == "line" || title.starts_with("line ") || title.contains("line.me") {
        return Some(Social::Line);
    }
    None
}

/// 手機連結把其他手機 App 的通知轉送到電腦時，AUMID 一律是 YourPhone，
/// 真正來源只寫在正文第一段（轉送的 App 名稱，例如「Instagram · 杜貓 · 透過 手機連結」）。
/// 因此先照原本規則判斷，判定為手機連結時再按正文改派到對應社群；認不出來才留在手機連結。
pub fn detect_relayed(app: &str, id: &str, text: &str) -> Option<Social> {
    match detect(app, id) {
        Some(Social::PhoneLink) => relayed_platform(text).or(Some(Social::PhoneLink)),
        Some(source) => Some(source),
        None => detect_browser_content(app, text),
    }
}

/// 只看正文第一段（通知標題）是否是某個社群平台的名稱，避免訊息內文提到平台就被誤判。
fn relayed_platform(text: &str) -> Option<Social> {
    let head = text.split(" · ").next().unwrap_or("").trim().to_lowercase();
    if head.is_empty() {
        return None;
    }
    let candidates: [(Social, &[&str]); 5] = [
        (
            Social::Instagram,
            &["instagram", "insta gram", "instagram.com", "ig", "ig "],
        ),
        (Social::Threads, &["threads", "threads.net"]),
        (Social::Messenger, &["messenger", "messenger.com"]),
        (Social::Facebook, &["facebook", "facebook.com"]),
        (Social::Line, &["line", "line.me"]),
    ];
    candidates.into_iter().find_map(|(social, names)| {
        names
            .iter()
            .any(|name| {
                head == *name
                    || head.starts_with(&format!("{name} "))
                    || head.starts_with(&format!("{name}（"))
                    || head.starts_with(&format!("{name}("))
            })
            .then_some(social)
    })
}

/// 每個來源的未讀則數；點開應用程式或到通知中心查看後歸零。
#[derive(Clone, Debug)]
pub struct Badges {
    counts: [u16; 6],
    ids: [Option<String>; 6],
    launches: [Option<String>; 6],
}
impl Default for Badges {
    fn default() -> Self {
        Self {
            counts: [0; 6],
            ids: std::array::from_fn(|_| None),
            launches: std::array::from_fn(|_| None),
        }
    }
}
impl Badges {
    pub fn load() -> Self {
        let mut badges = Self::load_raw();
        // 舊版把手機連結轉送的通知一律算成「手機連結」，升級後改按正文分派到 Instagram／Threads 等；
        // 第一次載入清掉手機連結的舊計數，實際數量會由啟動時的通知中心快照補回。
        let marker = crate::model::config_dir().join("social-badges.relay-split");
        if !marker.exists() {
            badges.counts[Social::PhoneLink.index()] = 0;
            badges.ids[Social::PhoneLink.index()] = None;
            badges.launches[Social::PhoneLink.index()] = None;
            let _ = std::fs::write(&marker, "1");
        }
        badges
    }
    fn load_raw() -> Self {
        let path = crate::model::config_dir().join("social-badges.json");
        let Ok(raw) = std::fs::read(path) else {
            return Self::default();
        };
        let Ok(list) = serde_json::from_slice::<Vec<serde_json::Value>>(&raw) else {
            return Self::default();
        };
        let mut badges = Self::default();
        for item in list {
            let Some(key) = item.get("app").and_then(|v| v.as_str()) else {
                continue;
            };
            let Some(count) = item.get("count").and_then(|v| v.as_u64()) else {
                continue;
            };
            if let Some(social) = Social::ALL.into_iter().find(|s| s.key() == key) {
                badges.counts[social.index()] = count.min(9999) as u16;
                badges.ids[social.index()] = item
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|id| !id.is_empty() && *id != key)
                    .map(|id| id.to_string());
                badges.launches[social.index()] = item
                    .get("launch")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|launch| !launch.is_empty())
                    .map(|launch| launch.to_string());
            }
        }
        badges
    }
    fn save(&self) {
        let list: Vec<_> = Social::ALL
            .into_iter()
            .filter(|s| self.counts[s.index()] > 0)
            .map(|s| {
                let mut item = serde_json::json!({"app":s.key(),"count":self.counts[s.index()]});
                if let Some(id) = &self.ids[s.index()] {
                    item["id"] = serde_json::Value::String(id.clone());
                }
                if let Some(launch) = &self.launches[s.index()] {
                    item["launch"] = serde_json::Value::String(launch.clone());
                }
                item
            })
            .collect();
        let path = crate::model::config_dir().join("social-badges.json");
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(data) = serde_json::to_vec(&list) {
            let _ = crate::model::atomic_write(&path, &data);
        }
    }
    pub fn bump(&mut self, social: Social, id: &str) {
        self.bump_with_launch(social, id, None);
    }
    pub fn bump_with_launch(&mut self, social: Social, id: &str, launch: Option<&str>) {
        let slot = &mut self.counts[social.index()];
        *slot = slot.saturating_add(1).min(9999);
        if let Some(id) = social.app_target(Some(id)) {
            self.ids[social.index()] = Some(id.to_owned());
        }
        if let Some(launch) = launch.map(str::trim).filter(|launch| !launch.is_empty()) {
            self.launches[social.index()] = Some(launch.to_owned());
        }
    }
    /// 啟動時把通知中心裡已經存在的未讀補上；只補不減，避免蓋掉即時通知。
    pub fn seed(&mut self, social: Social, count: u16, id: Option<&str>, launch: Option<&str>) {
        let slot = &mut self.counts[social.index()];
        if count > *slot {
            *slot = count.min(9999);
        }
        if let Some(id) = id.and_then(|id| social.app_target(Some(id))) {
            self.ids[social.index()] = Some(id.to_owned());
        }
        if let Some(launch) = launch.map(str::trim).filter(|launch| !launch.is_empty()) {
            self.launches[social.index()] = Some(launch.to_owned());
        }
    }
    pub fn clear(&mut self, social: Social) {
        self.counts[social.index()] = 0;
        self.ids[social.index()] = None;
        self.launches[social.index()] = None;
    }
    /// 最後一則通知帶的深層連結；點開後就清掉，避免下次誤開舊對話。
    pub fn launch(&self, social: Social) -> Option<String> {
        self.launches[social.index()].clone()
    }
    pub fn persist(&self) {
        self.save();
    }
    pub fn count(&self, social: Social) -> u16 {
        self.counts[social.index()]
    }
    /// 先記住實際發出通知的 AppID；舊資料沒有 id 時，從已觀察到的通知來源清單補上。
    pub fn target(&self, social: Social) -> Option<String> {
        self.ids[social.index()]
            .clone()
            .or_else(|| known_app(social))
    }
    pub fn active(&self) -> Vec<(Social, u16)> {
        Social::ALL
            .into_iter()
            .filter_map(|s| {
                let n = self.count(s);
                (n > 0).then_some((s, n))
            })
            .collect()
    }
    /// 依設定的優先順序回傳目前有未讀的來源；左邊先顯示。
    pub fn active_ordered(&self, order: &[String]) -> Vec<(Social, u16)> {
        Social::ordered(order)
            .into_iter()
            .filter_map(|s| {
                let n = self.count(s);
                (n > 0).then_some((s, n))
            })
            .collect()
    }
    pub fn total(&self) -> u32 {
        self.active().iter().map(|(_, n)| *n as u32).sum()
    }
}

fn known_app(social: Social) -> Option<String> {
    let raw = std::fs::read(crate::model::config_dir().join("notification-apps.json")).ok()?;
    let list = serde_json::from_slice::<Vec<serde_json::Value>>(&raw).ok()?;
    list.into_iter().find_map(|item| {
        let id = item.get("id")?.as_str()?.trim().to_owned();
        let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
        detect(name, &id)
            .filter(|source| *source == social)
            .map(|_| id)
    })
}

/// 右上角數字徽章；`count` 為 0 時畫小紅點。
pub fn badge(c: &Card, x: f32, y: f32, count: u16, radius: f32, font: f32) {
    use crate::theme::W;
    let red = egui::Color32::from_rgb(255, 59, 48);
    if count == 0 {
        c.circle(x, y, radius * 0.45, red);
        return;
    }
    let text = if count > 99 {
        "99+".to_string()
    } else {
        count.to_string()
    };
    // 兩位數以上改用短藥丸，避免「99+」撐破圓形徽章、看起來比圖示還大。
    let tw = c.text_w(&text, font, W::Bold);
    if tw > radius * 1.7 {
        let w = (tw + 5.0).max(radius * 2.0);
        let h = radius * 1.8;
        c.fill(x - w / 2.0, y - h / 2.0, w, h, h / 2.0, red);
    } else {
        c.circle(x, y, radius, red);
    }
    c.centered_text(
        x,
        y,
        &text,
        font,
        W::Bold,
        egui::Color32::WHITE,
        crate::paint::H::Center,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detects_the_six_sources_without_mixing_facebook_and_messenger() {
        assert_eq!(
            detect("Threads", "com.burbn.threads"),
            Some(Social::Threads)
        );
        assert_eq!(
            detect("Messenger", "com.facebook.messenger"),
            Some(Social::Messenger)
        );
        assert_eq!(
            detect("Facebook", "com.facebook.katana"),
            Some(Social::Facebook)
        );
        assert_eq!(detect("LINE", "jp.naver.line"), Some(Social::Line));
        assert_eq!(
            detect("Instagram", "com.instagram.android"),
            Some(Social::Instagram)
        );
        assert_eq!(
            detect("手機連結", "Microsoft.YourPhone_8wekyb3d8bbwe!App"),
            Some(Social::PhoneLink)
        );
        assert_eq!(
            detect("Phone Link", "Microsoft.YourPhone_8wekyb3d8bbwe!App"),
            Some(Social::PhoneLink)
        );
        assert_eq!(detect("LINE", "jp.naver.line"), Some(Social::Line));
        assert_eq!(detect("Outlook", "Microsoft.Outlook"), None);
    }
    #[test]
    fn phone_link_relayed_notifications_are_split_by_the_first_text_line() {
        let body = "Instagram · 杜貓 · 透過 手機連結";
        assert_eq!(
            detect_relayed("手機連結", "Microsoft.YourPhone_8wekyb3d8bbwe!App", body),
            Some(Social::Instagram)
        );
        assert_eq!(
            detect_relayed(
                "手機連結",
                "Microsoft.YourPhone_8wekyb3d8bbwe!App",
                "Threads · 阿明 · 透過 手機連結"
            ),
            Some(Social::Threads)
        );
        assert_eq!(
            detect_relayed(
                "手機連結",
                "Microsoft.YourPhone_8wekyb3d8bbwe!App",
                "Messenger · 小美 · 透過 手機連結"
            ),
            Some(Social::Messenger)
        );
        assert_eq!(
            detect_relayed(
                "手機連結",
                "Microsoft.YourPhone_8wekyb3d8bbwe!App",
                "Facebook · 社團新貼文"
            ),
            Some(Social::Facebook)
        );
        assert_eq!(
            detect_relayed(
                "手機連結",
                "Microsoft.YourPhone_8wekyb3d8bbwe!App",
                "LINE · 家人群組 · 3 則訊息"
            ),
            Some(Social::Line)
        );
        assert_eq!(
            detect_relayed(
                "手機連結",
                "Microsoft.YourPhone_8wekyb3d8bbwe!App",
                "簡訊 · 0912… · 透過 手機連結"
            ),
            Some(Social::PhoneLink)
        );
        assert_eq!(
            detect_relayed(
                "手機連結",
                "Microsoft.YourPhone_8wekyb3d8bbwe!App",
                "王小明 · 你的 Instagram 驗證碼是 123456"
            ),
            Some(Social::PhoneLink),
            "內文提到平台不能當成來源"
        );
        assert_eq!(
            detect_relayed("Google Chrome", "Chrome", "Instagram · 小美 傳送訊息"),
            Some(Social::Instagram)
        );
    }

    #[test]
    fn order_keeps_the_saved_priority_and_fills_missing_sources() {
        let order = Social::parse_order("phonelink,line,unknown,phonelink");
        assert_eq!(order[0], "phonelink");
        assert_eq!(order[1], "line");
        assert_eq!(order.len(), Social::ALL.len());
        assert_eq!(Social::ordered(&order)[0], Social::PhoneLink);
    }

    #[test]
    fn counts_survive_a_round_trip_and_clear() {
        let mut badges = Badges::default();
        badges.bump(Social::Threads, "com.burbn.threads");
        badges.bump(Social::Threads, "com.burbn.threads");
        badges.bump(Social::Line, "NAVER.WIN32_LINEwin8_8ptj331gd3tyt!LINE");
        assert_eq!(badges.count(Social::Threads), 2);
        assert_eq!(badges.total(), 3);
        assert_eq!(
            badges.ids[Social::Line.index()].as_deref(),
            Some("NAVER.WIN32_LINEwin8_8ptj331gd3tyt!LINE")
        );
        badges.clear(Social::Threads);
        assert_eq!(badges.count(Social::Threads), 0);
        assert_eq!(badges.total(), 1);
    }
    #[test]
    fn a_notification_deep_link_is_kept_until_the_badge_is_opened() {
        let mut badges = Badges::default();
        badges.bump_with_launch(
            Social::Line,
            "NAVER.WIN32_LINEwin8_8ptj331gd3tyt!LINE",
            Some(" line://chat/?mid=bank "),
        );
        assert_eq!(
            badges.launch(Social::Line).as_deref(),
            Some("line://chat/?mid=bank")
        );
        assert_eq!(badges.count(Social::Line), 1);
        badges.clear(Social::Line);
        assert_eq!(badges.launch(Social::Line), None);
        assert_eq!(badges.count(Social::Line), 0);
    }
    #[test]
    fn browser_notifications_are_classified_by_their_title() {
        assert_eq!(
            detect_browser_content("Google Chrome", "Threads · 小明 愛心你的貼文"),
            Some(Social::Threads)
        );
        assert_eq!(
            detect_browser_content("Microsoft Edge", "Instagram · 小美 傳送訊息"),
            Some(Social::Instagram)
        );
        assert_eq!(
            detect_browser_content("Firefox", "Messenger · 阿宏 傳送訊息"),
            Some(Social::Messenger)
        );
        assert_eq!(
            detect_browser_content("Brave", "LINE · 台新銀行 傳送訊息"),
            Some(Social::Line)
        );
        assert_eq!(
            detect_browser_content("LINE", "Threads · 小明 愛心你的貼文"),
            None,
            "桌面程式不靠正文猜來源"
        );
    }
    #[test]
    fn seeding_from_the_notification_center_only_raises_counts() {
        let mut badges = Badges::default();
        badges.seed(
            Social::Threads,
            4,
            Some("handler"),
            Some("threads://notification"),
        );
        assert_eq!(badges.count(Social::Threads), 4);
        assert_eq!(
            badges.launch(Social::Threads).as_deref(),
            Some("threads://notification")
        );
        badges.bump(Social::Threads, "handler");
        badges.seed(Social::Threads, 2, Some("handler"), None);
        assert_eq!(
            badges.count(Social::Threads),
            5,
            "較舊的快照不能把新通知蓋掉"
        );
    }
    #[test]
    fn only_chat_and_web_protocols_are_opened_from_notifications() {
        assert!(is_safe_launch("line://chat/?mid=bank"));
        assert!(is_safe_launch("ms-phone:phonenotifications?deviceid=1"));
        assert!(is_safe_launch("https://example.com/post/1"));
        assert!(!is_safe_launch("file:///C:/Windows/System32/cmd.exe"));
        assert!(!is_safe_launch("cmd:calc.exe"));
        assert!(!is_safe_launch("javascript:alert(1)"));
        assert!(!is_safe_launch(""));
    }
    #[test]
    fn launch_uses_the_notifying_desktop_app_and_ignores_placeholder_keys() {
        let id = "NAVER.WIN32_LINEwin8_8ptj331gd3tyt!LINE";
        assert_eq!(Social::Line.app_target(Some(id)), Some(id));
        assert_eq!(Social::Line.app_target(Some("line")), None);
        assert_eq!(Social::Line.app_target(None), None);
    }
}

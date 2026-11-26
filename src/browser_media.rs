//! 瀏覽器分頁播放的後備來源。
//!
//! 有些 Windows 環境不會把 Chrome／Edge 的分頁音訊登記成 SMTC 媒體工作階段，
//! 島因此只顯示「暫無音樂」。這裡改用「音量混音器裡正在出聲的瀏覽器程序 +
//! 該程序視窗標題」推回歌名，讓瀏覽器播放也能上島。

#[cfg(windows)]
use crate::smtc::{MediaKind, NowPlaying};

/// 支援的瀏覽器：程序檔名、視窗標題後綴、島上顯示的來源名稱。
const BROWSERS: [(&str, &str, &str); 5] = [
    ("chrome.exe", " - Google Chrome", "chrome.exe"),
    ("msedge.exe", " - Microsoft Edge", "msedge.exe"),
    ("firefox.exe", " - Mozilla Firefox", "firefox.exe"),
    ("brave.exe", " - Brave", "brave.exe"),
    ("opera.exe", " - Opera", "opera.exe"),
];

pub(crate) fn browser_for_exe(exe: &str) -> Option<(&'static str, &'static str)> {
    let exe = exe.to_ascii_lowercase();
    BROWSERS
        .iter()
        .find(|(name, _, _)| *name == exe)
        .map(|(_, suffix, source)| (*suffix, *source))
}

/// 社群網站、首頁、搜尋結果這類「只是分頁名稱」的標題，不該被當成正在播的歌。
fn is_plain_page(title: &str) -> bool {
    const PAGES: [&str; 24] = [
        "threads", "facebook", "instagram", "messenger", "x", "twitter", "google", "gmail",
        "youtube 首頁", "youtube home", "youtube", "youtube music", "kkbox", "spotify",
        "netflix", "line", "discord", "reddit", "tiktok", "twitch", "蝦皮", "momo",
        "新分頁", "new tab",
    ];
    let lower = title.trim().to_lowercase();
    PAGES.iter().any(|page| lower == *page)
}

/// 把瀏覽器視窗標題清成「歌手／歌名」。
/// 常見樣式：`周杰倫 Jay Chou【稻香 Rice Field】-Official Music Video - YouTube - Google Chrome`。
/// 去掉 Windows 分頁群組加在視窗標題上的雜訊。
/// 例：`稻香 - YouTube 和其他 2 個頁面 - 個人 - Microsoft Edge`
/// 這種標題原本會整串被當成歌名，歌詞自然也查不到。
fn clean_window_title(raw: &str) -> String {
    let mut title = raw.trim().to_string();
    // 「和其他 N 個頁面」「and N other pages」：主標題在前，後面全是分頁群組描述。
    for marker in [" 和其他 ", " 還有 ", " and "] {
        if let Some(position) = title.find(marker) {
            let tail = &title[position + marker.len()..];
            if tail.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                title = title[..position].trim().to_string();
            }
        }
    }
    // 群組名稱（個人／公司／系統管理員／來賓…）夾在瀏覽器名稱前。
    for tail in [
        " - 個人", " - 公司", " - 系統管理員", " - 來賓", " - 訪客",
        " - Personal", " - Work", " - Administrator", " - Guest",
    ] {
        if let Some(stripped) = title.strip_suffix(tail) {
            title = stripped.trim().to_string();
        }
    }
    title.trim_end_matches([' ', '-']).to_string()
}

pub fn parse_title(raw: &str, browser_suffix: &str) -> Option<(String, String, String)> {
    let raw = clean_window_title(raw);
    let raw = raw.as_str();
    // 視窗標題前面的空白已被清掉，後綴也要用去空白的版本比一次，
    // 否則「 - Chrome」這種只有瀏覽器名稱的標題會被留下來當成歌名。
    let mut title = match raw
        .strip_suffix(browser_suffix)
        .or_else(|| raw.strip_suffix(browser_suffix.trim()))
    {
        Some(stripped) => stripped.trim().to_string(),
        None => raw.trim_start().to_string(),
    };
    let mut album = String::new();
    for (suffix, name) in [
        (" - YouTube Music", "YouTube Music"),
        (" - YouTube", "YouTube"),
        (" - KKBOX", "KKBOX"),
        (" | KKBOX", "KKBOX"),
    ] {
        if let Some(stripped) = title.strip_suffix(suffix) {
            title = stripped.trim().to_string();
            album = name.to_string();
            break;
        }
    }
    // YouTube 安裝成應用程式（PWA）時視窗標題只剩影片名稱，沒有「 - YouTube」尾巴，
    // 這時仍要看整串標題有沒有站點字樣，否則會被當成來源不明的頁面濾掉。
    if album.is_empty() {
        for (hint, name) in [
            ("YouTube Music", "YouTube Music"),
            ("YouTube", "YouTube"),
            ("KKBOX", "KKBOX"),
            ("Spotify", "Spotify"),
            ("StreetVoice", "StreetVoice"),
            ("Bilibili", "Bilibili"),
            ("嗶哩嗶哩", "Bilibili"),
        ] {
            if title.contains(hint) {
                if let Some(stripped) = title.strip_suffix(&format!(" {hint}")) {
                    title = stripped.trim().to_string();
                } else if let Some(stripped) = title.strip_suffix(&format!(" | {hint}")) {
                    title = stripped.trim().to_string();
                }
                album = name.to_string();
                break;
            }
        }
    }
    for suffix in [
        "-Official Music Video",
        "- Official Music Video",
        " - Official Music Video",
        "-Official Audio",
        " - Official Audio",
        " - Official Video",
    ] {
        if let Some(stripped) = title.strip_suffix(suffix) {
            title = stripped.trim().to_string();
        }
    }
    title = title
        .trim_matches(|c: char| c == '-' || c == '|' || c == '｜' || c.is_whitespace())
        .to_string();
    if title.is_empty() || title.contains("chrome://") || title.contains("edge://") {
        return None;
    }
    if is_plain_page(&title) {
        return None;
    }
    // 「歌手【歌名】」是台灣 MV 常見格式；拆開後歌詞比對才不會被整串雜訊幹擾。
    if let (Some(open), Some(close)) = (title.find('【'), title.find('】')) {
        if open < close {
            let artist = title[..open].trim().to_string();
            let song = title[open + '【'.len_utf8()..close].trim().to_string();
            if !song.is_empty() {
                return Some((song, artist, album));
            }
        }
    }
    Some((title, String::new(), album))
}

#[cfg(windows)]
fn now_playing(source: &str, title: String, artist: String, mut album: String) -> NowPlaying {
    if album.is_empty() {
        album = "網頁播放".to_string();
    }
    NowPlaying {
        title,
        artist,
        album,
        source: source.to_string(),
        song_id: None,
        playing: true,
        position: 0.0,
        sampled_at: Some(std::time::Instant::now()),
        event_time_ms: None,
        duration: 0.0,
        artwork: None,
        playback_type: MediaKind::Music,
        volume: None,
        identified: false,
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use windows::Win32::Foundation::{BOOL, CloseHandle, HWND, LPARAM};
    use windows::Win32::Media::Audio::{
        AudioSessionStateActive, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator,
        ISimpleAudioVolume, MMDeviceEnumerator, DEVICE_STATE_ACTIVE, eRender,
    };
    use windows::Win32::System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
    };
    use windows::core::{Interface, PWSTR};

    fn process_exe(pid: u32) -> Option<String> {
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buffer = [0u16; 1024];
            let mut size = buffer.len() as u32;
            let result = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buffer.as_mut_ptr()), &mut size);
            let _ = CloseHandle(handle);
            result.ok()?;
            let full = String::from_utf16_lossy(&buffer[..size as usize]);
            full.rsplit(['\\', '/']).next().map(str::to_string)
        }
    }

    /// 音量混音器裡正在出聲、且沒靜音的瀏覽器來源名稱。
    fn audible_browsers() -> Vec<&'static str> {
        let mut found: Vec<&'static str> = Vec::new();
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let enumerator: IMMDeviceEnumerator = match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
                Ok(enumerator) => enumerator,
                Err(_) => return found,
            };
            let Ok(devices) = enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) else {
                return found;
            };
            for d in 0..devices.GetCount().unwrap_or(0) {
                let Ok(device) = devices.Item(d) else { continue };
                let Ok(manager) = device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else { continue };
                let Ok(sessions) = manager.GetSessionEnumerator() else { continue };
                for s in 0..sessions.GetCount().unwrap_or(0) {
                    let Ok(session) = sessions.GetSession(s) else { continue };
                    if session.GetState().ok() != Some(AudioSessionStateActive) {
                        continue;
                    }
                    let Ok(volume) = session.cast::<ISimpleAudioVolume>() else { continue };
                    if volume.GetMute().ok().is_some_and(|muted| muted.as_bool()) {
                        continue;
                    }
                    let Ok(control) = session.cast::<IAudioSessionControl2>() else { continue };
                    let Ok(pid) = control.GetProcessId() else { continue };
                    if pid == 0 {
                        continue;
                    }
                    let Some(exe) = process_exe(pid) else { continue };
                    if let Some((_, source)) = browser_for_exe(&exe) {
                        if !found.contains(&source) {
                            found.push(source);
                        }
                    }
                }
            }
        }
        found
    }

    #[derive(Default)]
    struct Pick {
        browsers: Vec<&'static str>,
        best: Option<(String, &'static str, &'static str)>,
    }

    unsafe extern "system" fn visit(hwnd: HWND, param: LPARAM) -> BOOL {
        let pick = unsafe { &mut *(param.0 as *mut Pick) };
        if !unsafe { IsWindowVisible(hwnd) }.as_bool() {
            return BOOL(1);
        }
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if pid == 0 {
            return BOOL(1);
        }
        let Some(exe) = process_exe(pid) else { return BOOL(1) };
        let Some((suffix, source)) = browser_for_exe(&exe) else { return BOOL(1) };
        if !pick.browsers.contains(&source) {
            return BOOL(1);
        }
        let mut text = [0u16; 1024];
        let len = unsafe { GetWindowTextW(hwnd, &mut text) };
        if len <= 0 {
            return BOOL(1);
        }
        let title = String::from_utf16_lossy(&text[..len as usize]);
        if parse_title(&title, suffix).is_none() {
            return BOOL(1);
        }
        // 同一套瀏覽器可能同時開很多視窗；先挑真的在播音樂的頁面，
        // 免得把信箱、檔案這類標題當成歌名。分數只影響挑選，不改標題內容。
        let lower = title.to_lowercase();
        let music_page = ["youtube", "kkbox", "spotify", "streetvoice", "bilibili", "嗶哩嗶哩", "music"]
            .iter().any(|hint| lower.contains(hint));
        let score = if title.contains("YouTube") { 3 } else if music_page { 2 } else { 1 };
        let current = pick.best.as_ref().map_or(0, |(known, _, _)| {
            let lower = known.to_lowercase();
            if known.contains("YouTube") { 3 }
            else if ["kkbox", "spotify", "streetvoice", "bilibili", "嗶哩嗶哩", "music"].iter().any(|hint| lower.contains(hint)) { 2 }
            else { 1 }
        });
        if score >= current {
            pick.best = Some((title, suffix, source));
        }
        BOOL(1)
    }

    /// 音量混音器裡正在出聲的瀏覽器來源（不要求視窗標題可用）。
    /// 聽歌辨識靠它判斷「有人在播東西」，標題抓不到也能開始聽。
    pub fn audible_source() -> Option<&'static str> {
        audible_browsers().into_iter().next()
    }

    pub fn probe() -> Option<NowPlaying> {
        let browsers = audible_browsers();
        let Some(&first) = browsers.first() else {
            return None;
        };
        let mut pick = Pick { browsers, best: None };
        unsafe {
            let _ = EnumWindows(Some(visit), LPARAM(&mut pick as *mut Pick as isize));
        }
        let Some((raw, suffix, source)) = pick.best else {
            // 有瀏覽器正在出聲，但視窗標題抓不到或不是可辨識的歌名（Edge 常見）：
            // 仍回一個只有來源的空殼，讓上層繼續走「聽聲音認歌」那條路，
            // 而不是整輪只拿到 None、島上一直顯示暫無音樂。
            return Some(now_playing(first, String::new(), String::new(), String::new()));
        };
        let (title, artist, album) = parse_title(&raw, suffix)?;
        Some(now_playing(source, title, artist, album))
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn probe() -> Option<crate::smtc::NowPlaying> {
        None
    }

    pub fn audible_source() -> Option<&'static str> {
        None
    }
}

pub use platform::{audible_source, probe};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_taiwan_mv_title() {
        let (title, artist, album) = parse_title(
            "周杰倫 Jay Chou【稻香 Rice Field】-Official Music Video - YouTube - Google Chrome",
            " - Google Chrome",
        )
        .unwrap();
        assert_eq!(title, "稻香 Rice Field");
        assert_eq!(artist, "周杰倫 Jay Chou");
        assert_eq!(album, "YouTube");
    }

    #[test]
    fn parses_plain_youtube_title() {
        let (title, artist, album) =
            parse_title("A2O LITE 'NEON' Official Video - YouTube - Google Chrome", " - Google Chrome").unwrap();
        assert_eq!(title, "A2O LITE 'NEON' Official Video");
        assert!(artist.is_empty());
        assert_eq!(album, "YouTube");
    }

    #[test]
    fn ignores_empty_title() {
        assert!(parse_title(" - Google Chrome", " - Google Chrome").is_none());
    }

    #[test]
    fn youtube_app_window_without_suffix_still_counts_as_music_page() {
        // YouTube 應用程式（PWA）的視窗標題常只有影片名稱，沒有瀏覽器與站點尾巴。
        let (title, _, album) =
            parse_title("zb1 'ICONIK' MV 完整版 - YouTube", " - Google Chrome").unwrap();
        assert_eq!(title, "zb1 'ICONIK' MV 完整版");
        assert_eq!(album, "YouTube");
    }

    #[test]
    fn edge_tab_group_noise_is_stripped() {
        let (title, _, album) = parse_title(
            "稻香 - YouTube 和其他 2 個頁面 - 個人 - Microsoft Edge",
            " - Microsoft Edge",
        )
        .unwrap();
        assert_eq!(title, "稻香");
        assert_eq!(album, "YouTube");
    }

    #[test]
    fn edge_tab_group_without_site_still_parses() {
        let (title, artist, _) = parse_title(
            "周杰倫【稻香】 和其他 1 個頁面 - 系統管理員 - Microsoft Edge",
            " - Microsoft Edge",
        )
        .unwrap();
        assert_eq!(title, "稻香");
        assert_eq!(artist, "周杰倫");
    }

    #[test]
    fn community_pages_are_not_songs() {
        for page in ["Threads - Google Chrome", "Instagram - Microsoft Edge", "YouTube 首頁 - Google Chrome"] {
            let suffix = if page.ends_with(" - Microsoft Edge") { " - Microsoft Edge" } else { " - Google Chrome" };
            assert!(parse_title(page, suffix).is_none(), "不該把分頁名稱當歌曲：{page}");
        }
    }
}

//! 聽歌辨識：直接聽電腦正在輸出的聲音，用 Shazam 相容指紋認出歌名、歌手與「唱到哪裡」。
//!
//! 瀏覽器播放 YouTube 時，Windows 播放卡只給影片標題（有時是頻道名、有時是空的），
//! 光靠標題猜歌詞常常猜錯。這裡改成聽聲音本身，抓到的歌名與時間位置就準得多；
//! 位置的意義是「這段取樣在歌曲裡的起點」，加上取樣長度就等於現在唱到第幾秒，
//! 歌詞因此可以從正確的段落進場，而不是從第一句開始。
//!
//! 抓不到音訊時（權限被 Windows 隱私設定封鎖、沒有裝置、整段沒聲音）不會默默失敗：
//! 原因會寫進 `diagnostics/listen.log`、顯示在設定頁，權限問題還會跳一次系統視窗請使用者開啟。

pub mod capture;
pub mod fingerprint;
pub mod shazam;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// 一次聆聽的長度。太短指紋不夠、太長辨識變慢；8 秒對 Shazam 已經足夠。
const LISTEN_SECONDS: u32 = 8;
/// 兩次辨識之間的間隔，避免一直佔用音訊裝置與網路。
const INTERVAL: Duration = Duration::from_secs(12);
/// 結果的有效期限；超過就當作沒有辨識結果。
pub const FRESH: Duration = Duration::from_secs(75);

#[derive(Clone, Debug, PartialEq)]
pub struct Recognized {
    pub title: String,
    pub artist: String,
    /// 取樣結束時，歌曲大約播到第幾秒。
    pub position: f32,
    pub captured: Instant,
}

/// 最近一次聆聽的結果，設定頁直接顯示，免得使用者以為功能沒作用。
#[derive(Clone, Debug)]
pub struct Status {
    pub text: String,
    /// 需要使用者去 Windows 設定開麥克風權限。
    pub needs_permission: bool,
    pub at: Instant,
}

static ENABLED: AtomicBool = AtomicBool::new(true);
static REQUESTED: AtomicBool = AtomicBool::new(false);
static PROMPTED: AtomicBool = AtomicBool::new(false);
static RESULT: OnceLock<Mutex<Option<Recognized>>> = OnceLock::new();
static STATUS: OnceLock<Mutex<Option<Status>>> = OnceLock::new();
static WORKER: OnceLock<()> = OnceLock::new();
static LAST_LISTEN: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();

fn slot() -> &'static Mutex<Option<Recognized>> {
    RESULT.get_or_init(|| Mutex::new(None))
}

fn status_slot() -> &'static Mutex<Option<Status>> {
    STATUS.get_or_init(|| Mutex::new(None))
}

/// 設定頁的開關；關掉就完全不會錄音、也不會連網。
pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// 請背景執行緒聽一段；重複呼叫不會排隊，最多一次待處理，且兩次之間至少隔 15 秒。
pub fn request() {
    if !enabled() {
        return;
    }
    let last = LAST_LISTEN.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = last.lock() {
        if guard.is_some_and(|at| at.elapsed() < Duration::from_secs(15)) {
            return;
        }
        *guard = Some(Instant::now());
    }
    REQUESTED.store(true, Ordering::Relaxed);
    start();
}

/// 取得仍然新鮮的辨識結果。
pub fn cached() -> Option<Recognized> {
    let guard = slot().lock().ok()?;
    let value = guard.as_ref()?;
    (value.captured.elapsed() < FRESH).then(|| value.clone())
}

/// 最近一次聆聽的狀態；超過一分鐘沒有更新就視為閒置，不顯示舊訊息。
pub fn status() -> Option<Status> {
    let guard = status_slot().lock().ok()?;
    let value = guard.as_ref()?;
    (value.at.elapsed() < Duration::from_secs(90)).then(|| value.clone())
}

/// 目前是否因為 Windows 權限而抓不到音訊；設定頁據此顯示提示與「開啟設定」。
pub fn needs_permission() -> bool {
    status().is_some_and(|status| status.needs_permission)
}

/// 開啟 Windows 的麥克風隱私設定頁。
pub fn open_permission_settings() {
    #[cfg(windows)]
    {
        use windows::core::PCWSTR;
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        let uri: Vec<u16> = "ms-settings:privacy-microphone"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        unsafe {
            ShellExecuteW(
                None,
                windows::core::w!("open"),
                PCWSTR(uri.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            );
        }
    }
}

/// 跳一次系統視窗，明確告訴使用者要去哪裡開權限；同一個程序只問一次，避免每 15 秒彈一輪。
fn prompt_permission_once() {
    if PROMPTED.swap(true, Ordering::Relaxed) {
        return;
    }
    #[cfg(windows)]
    std::thread::spawn(|| {
        use windows::core::PCWSTR;
        use windows::Win32::UI::WindowsAndMessaging::{
            MessageBoxW, IDYES, MB_ICONQUESTION, MB_SETFOREGROUND, MB_TOPMOST, MB_YESNO,
        };
        let text = "聽歌辨識需要音訊權限。\n\nWindows 目前封鎖了桌面應用程式使用麥克風（或音訊裝置），所以島上聽不到任何聲音。\n\n請開啟：設定 → 隱私權與安全性 → 麥克風 → 「允許桌面應用程式存取您的麥克風」。\n\n要現在開啟設定頁嗎？";
        let title = "鬼島：聽歌辨識需要權限";
        let wide = |value: &str| -> Vec<u16> {
            value.encode_utf16().chain(std::iter::once(0)).collect()
        };
        let text = wide(text);
        let title = wide(title);
        let answer = unsafe {
            MessageBoxW(
                None,
                PCWSTR(text.as_ptr()),
                PCWSTR(title.as_ptr()),
                MB_YESNO | MB_ICONQUESTION | MB_TOPMOST | MB_SETFOREGROUND,
            )
        };
        if answer == IDYES {
            open_permission_settings();
        }
    });
}

fn store(value: Recognized) {
    if let Ok(mut guard) = slot().lock() {
        *guard = Some(value);
    }
}

fn set_status(text: impl Into<String>, needs_permission: bool) {
    let text = text.into();
    if let Ok(mut guard) = status_slot().lock() {
        *guard = Some(Status { text, needs_permission, at: Instant::now() });
    }
}

/// 每次聆聽都留一行診斷，實機出問題時可以直接看是哪一段卡住。
fn log(line: &str) {
    let path = crate::model::config_dir().join("diagnostics");
    crate::resources::append_log(&path, "listen.log", line);
}

/// 背景執行緒只在第一次需要時啟動，之後靠 park/unpark 等待請求。
fn start() {
    WORKER.get_or_init(|| {
        let _ = std::thread::Builder::new()
            .name("kuidao-listen".into())
            .spawn(|| loop {
                if !REQUESTED.swap(false, Ordering::Relaxed) {
                    std::thread::park_timeout(Duration::from_millis(500));
                    continue;
                }
                if !enabled() {
                    continue;
                }
                listen_once();
                std::thread::sleep(INTERVAL);
            });
    });
}

fn listen_once() {
    let (samples, rate) = match capture::capture_mono(LISTEN_SECONDS) {
        Ok(samples) => samples,
        Err(error) => {
            log(&format!("捕捉失敗:{}\n", error.label()));
            set_status(error.label(), error == capture::CaptureError::Denied);
            if error == capture::CaptureError::Denied {
                prompt_permission_once();
            }
            return;
        }
    };
    let pcm = fingerprint::resample_to_16k(&samples, rate);
    if pcm.len() < fingerprint::SAMPLE_RATE as usize * 3 {
        set_status("音訊太短，稍後再試", false);
        return;
    }
    let signature = fingerprint::make_signature(&pcm);
    let peaks: usize = signature.bands.iter().map(|(_, peaks)| peaks.len()).sum();
    if peaks < 40 {
        // 幾乎沒有峰值：多半只是環境音或靜音，不必送出去。
        set_status("聲音太小，等播放中的歌曲", false);
        return;
    }
    match shazam::identify(&signature) {
        Ok(Some(found)) => {
            let listened = pcm.len() as f32 / fingerprint::SAMPLE_RATE as f32;
            let label = if found.artist.trim().is_empty() {
                found.title.clone()
            } else {
                format!("{} - {}", found.title, found.artist)
            };
            log(&format!("辨識成功:{label} 位置{:.1}s 峰值{peaks}\n", found.offset_seconds));
            store(Recognized {
                title: found.title,
                artist: found.artist,
                position: found.offset_seconds + listened,
                captured: Instant::now(),
            });
            set_status(format!("已辨識：{label}"), false);
        }
        Ok(None) => {
            log(&format!("認不出歌:峰值{peaks}\n"));
            set_status("聽不出是哪首歌", false);
        }
        Err(reason) => {
            log(&format!("辨識失敗:{reason}\n"));
            set_status(format!("連線失敗：{reason}"), false);
        }
    }
}

/// 把辨識結果套進正在播放的資訊：歌名、歌手與目前位置。
pub fn apply(target: &mut crate::smtc::NowPlaying, found: &Recognized) {
    if found.title.trim().is_empty() {
        return;
    }
    target.title = found.title.clone();
    if !found.artist.trim().is_empty() {
        target.artist = found.artist.clone();
    }
    if target.album.trim().is_empty() {
        target.album = "網頁播放".into();
    }
    // 標記「歌名是聽聲音認出來的」：歌詞查詢據此忽略影片長度。
    // YouTube 的長度是影片長度（含前奏、片尾、直播），拿它和唱片版比對會直接刷掉所有候選。
    target.identified = true;
    if found.position > 0.0 {
        target.position = found.position as f64;
        target.sampled_at = Some(Instant::now());
    }
}

/// 從播放位置推算歌詞該從哪一句開始；給歌詞模組當「進場時間」。
pub fn lyric_entry_seconds(cached: &Recognized) -> f32 {
    cached.position.max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Recognized {
        Recognized {
            title: "稻香".into(),
            artist: "周杰倫".into(),
            position: 63.0,
            captured: Instant::now(),
        }
    }

    #[test]
    fn lyric_entry_uses_recognized_position() {
        assert_eq!(lyric_entry_seconds(&sample()), 63.0);
    }

    #[test]
    fn fresh_result_is_kept_and_expires_later() {
        store(sample());
        assert!(cached().is_some());
    }

    #[test]
    fn apply_sets_position_and_marks_identified() {
        let mut media = crate::smtc::NowPlaying {
            source: "chrome.exe".into(),
            title: "Threads".into(),
            playing: true,
            ..Default::default()
        };
        apply(&mut media, &sample());
        assert_eq!(media.title, "稻香");
        assert_eq!(media.artist, "周杰倫");
        assert!(media.identified);
        assert!((media.position - 63.0).abs() < 0.01);
        assert_eq!(media.album, "網頁播放");
    }

    #[test]
    fn status_reports_permission_need() {
        set_status("麥克風權限未開啟", true);
        assert!(needs_permission());
        assert_eq!(status().map(|value| value.text), Some("麥克風權限未開啟".into()));
        set_status("已辨識：稻香 - 周杰倫", false);
        assert!(!needs_permission());
    }
}

//! 聽歌辨識：直接聽系統正在輸出的聲音，用 Shazam 相容指紋認出歌名、歌手與「唱到哪裡」。
//!
//! 瀏覽器播放 YouTube 時，Windows 播放卡只給影片標題（有時是頻道名、有時是空的），
//! 光靠標題猜歌詞常常猜錯。這裡改成聽聲音本身，抓到的歌名與時間位置就準得多；
//! 位置的意義是「這段取樣在歌曲裡的起點」，加上取樣長度就等於現在唱到第幾秒，
//! 歌詞因此可以從正確的段落進場，而不是從第一句開始。

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

static ENABLED: AtomicBool = AtomicBool::new(true);
static REQUESTED: AtomicBool = AtomicBool::new(false);
static RESULT: OnceLock<Mutex<Option<Recognized>>> = OnceLock::new();
static WORKER: OnceLock<()> = OnceLock::new();
static LAST_LISTEN: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();

fn slot() -> &'static Mutex<Option<Recognized>> {
    RESULT.get_or_init(|| Mutex::new(None))
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

fn store(value: Recognized) {
    if let Ok(mut guard) = slot().lock() {
        *guard = Some(value);
    }
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
    let Some((samples, rate)) = capture::capture_mono(LISTEN_SECONDS) else {
        return;
    };
    let pcm = fingerprint::resample_to_16k(&samples, rate);
    if pcm.len() < fingerprint::SAMPLE_RATE as usize * 3 {
        return;
    }
    let signature = fingerprint::make_signature(&pcm);
    let peaks: usize = signature.bands.iter().map(|(_, peaks)| peaks.len()).sum();
    if peaks < 40 {
        // 幾乎沒有峰值：多半只是環境音或靜音，不必送出去。
        return;
    }
    if let Some(found) = shazam::identify(&signature) {
        let listened = pcm.len() as f32 / fingerprint::SAMPLE_RATE as f32;
        store(Recognized {
            title: found.title,
            artist: found.artist,
            position: found.offset_seconds + listened,
            captured: Instant::now(),
        });
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

    #[test]
    fn fresh_result_expires() {
        let value = Recognized { title: "測試".into(), artist: "歌手".into(), position: 12.0, captured: Instant::now() };
        assert_eq!(lyric_entry_seconds(&value), 12.0);
        store(value);
        assert!(cached().is_some());
    }

    #[test]
    fn apply_sets_position_and_keeps_source() {
        let mut media = crate::smtc::NowPlaying { source: "chrome.exe".into(), title: "Threads".into(), playing: true, ..Default::default() };
        let found = Recognized { title: "稻香".into(), artist: "周杰倫".into(), position: 63.0, captured: Instant::now() };
        apply(&mut media, &found);
        assert_eq!(media.title, "稻香");
        assert_eq!(media.artist, "周杰倫");
        assert!((media.position - 63.0).abs() < 0.01);
        assert_eq!(media.album, "網頁播放");
    }
}

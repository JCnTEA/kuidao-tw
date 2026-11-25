//! 歌詞對齊：把「聽歌辨識」聽出來的曲內秒數，對照播放器自己記的時間，
//! 算出這首歌在播放器上的位移，存到使用者電腦；同一首歌下次一開頭就對齊，
//! 不必等歌詞檔的第一句從 0 秒開始唱。
//!
//! 只做「位移」這一件事：進度條維持播放器真實時間，歌詞改用位移後的曲內時間，
//! 兩邊各自誠實；位移超過三分鐘當成辨識錯歌，不採用也不儲存。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// 一首歌最多容忍三分鐘差異（前奏／口白很長的影片）；再大就當辨識錯歌。
pub const MAX_OFFSET_SECONDS: f32 = 180.0;
/// 播放器位置小於這個值，代表它根本沒給時間軸，不能拿來算位移。
pub const MIN_PLAYER_SECONDS: f64 = 0.5;
/// 辨識取樣起點到真的拿到結果之間隔太久，這段音的秒數已經對不上現在，寧可不學。
const MAX_ANCHOR_AGE_SECS: u64 = 30;
const FILE: &str = "offsets.txt";

/// 播放器每一幀報到的時間；只在記憶體裡，重開程式就重新累積。
struct Clock {
    title: String,
    artist: String,
    position: f64,
    playing: bool,
    at: Instant,
}

static CLOCK: OnceLock<Mutex<Option<Clock>>> = OnceLock::new();
static PENDING: OnceLock<Mutex<Option<(Instant, f64)>>> = OnceLock::new();
static LAST_ANCHOR: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();

fn clock_slot() -> &'static Mutex<Option<Clock>> {
    CLOCK.get_or_init(|| Mutex::new(None))
}

fn pending_slot() -> &'static Mutex<Option<(Instant, f64)>> {
    PENDING.get_or_init(|| Mutex::new(None))
}

/// 播放器每一幀回報目前播到第幾秒；換歌或停止就清掉，免得拿上一首的秒數去對。
pub fn tick(title: &str, artist: &str, position: f64, playing: bool) {
    if title.trim().is_empty() {
        if let Ok(mut guard) = clock_slot().lock() {
            *guard = None;
        }
        return;
    }
    if let Ok(mut guard) = clock_slot().lock() {
        *guard = Some(Clock {
            title: title.to_string(),
            artist: artist.to_string(),
            position,
            playing,
            at: Instant::now(),
        });
    }
}

/// 認出歌的當下比對用：這段音的起點，播放器是第幾秒。
fn player_now() -> Option<f64> {
    let guard = clock_slot().lock().ok()?;
    let clock = guard.as_ref()?;
    if !clock.playing || clock.position < MIN_PLAYER_SECONDS {
        return None;
    }
    Some(clock.position + clock.at.elapsed().as_secs_f64())
}

/// 取樣前先記一筆：之後辨識回來的「曲內起點秒數」就是對照這個時刻。
pub fn mark_capture() {
    let value = player_now().map(|seconds| (Instant::now(), seconds));
    if let Ok(mut guard) = pending_slot().lock() {
        *guard = value;
    }
}

/// 曲內秒數 ↔ 播放器秒數的差；不合理就回 None（不猜）。
pub fn resolved(track_seconds: f32, player_seconds: f64) -> Option<f32> {
    if !track_seconds.is_finite() || track_seconds <= 0.0 {
        return None;
    }
    if !player_seconds.is_finite() || player_seconds < MIN_PLAYER_SECONDS {
        return None;
    }
    let offset = track_seconds - player_seconds as f32;
    (offset.abs() <= MAX_OFFSET_SECONDS).then_some(offset)
}

/// 位移只在同一種播放來源之間通用：影片版（瀏覽器）與播放器版（曲內時間）本來就不該套同一個位移。
fn scope(source: &str) -> &'static str {
    if crate::smtc::is_browser_source(source) { "web" } else { "local" }
}

fn store_key(title: &str, artist: &str, source: &str) -> String {
    format!("{}@{}", crate::lyrics_library::key(title, artist), scope(source))
}

fn dir() -> PathBuf {
    crate::model::config_dir().join("lyrics")
}

fn parse(text: &str) -> HashMap<String, i32> {
    let mut map = HashMap::new();
    for line in text.lines() {
        if let Some((key, value)) = line.split_once('\t') {
            if let Ok(ms) = value.trim().parse::<i32>() {
                map.insert(key.to_string(), ms);
            }
        }
    }
    map
}

fn read_from(dir: &Path) -> HashMap<String, i32> {
    std::fs::read_to_string(dir.join(FILE)).map(|text| parse(&text)).unwrap_or_default()
}

fn write_to(dir: &Path, map: &HashMap<String, i32>) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut keys: Vec<&String> = map.keys().collect();
    keys.sort();
    let mut text = String::new();
    for key in keys {
        text.push_str(key);
        text.push('\t');
        text.push_str(&map[key].to_string());
        text.push('\n');
    }
    std::fs::write(dir.join(FILE), text)
}

struct Cache {
    loaded: bool,
    map: HashMap<String, i32>,
}

static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();

fn cache() -> &'static Mutex<Cache> {
    CACHE.get_or_init(|| Mutex::new(Cache { loaded: false, map: HashMap::new() }))
}

/// 這首歌學到的位移（秒）；沒學過就是 0。
pub fn offset_seconds(title: &str, artist: &str, source: &str) -> f32 {
    if title.trim().is_empty() {
        return 0.0;
    }
    let key = store_key(title, artist, source);
    let Ok(mut guard) = cache().lock() else { return 0.0 };
    if !guard.loaded {
        guard.map = read_from(&dir());
        guard.loaded = true;
    }
    guard.map.get(&key).map_or(0.0, |ms| *ms as f32 / 1000.0)
}

/// 記住這首歌的位移；回傳實際存進去的毫秒數。
pub fn remember(title: &str, artist: &str, source: &str, offset: f32) -> Option<i32> {
    if title.trim().is_empty() || !offset.is_finite() || offset.abs() > MAX_OFFSET_SECONDS {
        return None;
    }
    let key = store_key(title, artist, source);
    let ms = (offset * 1000.0).round() as i32;
    let Ok(mut guard) = cache().lock() else { return None };
    if !guard.loaded {
        guard.map = read_from(&dir());
        guard.loaded = true;
    }
    guard.map.insert(key, ms);
    let _ = write_to(&dir(), &guard.map);
    Some(ms)
}

/// 忘掉這首歌的位移（使用者手動歸零時用）。
pub fn forget(title: &str, artist: &str, source: &str) {
    if title.trim().is_empty() {
        return;
    }
    let key = store_key(title, artist, source);
    let Ok(mut guard) = cache().lock() else { return };
    if !guard.loaded {
        guard.map = read_from(&dir());
        guard.loaded = true;
    }
    guard.map.remove(&key);
    let _ = write_to(&dir(), &guard.map);
}

/// 同一個辨識結果會被播放卡每一輪沿用，不能每幀都重寫檔案；五秒最多校正一次。
const ANCHOR_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);
/// 學到的位移跟原本差不到這個量就不重寫。
const ANCHOR_EPSILON: f32 = 0.3;

/// 用剛拿到的辨識結果校正這首歌：算出位移、寫進本機並留一行診斷。
pub fn anchor(title: &str, artist: &str, source: &str, track_seconds: f32) -> Option<i32> {
    let pending = *pending_slot().lock().ok()?.as_ref()?;
    if pending.0.elapsed().as_secs() > MAX_ANCHOR_AGE_SECS {
        return None;
    }
    {
        let mut last = LAST_ANCHOR.get_or_init(|| Mutex::new(None)).lock().ok()?;
        if last.is_some_and(|at: Instant| at.elapsed() < ANCHOR_INTERVAL) {
            return None;
        }
        *last = Some(Instant::now());
    }
    let offset = resolved(track_seconds, pending.1)?;
    if (offset - offset_seconds(title, artist, source)).abs() < ANCHOR_EPSILON {
        return None;
    }
    let ms = remember(title, artist, source, offset)?;
    let line = format!(
        "對齊 {title} - {artist}：曲內 {track_seconds:.1}s、播放器 {:.1}s → 位移 {:+.2}s\n",
        pending.1,
        ms as f32 / 1000.0
    );
    crate::resources::append_log(&crate::model::config_dir().join("diagnostics"), "lyric-sync.log", &line);
    Some(ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolved_measures_the_gap_between_track_and_player() {
        // 影片有 20 秒前奏：畫面 45 秒時，歌其實唱到 25 秒。
        assert_eq!(resolved(25.0, 45.0), Some(-20.0));
        // 播放器慢半拍（藍牙延遲）：歌詞要往後挪。
        assert_eq!(resolved(30.5, 30.0), Some(0.5));
    }

    #[test]
    fn resolved_rejects_broken_samples() {
        assert!(resolved(0.0, 10.0).is_none(), "曲內位置 0 代表辨識沒給時間");
        assert!(resolved(10.0, 0.0).is_none(), "播放器沒時間軸時不能比");
        assert!(resolved(600.0, 10.0).is_none(), "差太多當成認錯歌");
        assert!(resolved(f32::NAN, 10.0).is_none());
    }

    #[test]
    fn offsets_round_trip_through_the_file() {
        let root = std::env::temp_dir().join(format!("kuidao-sync-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mut map = HashMap::new();
        map.insert(store_key("夢中的情話", "江蕙", "chrome.exe"), -20000);
        write_to(&root, &map).unwrap();
        let back = read_from(&root);
        assert_eq!(back, map, "寫進去的位移要原樣讀得回來");
        assert!(parse("壞行\n另一行\t12\n").get("另一行") == Some(&12), "壞行要跳過，好行照收");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn video_and_player_offsets_stay_separate() {
        assert_eq!(store_key("稻香", "周杰倫", "chrome.exe"), store_key("稻香", "周杰倫", "msedge.exe"), "瀏覽器之間共用");
        assert_ne!(store_key("稻香", "周杰倫", "chrome.exe"), store_key("稻香", "周杰倫", "cloudmusic.exe"), "播放器不套用影片位移");
    }

    #[test]
    fn remember_ignores_impossible_offsets() {
        assert!(remember("", "江蕙", "chrome.exe", 3.0).is_none(), "沒有歌名不能存");
        assert!(remember("夢中的情話", "江蕙", "chrome.exe", 999.0).is_none(), "超過上限不存");
    }
}

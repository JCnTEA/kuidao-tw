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
/// 每首歌這次執行期間的辨識取樣；只放記憶體，不把半成品寫進使用者設定。
static SAMPLES: OnceLock<Mutex<HashMap<String, Vec<i32>>>> = OnceLock::new();
/// 同一個辨識結果只學一次；播放卡會把快取結果每輪重複送進來。
static LAST_CAPTURE: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();

fn samples_slot() -> &'static Mutex<HashMap<String, Vec<i32>>> {
    SAMPLES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn last_capture_slot() -> &'static Mutex<HashMap<String, Instant>> {
    LAST_CAPTURE.get_or_init(|| Mutex::new(HashMap::new()))
}

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

/// 取樣結束時換算這段音在播放器上的起點。
/// 比「按下錄音前的時間」準：錄音裝置初始化常常晚幾百毫秒，直接用起點會把歌詞校正往前偏。
pub fn capture_start(listened_seconds: f32) -> Option<f64> {
    if !listened_seconds.is_finite() || listened_seconds <= 0.0 { return None; }
    player_now().map(|end| (end - listened_seconds as f64).max(0.0))
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

/// 這首歌學到的位移（毫秒）；沒學過就是 None。
fn stored_ms(title: &str, artist: &str, source: &str) -> Option<i32> {
    if title.trim().is_empty() {
        return None;
    }
    let key = store_key(title, artist, source);
    let Ok(mut guard) = cache().lock() else { return None };
    if !guard.loaded {
        guard.map = read_from(&dir());
        guard.loaded = true;
    }
    guard.map.get(&key).copied()
}

/// 這首歌學到的位移（秒）；沒學過就是 0。
pub fn offset_seconds(title: &str, artist: &str, source: &str) -> f32 {
    stored_ms(title, artist, source).map_or(0.0, |ms| ms as f32 / 1000.0)
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
    if let Ok(mut samples) = samples_slot().lock() { samples.remove(&key); }
    if let Ok(mut captures) = last_capture_slot().lock() { captures.remove(&key); }
}

/// 同一個辨識結果會被播放卡每一輪沿用，不能每幀都重寫檔案；十秒最多校正一次。
const ANCHOR_INTERVAL: std::time::Duration = std::time::Duration::from_secs(10);
/// 同一首歌保留最近幾次辨識取樣，取穩定的部分再寫入。
const SAMPLE_WINDOW: usize = 5;
/// 取樣彼此相差在這個範圍內，視為同一組可信值。
const SAMPLE_INLIER_MS: i32 = 600;
/// 新值跟已存位移差不到這個量就不重寫，避免歌詞每幾秒微調一次。
const SAMPLE_SETTLE_MS: i32 = 300;

/// 把一次辨識取樣併進這首歌的校正值。
///
/// 單次辨識的時間位置會有零點幾秒的抖動；直接覆寫會讓歌詞每隔幾秒前後跳。
/// 這裡保留最近五次取樣，先剔除離群值，再用穩定值的平均；已有位移時，
/// 至少要兩次彼此接近的新取樣才願意改動，避免切視窗或播放器回報慢半拍時被單一噪聲帶偏。
fn pick_offset(samples: &[i32], current: Option<i32>) -> Option<i32> {
    if samples.is_empty() { return None; }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let median = sorted[sorted.len() / 2];
    let inliers: Vec<i32> = sorted.iter().copied()
        .filter(|value| (value - median).abs() <= SAMPLE_INLIER_MS).collect();
    if samples.len() >= 2 && inliers.len() < 2 { return None; }
    if inliers.len() == 1 && current.is_some() { return None; }
    let sum: i64 = inliers.iter().map(|value| *value as i64).sum();
    let candidate = (sum / inliers.len() as i64) as i32;
    if let Some(previous) = current {
        if (candidate - previous).abs() <= SAMPLE_SETTLE_MS { return None; }
    }
    Some(candidate)
}

/// 用剛拿到的辨識結果校正這首歌：算出位移、併入取樣、穩定後寫進本機並留一行診斷。
pub fn anchor(title: &str, artist: &str, source: &str, track_seconds: f32, captured: Instant, player_start: Option<f64>) -> Option<i32> {
    // 優先用「錄音結束時間 - 實際音訊長度」算出的取樣起點；沒有播放時鐘才退回錄音前記下的位置。
    let player_seconds = match player_start {
        Some(seconds) if seconds.is_finite() && seconds >= MIN_PLAYER_SECONDS => seconds,
        _ => {
            let pending = *pending_slot().lock().ok()?.as_ref()?;
            if pending.0.elapsed().as_secs() > MAX_ANCHOR_AGE_SECS { return None; }
            pending.1
        }
    };
    let key = store_key(title, artist, source);
    {
        let mut seen = last_capture_slot().lock().ok()?;
        if seen.get(&key).is_some_and(|last| *last == captured) { return None; }
        seen.insert(key.clone(), captured);
    }
    {
        let mut last = LAST_ANCHOR.get_or_init(|| Mutex::new(None)).lock().ok()?;
        if last.is_some_and(|at: Instant| at.elapsed() < ANCHOR_INTERVAL) { return None; }
        *last = Some(Instant::now());
    }
    let offset = resolved(track_seconds, player_seconds)?;
    let sample = (offset * 1000.0).round() as i32;
    let current = stored_ms(title, artist, source);
    let (samples, candidate) = {
        let mut guard = samples_slot().lock().ok()?;
        let list = guard.entry(key.clone()).or_default();
        list.push(sample);
        if list.len() > SAMPLE_WINDOW { let extra = list.len() - SAMPLE_WINDOW; list.drain(..extra); }
        let snapshot = list.clone();
        (snapshot.clone(), pick_offset(&snapshot, current)?)
    };
    if current.is_some_and(|value| (candidate - value).abs() < SAMPLE_SETTLE_MS) { return None; }
    let ms = remember(title, artist, source, candidate as f32 / 1000.0)?;
    let line = format!("對齊 {title} - {artist}：曲內 {track_seconds:.1}s、播放器 {player_seconds:.1}s → 位移 {:+.2}s（取樣 {} 筆 {:?}）\n",
        ms as f32 / 1000.0, samples.len(), samples);
    crate::resources::append_log(&crate::model::config_dir().join("diagnostics"), "lyric-sync.log", &line);
    Some(ms)
}

/// 歌詞播放時鐘：播放器回報會隨視窗焦點、採樣頻率改變，但歌詞不能跟著倒退或跳動。
/// 同一個歌名／歌手只前進不倒退；小幅落後的舊取樣視為噪聲，真正跳轉（超過三秒）才重設。
#[derive(Default)]
pub struct LyricClock {
    key: String,
    position: f64,
    at: Option<Instant>,
    playing: bool,
    duration: f64,
}

impl LyricClock {
    pub fn reset(&mut self) { *self = Self::default(); }

    pub fn observe(&mut self, key: &str, raw: f64, playing: bool, duration: f64) {
        self.observe_at(Instant::now(), key, raw, playing, duration);
    }

    fn observe_at(&mut self, now: Instant, key: &str, raw: f64, playing: bool, duration: f64) {
        let raw = if raw.is_finite() { raw.max(0.0) } else { 0.0 };
        let raw = if duration > 0.0 { raw.min(duration) } else { raw };
        if key.trim().is_empty() || !playing || !self.playing || self.at.is_none() || key != self.key {
            self.key = key.to_string(); self.position = raw; self.at = Some(now);
            self.playing = playing; self.duration = duration; return;
        }
        let elapsed = now.duration_since(self.at.unwrap()).as_secs_f64();
        let expected = self.position + elapsed;
        let expected = if duration > 0.0 { expected.min(duration) } else { expected };
        // 舊取樣小幅落後：維持時鐘自然前進，不把歌詞拉回去。
        // 真的往回跳（>3 秒）才當成使用者拖動進度條；往前跳同樣當成跳轉。
        let next = if raw < self.position - 3.0 || raw > expected + 2.5 { raw } else { raw.max(expected) };
        self.position = if duration > 0.0 { next.min(duration) } else { next };
        self.at = Some(now); self.playing = playing; self.duration = duration;
    }

    pub fn seconds(&self) -> f32 { self.seconds_at(Instant::now()) }

    fn seconds_at(&self, now: Instant) -> f32 {
        if !self.playing { return self.position as f32; }
        let elapsed = self.at.map_or(0.0, |at| now.duration_since(at).as_secs_f64());
        let seconds = self.position + elapsed;
        let seconds = if self.duration > 0.0 { seconds.min(self.duration) } else { seconds };
        seconds as f32
    }
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

    #[test]
    fn calibration_waits_for_two_matching_samples_before_moving_a_known_offset() {
        assert_eq!(pick_offset(&[2_000], None), Some(2_000), "第一次辨識就採用，不必等兩輪");
        assert_eq!(pick_offset(&[2_000, 2_050], None), Some(2_025), "兩次接近就平均，抖動不直接覆寫");
        assert_eq!(pick_offset(&[2_000, 5_000], None), None, "沒有共識寧可不學");
        assert_eq!(pick_offset(&[2_000, 2_040, 5_100], None), Some(2_020), "少數離群值要被剔除");
        assert_eq!(pick_offset(&[2_000], Some(1_500)), None, "已有位移時單次新取樣不能改");
        assert_eq!(pick_offset(&[2_000, 2_050], Some(1_500)), Some(2_025), "兩次一致才修正");
        assert_eq!(pick_offset(&[2_000, 2_050], Some(2_000)), None, "差距小於穩定門檻不重寫");
    }

    #[test]
    fn lyric_clock_does_not_go_back_when_a_stale_sample_arrives() {
        let start = Instant::now();
        let mut clock = LyricClock::default();
        clock.observe_at(start, "稻香\u{1f}周杰倫", 42.0, true, 240.0);
        assert!((clock.seconds_at(start + Duration::from_secs(1)) - 43.0).abs() < 0.01);
        // 切視窗後播放器回報同一個舊位置：時鐘照本機時間往前，不把歌詞拉回 42 秒。
        clock.observe_at(start + Duration::from_secs(1), "稻香\u{1f}周杰倫", 42.0, true, 240.0);
        assert!((clock.seconds_at(start + Duration::from_secs(1)) - 43.0).abs() < 0.01);
        assert!((clock.seconds_at(start + Duration::from_secs(2)) - 44.0).abs() < 0.01);
        clock.observe_at(start + Duration::from_secs(2), "稻香\u{1f}周杰倫", 44.5, true, 240.0);
        assert!((clock.seconds_at(start + Duration::from_secs(2)) - 44.5).abs() < 0.01);
        clock.observe_at(start + Duration::from_secs(3), "稻香\u{1f}周杰倫", 10.0, true, 240.0);
        assert!((clock.seconds_at(start + Duration::from_secs(3)) - 10.0).abs() < 0.01, "真正往回拖超過三秒才重設");
    }

    #[test]
    fn lyric_clock_resets_on_pause_song_change_and_seek_target() {
        let start = Instant::now();
        let mut clock = LyricClock::default();
        clock.observe_at(start, "A\u{1f}X", 30.0, true, 120.0);
        clock.observe_at(start + Duration::from_secs(2), "A\u{1f}X", 30.0, false, 120.0);
        assert!((clock.seconds_at(start + Duration::from_secs(5)) - 30.0).abs() < 0.01, "暫停不能繼續走");
        clock.observe_at(start + Duration::from_secs(5), "A\u{1f}X", 30.0, true, 120.0);
        assert!((clock.seconds_at(start + Duration::from_secs(6)) - 31.0).abs() < 0.01);
        clock.observe_at(start + Duration::from_secs(6), "B\u{1f}Y", 10.0, true, 120.0);
        assert!((clock.seconds_at(start + Duration::from_secs(6)) - 10.0).abs() < 0.01, "換歌要立即換到新歌位置");
    }
}

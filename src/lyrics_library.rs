//! 本機歌詞庫：每次成功抓到歌詞就存一份在使用者自己的電腦上。
//! 只以「正規化歌名＋歌手」當鍵，不含影片長度：同一首歌不管從 YouTube、
//! 播放卡還是聽歌辨識進來，下次都直接命中，不必再連網重抓。
//! 記住的是歌詞本身（含時間軸），不是播放狀態；換歌時由呼叫端自行判斷要不要用。

use crate::lyrics::{Lyrics, LyricsLine};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// 單筆上限：超過就不寫，避免把異常的大檔塞進使用者電腦。
const MAX_LINES: usize = 4000;
const MAX_TEXT: usize = 256 * 1024;

/// 資料夾位置：設定目錄下的 `lyrics/library`，跟既有的快取放在一起。
fn dir() -> PathBuf {
    crate::model::config_dir().join("lyrics").join("library")
}

/// 鍵：歌名與歌手都先做同樣的正規化（大小寫、空白、標點都不影響），長度不參與。
pub fn key(title: &str, artist: &str) -> String {
    format!("{}|{}", normalize(title), normalize(artist))
}

fn normalize(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace() && !"()[]{}（）【】「」《》<>.,，。!！?？:：;；/\\-_·|".contains(*c))
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn path_in(base: &Path, title: &str, artist: &str) -> PathBuf {
    let key = key(title, artist);
    base.join(format!("{:016x}.json", hash(&key)))
}

/// FNV-1a：同一個字串每次都得到同一個檔名，不需要另外維護索引。
fn hash(text: &str) -> u64 {
    let mut value: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        value ^= *byte as u64;
        value = value.wrapping_mul(0x1000_0000_01b3);
    }
    value
}

/// 本機有沒有這首歌的歌詞；有就直接回傳，讓「辨識完成」當下就能出詞。
pub fn read(title: &str, artist: &str) -> Option<Lyrics> {
    read_in(&dir(), title, artist)
}

fn read_in(base: &Path, title: &str, artist: &str) -> Option<Lyrics> {
    read_file(&path_in(base, title, artist))
}

fn read_file(path: &Path) -> Option<Lyrics> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.len() > MAX_TEXT as u64 {
        return None;
    }
    let raw = std::fs::read_to_string(path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let synced = json.get("synced").and_then(|v| v.as_bool()).unwrap_or(false);
    let lines = json
        .get("lines")
        .and_then(|v| v.as_array())?
        .iter()
        .filter_map(|line| {
            let item = line.as_array()?;
            let t = item.first()?.as_f64()? as f32;
            let text = item.get(1)?.as_str()?.to_owned();
            Some(LyricsLine { t, text })
        })
        .collect::<Vec<_>>();
    if lines.is_empty() || lines.len() > MAX_LINES {
        return None;
    }
    // 只有帶時間軸的版本才直接拿來播；純文字留在本機但不搶先顯示，仍讓呼叫端去補同步版。
    if !synced || !lines.iter().any(|line| !line.text.trim().is_empty()) {
        return None;
    }
    Some(Lyrics { lines, synced })
}

/// 抓到歌詞後寫進本機；純文字（沒有時間軸）也存，下次至少能先看到內容。
pub fn save(title: &str, artist: &str, source: &str, lyrics: &Lyrics) {
    if title.trim().is_empty() || artist.trim().is_empty() {
        return;
    }
    if lyrics.lines.is_empty() || lyrics.lines.len() > MAX_LINES {
        return;
    }
    write_in(&dir(), title, artist, source, lyrics);
}

fn write_in(base: &Path, title: &str, artist: &str, source: &str, lyrics: &Lyrics) {
    let lines: Vec<serde_json::Value> = lyrics
        .lines
        .iter()
        .map(|line| serde_json::json!([line.t, line.text]))
        .collect();
    let body = serde_json::json!({
        "title": title.trim(),
        "artist": artist.trim(),
        "source": source,
        "synced": lyrics.synced,
        "saved": now_secs(),
        "lines": lines,
    })
    .to_string();
    if body.len() > MAX_TEXT {
        return;
    }
    let _ = std::fs::create_dir_all(base);
    let _ = crate::model::atomic_write(&path_in(base, title, artist), body.as_bytes());
    invalidate_stats();
}

/// 本機歌詞庫的概況，設定頁用來顯示「已經存了幾首」，讓使用者知道這台電腦會自己長大。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub songs: usize,
    pub bytes: u64,
    pub last: Option<SystemTime>,
}

fn stats_cache() -> &'static std::sync::Mutex<Option<(std::time::Instant, Stats)>> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<Option<(std::time::Instant, Stats)>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(None))
}

fn invalidate_stats() {
    if let Ok(mut guard) = stats_cache().lock() {
        *guard = None;
    }
}

pub fn stats() -> Stats {
    if let Ok(guard) = stats_cache().lock() {
        if let Some((at, stats)) = guard.as_ref() {
            if at.elapsed() < Duration::from_secs(5) {
                return stats.clone();
            }
        }
    }
    let stats = stats_in(&dir());
    if let Ok(mut guard) = stats_cache().lock() {
        *guard = Some((std::time::Instant::now(), stats.clone()));
    }
    stats
}

fn stats_in(base: &Path) -> Stats {
    let mut stats = Stats::default();
    let Ok(entries) = std::fs::read_dir(base) else {
        return stats;
    };
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() || entry.path().extension().is_some_and(|ext| ext != "json") {
            continue;
        }
        stats.songs += 1;
        stats.bytes = stats.bytes.saturating_add(meta.len());
        if let Ok(modified) = meta.modified() {
            stats.last = Some(stats.last.map_or(modified, |old| old.max(modified)));
        }
    }
    stats
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "kuidao-library-{}-{}",
            std::process::id(),
            now_secs()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample() -> Lyrics {
        Lyrics {
            lines: vec![
                LyricsLine { t: 1.5, text: "第一句".into() },
                LyricsLine { t: 4.0, text: "第二句".into() },
            ],
            synced: true,
        }
    }

    #[test]
    fn library_key_ignores_duration_and_decoration() {
        assert_eq!(key("無歡 NoFun", "我們OURS"), key("無歡…", "我們ours"));
        assert_eq!(key("(Live) 晴天", "周杰倫"), key("晴天", "周杰倫"));
    }

    #[test]
    fn saved_lyrics_come_back_from_the_machine() {
        let dir = temp_dir();
        write_in(&dir, "無歡", "陳華", "lrclib", &sample());
        let hit = read_in(&dir, "無歡", "陳華").expect("抓到過就該在本機");
        assert!(hit.synced);
        assert_eq!(hit.lines.len(), 2);
        assert_eq!(hit.lines[1].text, "第二句");
        assert_eq!(read_in(&dir, "另一首", "別人"), None);
        let stats = stats_in(&dir);
        assert_eq!(stats.songs, 1);
        assert!(stats.bytes > 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn plain_lyrics_are_not_served_as_synced() {
        let dir = temp_dir();
        let plain = Lyrics {
            lines: vec![LyricsLine { t: 0.0, text: "純文字".into() }],
            synced: false,
        };
        write_in(&dir, "歌", "手", "netease", &plain);
        assert!(read_in(&dir, "歌", "手").is_none(), "沒有時間軸的舊資料不當同步歌詞用");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

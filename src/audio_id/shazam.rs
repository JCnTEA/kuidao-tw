//! 最小可用的 Shazam 辨識客戶端：把簽章打包成官方 App 使用的 JSON，換回歌名、歌手與時間位置。
//!
//! 請求格式參考 MIT 授權的 `middledot/shazam.py`，只保留辨識所需的欄位。

use serde_json::{json, Value};

const HOST: &str = "https://amp.shazam.com";

/// 辨識結果。
#[derive(Clone, Debug, PartialEq)]
pub struct Match {
    /// 歌名。
    pub title: String,
    /// 歌手；Shazam 有時把「歌手 · 專輯」放在這裡。
    pub artist: String,
    /// 這次取樣在歌曲裡的起點（秒）。用它可以推回「現在唱到哪裡」，讓歌詞從正確的段落進場。
    pub offset_seconds: f32,
}

/// 產生兩個大寫 UUID，Shazam 的網址需要它們。
fn uuid() -> String {
    let mut bytes = [0u8; 16];
    let rng = ring::rand::SystemRandom::new();
    let _ = ring::rand::SecureRandom::fill(&rng, &mut bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join("");
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// 送出簽章並解析回傳結果。
///
/// 回傳 `Ok(None)` 代表連線正常但認不出歌（安靜、人聲、冷門曲目）；
/// `Err` 帶回短原因，設定頁與診斷檔要看得到是「連不上」還是「格式不對」，
/// 不然使用者只會覺得「聽歌辨識沒反應」。
pub fn identify(signature: &super::fingerprint::Signature) -> Result<Option<Match>, String> {
    let bytes = super::fingerprint::encode(signature);
    let uri = format!(
        "data:audio/vnd.shazam.sig;base64,{}",
        base64_encode(&bytes)
    );
    let sample_ms = (signature.number_samples as f64 / signature.sample_rate_hz as f64 * 1000.0) as u64;
    let body = json!({
        "timezone": "Asia/Taipei",
        "signature": { "uri": uri, "samplems": sample_ms },
        "timestamp": now_ms(),
        "context": {},
        "geolocation": {},
    });

    let url = tag_url("zh/TW", &uuid(), &uuid());
    let payload = serde_json::to_vec(&body).map_err(|error| format!("打包失敗:{error}"))?;
    let agent = agent().ok_or_else(|| "代理設定無效".to_string())?;
    // 「zh/ZH」這種語系不是合法地區，Shazam 直接回 400；用 zh/TW，失敗再退 en/US。
    let mut text = match post(&agent, &url, &payload) {
        Ok(text) => text,
        Err(Failure::Status(code, body)) if code == 400 => {
            let fallback = tag_url("en/US", &uuid(), &uuid());
            match post(&agent, &fallback, &payload) {
                Ok(text) => text,
                Err(Failure::Status(code, body)) => return Err(format!("HTTP {code}{body}")),
                Err(Failure::Other(reason)) => return Err(reason),
            }
        }
        Err(Failure::Status(code, body)) => return Err(format!("HTTP {code}{body}")),
        Err(Failure::Other(reason)) => return Err(reason),
    };
    // 回應可能是壓縮或空字串；認不出歌時 body 會是空的 matches 陣列。
    if text.trim().is_empty() {
        return Err("回應為空".to_string());
    }
    text.shrink_to_fit();
    Ok(parse(&text))
}

/// 組出辨識用的網址；`locale` 必須是「語言/地區」且地區存在（zh/ZH 會 400）。
fn tag_url(locale: &str, a: &str, b: &str) -> String {
    format!(
        "{HOST}/discovery/v5/{locale}/iphone/-/tag/{a}/{b}?sync=true&webv3=true&sampling=true&connected=&shazamapiversion=v3&sharehub=true&hubv5minorversion=v5.1&hidelb=true&video=v3"
    )
}

enum Failure {
    /// 伺服器回的狀態碼，附上縮短的錯誤內容，診斷檔才看得出原因。
    Status(u16, String),
    Other(String),
}

fn post(agent: &ureq::Agent, url: &str, payload: &[u8]) -> Result<String, Failure> {
    let response = agent
        .post(url)
        .set("X-Shazam-Platform", "IPHONE")
        .set("X-Shazam-AppVersion", "14.1.0")
        .set("Accept", "*/*")
        .set("Accept-Language", "zh")
        .set("User-Agent", "Shazam/3685 CFNetwork/1197 Darwin/20.0.0")
        .set("Content-Type", "application/json")
        .send_bytes(payload);
    match response {
        Ok(response) => response.into_string().map_err(|error| Failure::Other(format!("讀取回應失敗:{error}"))),
        Err(ureq::Error::Status(code, response)) => {
            let body = response.into_string().unwrap_or_default();
            let excerpt = body.replace('\n', " ").trim().chars().take(120).collect::<String>();
            Err(Failure::Status(code, if excerpt.is_empty() { String::new() } else { format!(":{excerpt}") }))
        }
        Err(error) => Err(Failure::Other(format!("連線失敗:{error}"))),
    }
}

/// 解析 Shazam 回應；沒有 track 時視為未辨識。
pub fn parse(text: &str) -> Option<Match> {
    let value: Value = serde_json::from_str(text).ok()?;
    let track = value.get("track")?;
    let title = track.get("title")?.as_str()?.trim().to_string();
    if title.is_empty() {
        return None;
    }
    let subtitle = track
        .get("subtitle")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let offset = value
        .get("matches")
        .and_then(Value::as_array)
        .and_then(|matches| matches.first())
        .and_then(|item| item.get("offset"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0) as f32;
    // 「歌手 · 專輯」這種寫法只留歌手，比對歌詞時才不會把專輯名當人。
    let artist = subtitle.split('·').next().unwrap_or("").trim().to_string();
    Some(Match { title, artist, offset_seconds: offset })
}

/// 尊重系統代理設定，企業網路或代理環境下才連得到 Shazam。
fn agent() -> Option<ureq::Agent> {
    let proxy = ["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy", "HTTP_PROXY", "http_proxy"]
        .iter()
        .find_map(|key| std::env::var(key).ok().filter(|value| !value.trim().is_empty()));
    let mut builder = ureq::AgentBuilder::new()
        .try_proxy_from_env(false)
        .timeout_connect(std::time::Duration::from_secs(10))
        .timeout_read(std::time::Duration::from_secs(20))
        .timeout_write(std::time::Duration::from_secs(20));
    if let Some(proxy) = proxy {
        builder = builder.proxy(ureq::Proxy::new(proxy).ok()?);
    }
    Some(builder.build())
}

fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_track_and_offset() {
        let body = r#"{"matches":[{"id":"1","offset":42.5,"timeskew":0.1}],
            "track":{"title":" 稻香 ","subtitle":"周杰倫 · 魔傑座","url":"https://x"}}"#;
        let found = parse(body).unwrap();
        assert_eq!(found.title, "稻香");
        assert_eq!(found.artist, "周杰倫");
        assert!((found.offset_seconds - 42.5).abs() < 0.01);
    }

    #[test]
    fn no_track_means_no_match() {
        assert!(parse(r#"{"matches":[]}"#).is_none());
        assert!(parse("not json").is_none());
    }

    #[test]
    fn tag_url_uses_a_real_locale() {
        let url = tag_url("zh/TW", "A", "B");
        assert!(url.contains("/discovery/v5/zh/TW/iphone/-/tag/A/B"));
        assert!(url.contains("sync=true"));
        // zh/ZH 不是合法地區，Shazam 會回 400；任何改動都不能再寫回這個值。
        assert!(!tag_url("zh/TW", "A", "B").contains("zh/ZH"));
    }

    #[test]
    fn uuid_is_uppercase_v4() {
        let id = uuid();
        assert_eq!(id.len(), 36);
        assert_eq!(id.chars().filter(|c| *c == '-').count(), 4);
        assert!(id.split('-').nth(2).unwrap().starts_with('4'));
        assert!(id.chars().all(|c| c == '-' || c.is_ascii_uppercase() || c.is_ascii_digit()));
    }
}

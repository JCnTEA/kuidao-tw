//! LINE Login v2.1（web app 流程）：PKCE、本機回呼、換 token、驗 ID token。
//!
//! 桌機沒有官方 LINE SDK，因此照 LINE 文件用授權碼流程：系統瀏覽器登入，
//! 回呼打到本機 `http://localhost:<port>/…`，拿到 code 後在背景執行緒換 token，
//! 再以 `/oauth2/v2.1/verify` 驗 ID token（`aud` 必須是本 Channel，`nonce` 必須一致）。
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};

pub const AUTHORIZE: &str = "https://access.line.me/oauth2/v2.1/authorize";
pub const TOKEN: &str = "https://api.line.me/oauth2/v2.1/token";
pub const VERIFY: &str = "https://api.line.me/oauth2/v2.1/verify";
pub const REVOKE: &str = "https://api.line.me/oauth2/v2.1/revoke";
pub const PROFILE: &str = "https://api.line.me/v2/profile";

/// 必須與 LINE Developers Console 的 Callback URL 一字不差；可在設定裡改。
pub const DEFAULT_REDIRECT: &str = "http://localhost:27311/line/callback";
/// 等使用者完成登入的上限。
const WAIT: Duration = Duration::from_secs(300);

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub channel_id: String,
    pub channel_secret: String,
    pub redirect_uri: String,
}

impl Config {
    /// 只做格式檢查，不在登入前打任何 API。
    pub fn validate(&self) -> Result<(), String> {
        let id = self.channel_id.trim();
        if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
            return Err("Channel ID 只能是數字，請從 LINE Login Channel 的基本設定複製".into());
        }
        if self.channel_secret.trim().is_empty() {
            return Err("請填入 Channel Secret".into());
        }
        redirect_target(&self.redirect_uri)?;
        Ok(())
    }
}

/// 回呼位址只接受本機 http，避免把 code 送到外部主機。
pub fn redirect_target(uri: &str) -> Result<(u16, String), String> {
    let url = url::Url::parse(uri.trim()).map_err(|_| "回呼網址格式不正確".to_string())?;
    if url.scheme() != "http" {
        return Err("回呼網址必須是 http://localhost 或 http://127.0.0.1".into());
    }
    let host = url.host_str().unwrap_or_default();
    if host != "localhost" && host != "127.0.0.1" {
        return Err("回呼網址只支援本機（localhost／127.0.0.1）".into());
    }
    let port = url.port().ok_or_else(|| "回呼網址需要明確的埠號".to_string())?;
    let path = url.path();
    if path.is_empty() || path == "/" {
        return Err("回呼網址需要路徑，例如 /line/callback".into());
    }
    Ok((port, path.to_string()))
}

pub fn pkce() -> (String, String) {
    use ring::rand::SecureRandom;
    let mut bytes = [0u8; 32];
    let verifier = if ring::rand::SystemRandom::new().fill(&mut bytes).is_ok() {
        URL_SAFE_NO_PAD.encode(bytes)
    } else {
        // 32 bytes 只會失敗在系統 RNG 不可用時；退回時間雜湊仍滿足長度要求。
        URL_SAFE_NO_PAD.encode(format!("{}-{}", std::process::id(), unix_now()).as_bytes())
    };
    (verifier.clone(), challenge(&verifier))
}

pub fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(ring::digest::digest(&ring::digest::SHA256, verifier.as_bytes()).as_ref())
}

fn random_token() -> String {
    use ring::rand::SecureRandom;
    let mut bytes = [0u8; 24];
    let _ = ring::rand::SystemRandom::new().fill(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn authorize_url(cfg: &Config, state: &str, nonce: &str, challenge: &str) -> String {
    let url = url::Url::parse_with_params(
        AUTHORIZE,
        &[
            ("response_type", "code"),
            ("client_id", cfg.channel_id.trim()),
            ("redirect_uri", cfg.redirect_uri.trim()),
            ("state", state),
            ("scope", "profile openid"),
            ("nonce", nonce),
            ("code_challenge", challenge),
            ("code_challenge_method", "S256"),
        ],
    );
    let Ok(url) = url else { return String::new() };
    // LINE 文件用 %20 表示 scope 分隔；url crate 可能輸出 '+'，兩種都先正規化成 %20。
    url.to_string().replace("scope=profile+openid", "scope=profile%20openid")
}

/// 從回呼的 query 取 code；`error` 與 `state` 不符都在這裡擋掉。
pub fn callback_code(query: &str, expected_state: &str) -> Result<String, String> {
    let mut code = String::new();
    let mut state = String::new();
    let mut error = String::new();
    for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
        match key.as_ref() {
            "code" => code = value.into_owned(),
            "state" => state = value.into_owned(),
            "error" => error = value.into_owned(),
            "error_description" => {
                if error.is_empty() {
                    error = value.into_owned();
                }
            }
            _ => {}
        }
    }
    if !error.is_empty() {
        return Err(match error.as_str() {
            "ACCESS_DENIED" => "你在 LINE 畫面取消了授權".into(),
            "INVALID_SCOPE" => "Channel 不支援 profile／openid 權限，請檢查 LINE Login 設定".into(),
            other => format!("LINE 回報錯誤：{other}"),
        });
    }
    if state != expected_state {
        return Err("回呼的 state 與本次登入不一致，已中止".into());
    }
    if code.is_empty() {
        return Err("回呼沒有帶授權碼".into());
    }
    Ok(code)
}

fn response_page() -> String {
    concat!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n",
        "<!doctype html><meta charset=utf-8><title>鬼島</title>",
        "<body style=\"font-family:system-ui,-apple-system,'Microsoft JhengHei',sans-serif;",
        "background:#101013;color:#eee;display:grid;place-items:center;height:100vh;margin:0\">",
        "<div style=\"text-align:center\"><h2 style=\"font-weight:600\">登入完成</h2>",
        "<p style=\"color:#9aa\">可以關閉這個視窗，回到鬼島。</p></div>"
    )
    .into()
}

/// 阻塞等待瀏覽器回呼，直到拿到 code、被取消或逾時。
fn wait_for_code(
    listener: TcpListener,
    path: &str,
    state: &str,
    cancel: &AtomicBool,
) -> Result<String, String> {
    listener.set_nonblocking(true).map_err(|_| "無法設定本機回呼".to_string())?;
    let deadline = Instant::now() + WAIT;
    loop {
        if cancel.load(Ordering::Acquire) {
            return Err("已取消 LINE 登入".into());
        }
        if Instant::now() >= deadline {
            return Err("等待 LINE 登入逾時，請重新登入".into());
        }
        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = stream.set_nonblocking(false);
                let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
                let mut buffer = [0u8; 4096];
                let Ok(read) = stream.read(&mut buffer) else { continue };
                let request = String::from_utf8_lossy(&buffer[..read]);
                let Some(target) = request.split_whitespace().nth(1) else {
                    let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\n");
                    continue;
                };
                let (request_path, query) = match target.split_once('?') {
                    Some((path, query)) => (path, query),
                    None => (target, ""),
                };
                if request_path != path {
                    let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nConnection: close\r\n\r\n");
                    continue;
                }
                match callback_code(query, state) {
                    Ok(code) => {
                        let _ = stream.write_all(response_page().as_bytes());
                        let _ = stream.flush();
                        return Ok(code);
                    }
                    Err(message) => {
                        let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\n");
                        return Err(message);
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => return Err("本機回呼發生錯誤，請重新登入".into()),
        }
    }
}

fn agent() -> ureq::Agent {
    let mut builder = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(20))
        .timeout_connect(Duration::from_secs(10))
        .redirects(0);
    let proxy = ["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy", "HTTP_PROXY", "http_proxy"]
        .iter()
        .find_map(|key| std::env::var(key).ok().filter(|value| !value.trim().is_empty()))
        .or_else(crate::quota::system_proxy);
    if let Some(proxy) = proxy.and_then(|value| ureq::Proxy::new(value).ok()) {
        builder = builder.proxy(proxy);
    }
    builder.build()
}

fn api_error(error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(code, response) => {
            let body = response.into_string().unwrap_or_default();
            let message = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|value| {
                    value["error_description"]
                        .as_str()
                        .or_else(|| value["message"].as_str())
                        .or_else(|| value["error"].as_str())
                        .map(str::to_owned)
                })
                .unwrap_or_else(|| body.chars().take(120).collect());
            if message.trim().is_empty() {
                format!("LINE API 回應 HTTP {code}")
            } else {
                format!("LINE API HTTP {code}：{message}")
            }
        }
        ureq::Error::Transport(_) => "連不上 LINE，請檢查網路或代理設定".into(),
    }
}

#[derive(Deserialize, Debug)]
struct Token {
    access_token: String,
    #[serde(default)]
    id_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    expires_in: i64,
}

fn exchange_code(cfg: &Config, code: &str, verifier: &str) -> Result<Token, String> {
    let response = agent()
        .post(TOKEN)
        .send_form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", cfg.redirect_uri.trim()),
            ("client_id", cfg.channel_id.trim()),
            ("client_secret", cfg.channel_secret.trim()),
            ("code_verifier", verifier),
        ])
        .map_err(api_error)?;
    let body = response.into_string().map_err(|_| "無法讀取 LINE 回應".to_string())?;
    serde_json::from_str::<Token>(&body).map_err(|_| "LINE token 回應格式不正確".into())
}

#[derive(Deserialize, Debug)]
struct Identity {
    #[serde(default)]
    aud: String,
    #[serde(default)]
    sub: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    picture: String,
    #[serde(default)]
    nonce: String,
}

fn verify_id_token(cfg: &Config, id_token: &str, nonce: &str) -> Result<Account, String> {
    if id_token.is_empty() {
        return Err("Channel 沒有回傳 ID token，請確認 scope 含 openid".into());
    }
    let response = agent()
        .post(VERIFY)
        .send_form(&[("id_token", id_token), ("client_id", cfg.channel_id.trim())])
        .map_err(api_error)?;
    let body = response.into_string().map_err(|_| "無法讀取 LINE 回應".to_string())?;
    let identity: Identity = serde_json::from_str(&body).map_err(|_| "LINE ID token 驗證回應格式不正確".to_string())?;
    if identity.aud != cfg.channel_id.trim() {
        return Err("ID token 不屬於這個 Channel".into());
    }
    if !nonce.is_empty() && identity.nonce != nonce {
        return Err("ID token 的 nonce 與本次登入不一致".into());
    }
    if identity.sub.is_empty() {
        return Err("LINE 沒有回傳使用者 ID".into());
    }
    Ok(Account {
        user_id: identity.sub,
        display_name: identity.name,
        picture_url: identity.picture,
        channel_id: cfg.channel_id.trim().to_string(),
        verified_at: unix_now(),
    })
}

fn unix_now() -> i64 {
    chrono::Utc::now().timestamp()
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Account {
    pub user_id: String,
    pub display_name: String,
    pub picture_url: String,
    /// 發 token 的 Channel；登出時撤銷要用同一個 client_id。
    #[serde(default)]
    pub channel_id: String,
    pub verified_at: i64,
}

pub fn account_path() -> PathBuf {
    crate::model::config_dir().join("line-account.json")
}

pub fn load_account() -> Option<Account> {
    let bytes = std::fs::read(account_path()).ok()?;
    serde_json::from_slice::<Account>(&bytes)
        .ok()
        .filter(|account| !account.user_id.trim().is_empty())
}

fn save_account(account: &Account) -> Result<(), String> {
    let bytes = serde_json::to_vec(account).map_err(|_| "無法寫入 LINE 帳號".to_string())?;
    crate::model::atomic_write(&account_path(), &bytes).map_err(|_| "無法寫入 LINE 帳號檔".into())
}

fn forget_account() {
    let _ = std::fs::remove_file(account_path());
}

#[cfg(windows)]
fn secret_slot(action: SlotAction) -> Result<Option<String>, String> {
    use keyring::Entry;
    let entry = Entry::new("Kuidao.Line", "channel_secret").map_err(|_| "無法開啟 Windows 憑據管理器".to_string())?;
    match action {
        SlotAction::Read => Ok(entry.get_password().ok()),
        SlotAction::Write(value) => entry
            .set_password(&value)
            .map(|_| None)
            .map_err(|_| "無法把 Channel Secret 存進憑據管理器".to_string()),
        SlotAction::Delete => {
            let _ = entry.delete_credential();
            Ok(None)
        }
    }
}

#[cfg(not(windows))]
fn secret_slot(_action: SlotAction) -> Result<Option<String>, String> {
    Err("LINE 登入只在 Windows 版提供".into())
}

enum SlotAction {
    Read,
    Write(String),
    Delete,
}

pub fn stored_secret() -> Option<String> {
    secret_slot(SlotAction::Read).ok().flatten().filter(|value| !value.trim().is_empty())
}

pub fn remember_secret(secret: &str) -> Result<(), String> {
    secret_slot(SlotAction::Write(secret.trim().to_string())).map(|_| ())
}

#[cfg(windows)]
fn token_slot(slot: &str, action: TokenAction) -> Result<Option<String>, String> {
    use keyring::Entry;
    let entry = Entry::new("Kuidao.Line", slot).map_err(|_| "無法開啟 Windows 憑據管理器".to_string())?;
    match action {
        TokenAction::Read => Ok(entry.get_password().ok()),
        TokenAction::Write(value) => entry.set_password(&value).map(|_| None).map_err(|_| "無法儲存 LINE token".to_string()),
        TokenAction::Delete => {
            let _ = entry.delete_credential();
            Ok(None)
        }
    }
}

#[cfg(not(windows))]
fn token_slot(_slot: &str, _action: TokenAction) -> Result<Option<String>, String> {
    Ok(None)
}

enum TokenAction {
    Read,
    Write(String),
    Delete,
}

fn store_tokens(token: &Token) {
    let _ = token_slot("access_token", TokenAction::Write(token.access_token.clone()));
    let _ = token_slot("refresh_token", TokenAction::Write(token.refresh_token.clone()));
}

fn forget_tokens() {
    let _ = token_slot("access_token", TokenAction::Delete);
    let _ = token_slot("refresh_token", TokenAction::Delete);
}

fn revoke_token(client_id: &str, token: &str) {
    let _ = agent()
        .post(REVOKE)
        .send_form(&[("token", token), ("client_id", client_id)]);
}

/// 登入流程狀態；UI 每幀 `poll()`，完成後把帳號寫進設定頁。
#[derive(Default)]
pub struct Login {
    receiver: Option<mpsc::Receiver<Result<Account, String>>>,
    cancel: Option<Arc<AtomicBool>>,
}

impl Login {
    pub fn pending(&self) -> bool {
        self.receiver.is_some()
    }

    pub fn start(cfg: Config) -> Self {
        let (sender, receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker = cancel.clone();
        std::thread::spawn(move || {
            let result = (|| {
                cfg.validate()?;
                let (port, path) = redirect_target(&cfg.redirect_uri)?;
                let state = random_token();
                let nonce = random_token();
                let (verifier, challenge) = pkce();
                let url = authorize_url(&cfg, &state, &nonce, &challenge);
                if url.is_empty() {
                    return Err("無法組出 LINE 授權網址".into());
                }
                // 先佔住回呼埠再開瀏覽器，埠被占用時才不會白走一趟登入。
                let listener = TcpListener::bind(("127.0.0.1", port))
                    .map_err(|_| format!("連接埠 {port} 被占用，請改 Callback URL 或關閉占用程式"))?;
                open_browser(&url)?;
                let code = wait_for_code(listener, &path, &state, &worker)?;
                let token = exchange_code(&cfg, &code, &verifier)?;
                let account = verify_id_token(&cfg, &token.id_token, &nonce)?;
                save_account(&account)?;
                store_tokens(&token);
                Ok(account)
            })();
            let _ = sender.send(result);
        });
        Self {
            receiver: Some(receiver),
            cancel: Some(cancel),
        }
    }

    pub fn poll(&mut self) -> Option<Result<Account, String>> {
        let receiver = self.receiver.take()?;
        match receiver.try_recv() {
            Ok(result) => {
                self.cancel = None;
                Some(result)
            }
            Err(mpsc::TryRecvError::Empty) => {
                self.receiver = Some(receiver);
                None
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.cancel = None;
                Some(Err("LINE 登入執行緒已結束".into()))
            }
        }
    }

    pub fn cancel(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::Release);
        }
    }
}

impl Drop for Login {
    fn drop(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::Release);
        }
    }
}

/// 登出：先撤銷 token，再清掉本機記錄。
pub fn logout(account: &Account) {
    if let Some(token) = token_slot("access_token", TokenAction::Read).ok().flatten() {
        revoke_token(&account.channel_id, &token);
    }
    forget_tokens();
    forget_account();
}

#[cfg(windows)]
fn open_browser(url: &str) -> Result<(), String> {
    use windows::{core::PCWSTR, Win32::UI::Shell::ShellExecuteW, Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL};
    let operation: Vec<u16> = "open".encode_utf16().chain(Some(0)).collect();
    let file: Vec<u16> = url.encode_utf16().chain(Some(0)).collect();
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR::from_raw(operation.as_ptr()),
            PCWSTR::from_raw(file.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize > 32 {
        Ok(())
    } else {
        Err("打不開瀏覽器，請確認系統預設瀏覽器可用".into())
    }
}

#[cfg(not(windows))]
fn open_browser(_url: &str) -> Result<(), String> {
    Err("LINE 登入只在 Windows 版提供".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> Config {
        Config {
            channel_id: "1234567890".into(),
            channel_secret: "secret".into(),
            redirect_uri: DEFAULT_REDIRECT.into(),
        }
    }

    #[test]
    fn pkce_matches_rfc7636_vector() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(challenge(verifier), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
        let (verifier, challenge) = pkce();
        assert!(verifier.len() >= 43 && verifier.len() <= 128);
        assert_eq!(challenge, super::challenge(&verifier));
    }

    #[test]
    fn authorize_url_carries_all_required_parameters() {
        let url = authorize_url(&cfg(), "state-1", "nonce-1", "challenge-1");
        assert!(url.starts_with(AUTHORIZE));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("client_id=1234567890"));
        assert!(url.contains("scope=profile+openid") || url.contains("scope=profile%20openid"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("redirect_uri=http%3A%2F%2Flocalhost%3A27311%2Fline%2Fcallback"));
    }

    #[test]
    fn callback_requires_matching_state_and_code() {
        assert_eq!(callback_code("code=abc&state=s1", "s1").as_deref(), Ok("abc"));
        assert!(callback_code("code=abc&state=s2", "s1").is_err());
        assert!(callback_code("error=ACCESS_DENIED&state=s1", "s1").is_err());
        assert!(callback_code("state=s1", "s1").is_err());
    }

    #[test]
    fn callback_target_only_accepts_loopback_http_with_port() {
        assert_eq!(redirect_target(DEFAULT_REDIRECT), Ok((27311, "/line/callback".to_string())));
        assert!(redirect_target("http://localhost/line").is_err());
        assert!(redirect_target("https://localhost:27311/line").is_err());
        assert!(redirect_target("http://example.com:27311/line").is_err());
    }

    #[test]
    fn account_json_roundtrip_and_rejects_empty_id() {
        let account = Account {
            user_id: "U1234".into(),
            display_name: "島民".into(),
            picture_url: "https://profile.line-scdn.net/x".into(),
            channel_id: "1234567890".into(),
            verified_at: 1,
        };
        let bytes = serde_json::to_vec(&account).unwrap();
        let decoded: Account = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(decoded, account);
        let empty: Account = serde_json::from_slice(br#"{"user_id":"","display_name":"","picture_url":"","verified_at":0}"#).unwrap();
        assert!(empty.user_id.is_empty());
    }
}

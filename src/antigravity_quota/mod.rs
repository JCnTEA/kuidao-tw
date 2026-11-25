//! 通過已連接的官方 CLI 獲取額度，複用島仔的 AI 額度面板。
use std::path::{Path, PathBuf};

#[cfg(windows)]
mod cli;
#[cfg(any(windows, test))]
mod parse;

pub(crate) const INSTALL_URL: &str = "https://antigravity.google/docs/getting-started?tab=cli";
pub(crate) const CLI_MISSING: &str = "請先安裝 Antigravity CLI";
pub(crate) const CLI_OLD: &str = "請更新 Antigravity CLI 後再連接";
pub(crate) const CLI_UNTRUSTED: &str = "無法驗證 Antigravity CLI 的 Google 簽名，請從官網重新安裝";
pub(crate) const LOGIN_REQUIRED: &str = "請在 Antigravity CLI 中登錄";
pub(crate) const SLASH_DISABLED: &str = "請在 Antigravity CLI 啟用斜槓命令後刷新";

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Mode {
    Workspace,
    WorkspaceDenied,
    Enterprise,
    Cloud,
    ApiKey,
}

impl Mode {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Workspace => "Workspace 暫無可讀取額度",
            Self::WorkspaceDenied => "組織未開放訪問",
            Self::Enterprise => "企業共享額度",
            Self::Cloud => "Google Cloud 按量計費",
            Self::ApiKey => "API Key 模式",
        }
    }

    pub(crate) fn url(self) -> Option<&'static str> {
        match self {
            Self::Workspace | Self::WorkspaceDenied => {
                Some("https://antigravity.google/docs/workspace")
            }
            Self::Enterprise => Some("https://console.cloud.google.com/gemini-enterprise"),
            Self::Cloud => Some("https://console.cloud.google.com/billing"),
            Self::ApiKey => Some("https://aistudio.google.com/usage"),
        }
    }
}

#[cfg(windows)]
fn lock_official_cli(exe: &Path) -> Option<std::fs::File> {
    crate::messenger::trust::lock_signed_executable(exe, |publisher| {
        publisher.eq_ignore_ascii_case("Google LLC")
    })
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct UnavailableWindow {
    pub label: String,
    pub reset: Option<i64>,
    pub disabled: bool,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Details {
    pub credits: Option<f64>,
    #[serde(default)]
    pub credits_at: Option<i64>,
    pub unavailable: Vec<UnavailableWindow>,
    pub mode: Option<Mode>,
}

#[cfg(windows)]
enum Failure {
    Message(String),
    Login,
    UnsupportedMode(Mode),
    Temporary(String),
}

pub(super) fn discover() -> Option<PathBuf> {
    #[cfg(windows)]
    return cli::discover();
    #[cfg(not(windows))]
    None
}

pub(super) fn fetch(source: &Path) -> Result<super::Snapshot, (String, u64)> {
    #[cfg(windows)]
    {
        let exe = discover().ok_or((CLI_MISSING.into(), 0))?;
        if source != exe {
            return Err(("Antigravity CLI 位置已變化，請重新連接".into(), 0));
        }
        let exe = exe.canonicalize().map_err(|_| (CLI_MISSING.into(), 0))?;
        let _verified_file = lock_official_cli(&exe).ok_or((CLI_UNTRUSTED.into(), 0))?;
        let mut snapshot = super::Snapshot {
            plan: "當前 CLI 帳號".into(),
            ..Default::default()
        };
        let usage_report = cli::report(&exe, false);
        snapshot.at = chrono::Utc::now().timestamp();
        let details = match usage_report {
            Ok(json) => {
                let usage = parse::usage(&json).map_err(|error| (error, 60))?;
                snapshot.windows = usage.windows;
                // 兩個報告都不含帳號 ID，單獨記錄積分的查詢時間，不將其視為同一時刻的讀數。
                let credits = match cli::report(&exe, true) {
                    Ok(json) => parse::credits(&json),
                    Err(Failure::Login | Failure::UnsupportedMode(_)) => {
                        return Err(("CLI 登錄狀態已變化，請確認當前帳號後刷新".into(), 0))
                    }
                    Err(_) => None,
                };
                Details {
                    credits,
                    credits_at: credits.map(|_| chrono::Utc::now().timestamp()),
                    unavailable: usage.unavailable,
                    mode: None,
                }
            }
            Err(Failure::UnsupportedMode(mode)) => Details {
                mode: Some(mode),
                ..Default::default()
            },
            Err(Failure::Login) => return Err((LOGIN_REQUIRED.into(), 120)),
            Err(Failure::Message(error)) => return Err((error, 0)),
            Err(Failure::Temporary(error)) => return Err((error, 60)),
        };
        snapshot.antigravity = Some(details);
        Ok(snapshot)
    }
    #[cfg(not(windows))]
    {
        let _ = source;
        Err((CLI_MISSING.into(), 0))
    }
}

//! 本地開發包的安裝、狀態及卡片執行。每次操作原子保存一個插件的快照。
#[path = "plugin_interactions.rs"]
mod interactions;
#[path = "appearance_assets.rs"]
pub(crate) mod appearance_assets;
#[path = "canvas_images.rs"]
mod canvas_images;
#[path = "capabilities.rs"]
mod capabilities;
#[path = "font_assets.rs"]
mod font_assets;
#[path = "plugin_devtools.rs"]
mod devtools;
#[path = "navigation_icon.rs"]
mod navigation_icon;
pub(crate) use navigation_icon::NavigationIcon;
#[path = "pinned.rs"]
mod pinned;
pub(crate) use pinned::PinnedDisplay;
#[path = "cover.rs"]
mod cover;
#[cfg(windows)]
#[path = "market.rs"]
pub(super) mod market;
#[cfg(windows)]
#[path = "market_metrics.rs"]
mod market_metrics;
#[cfg(windows)]
#[path = "market_state.rs"]
mod market_state;
#[cfg(windows)]
#[path = "source_https.rs"]
mod source_https;
#[path = "source_read.rs"]
mod source_read;
#[path = "script_settings.rs"]
mod script_settings;
#[path = "package_data.rs"]
mod package_data;
#[cfg(windows)]
#[path = "plugin_storage_backup.rs"]
mod storage_backup;
#[path = "local_ui.rs"]
mod ui;
pub(crate) use ui::{plugin_tag, SettingsNavigation};
use super::{fail, Result};
use crate::plugin_status::{PluginStatus, StatusHealth, StatusLayout};
use crate::plugin_canvas::Canvas;
use crate::quota_plugin::PluginQuota;
use capabilities::Surface;
use cover::{CoverPng, COVER_FILE};
use interactions::{Call, EventName, InteractionState, NoticeHistory, ScriptNotice};
use script_settings::{valid_field, valid_fields, Choice};
use package_data::PackageData;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use source_read::{SourceGrant, SourceKind};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    sync::{Arc, OnceLock, atomic::{AtomicBool, Ordering}},
    time::{Duration, Instant},
};

const MATERIAL_SOURCE_LIMIT: u64 = 64 * 1024;
const PAGE_SHORTCUT_SOURCE_LIMIT: u64 = 4 * 1024;
const PAGE_SHORTCUT_ENTRY: &str = "shortcuts.json";
const SCRIPT_SOURCE_LIMIT: u64 = 256 * 1024;
const BACKDROP_PERMISSION: &str = "screen.backdrop";
const SOURCE_PERMISSION: &str = "source.read";
const API_VERSION: u32 = 1;
const WIRELESS_PLUGIN_ID: &str = "daozai.wireless-receivers";
#[cfg(windows)]
const WIRELESS_FEEDBACK_URL: &str = "https://github.com/Renly1994/xiaotang-daozai/issues/267";

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Field {
    key: String,
    label: String,
    #[serde(rename = "type")]
    kind: String,
    default: String,
    #[serde(default, deserialize_with = "present_value", skip_serializing_if = "Option::is_none")]
    multiline: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    min: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    max: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    step: Option<f32>,
    #[serde(
        default,
        deserialize_with = "present_value",
        skip_serializing_if = "Option::is_none"
    )]
    unit: Option<String>,
    #[serde(
        default,
        deserialize_with = "present_value",
        skip_serializing_if = "Option::is_none"
    )]
    precision: Option<u8>,
    #[serde(default, deserialize_with = "present_value", skip_serializing_if = "Option::is_none")]
    choices: Option<Vec<Choice>>,
    #[serde(default, deserialize_with = "present_value", skip_serializing_if = "Option::is_none")]
    item: Option<Vec<Field>>,
    #[serde(rename = "maxItems", default, deserialize_with = "present_value", skip_serializing_if = "Option::is_none")]
    max_items: Option<usize>,
}

impl Field {
    /// 插件清單原文可能是簡體：只轉顯示文字，key、default、選項 value 與單位數值都不動。
    fn localize_display(&mut self) {
        self.label = crate::zh::to_traditional(&self.label);
        if let Some(unit) = &mut self.unit {
            *unit = crate::zh::to_traditional(unit);
        }
        if let Some(choices) = &mut self.choices {
            for choice in choices {
                choice.label = crate::zh::to_traditional(&choice.label);
            }
        }
        if let Some(items) = &mut self.item {
            for field in items {
                field.localize_display();
            }
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Material {
    controls: bool,
    spring: MaterialSpring,
    #[serde(
        default,
        deserialize_with = "present_value",
        skip_serializing_if = "Option::is_none"
    )]
    shadow: Option<f32>,
    #[serde(default)]
    foreground: MaterialForeground,
}

fn present_value<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    input: D,
) -> std::result::Result<Option<T>, D::Error> {
    T::deserialize(input).map(Some)
}

#[derive(Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum MaterialForeground {
    #[default]
    Skin,
    Glass,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MaterialSpring {
    stiffness: f32,
    damping: f32,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Manifest {
    id: String,
    name: String,
    #[serde(default, deserialize_with = "present_value", skip_serializing_if = "Option::is_none")]
    title_accent: Option<String>,
    author: String,
    version: String,
    api: u32,
    runtime: String,
    entry: String,
    #[serde(
        default,
        deserialize_with = "present_value",
        skip_serializing_if = "Option::is_none"
    )]
    cover: Option<String>,
    owner: String,
    capabilities: Vec<String>,
    data_version: u32,
    permissions: BTreeMap<String, Value>,
    #[cfg(windows)]
    #[serde(default, deserialize_with = "present_value", skip_serializing_if = "Option::is_none")]
    network: Option<source_https::Network>,
    #[cfg(windows)]
    #[serde(default, deserialize_with = "present_value", skip_serializing_if = "Option::is_none")]
    attribution: Option<source_https::Attribution>,
    settings: Vec<Field>,
    #[serde(default = "default_refresh")]
    refresh_seconds: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    material: Option<Material>,
    #[serde(
        default,
        deserialize_with = "present_value",
        skip_serializing_if = "Option::is_none"
    )]
    status_layout: Option<StatusLayout>,
    #[serde(default, deserialize_with = "present_value", skip_serializing_if = "Option::is_none")]
    navigation_icon: Option<NavigationIcon>,
    #[serde(
        default,
        deserialize_with = "present_value",
        skip_serializing_if = "Option::is_none"
    )]
    events: Option<[EventName; 1]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    data: Vec<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    timezones: bool,
}
fn default_refresh() -> u32 {
    60
}
type Values = BTreeMap<String, String>;
#[derive(Clone, Serialize, Deserialize)]
struct Package {
    manifest: Manifest,
    source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cover: Option<CoverPng>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    data: PackageData,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    canvas_images: canvas_images::CanvasImages,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    font_assets: font_assets::Bundle,
    #[serde(skip)]
    page_shortcut_colors: OnceLock<BTreeMap<String, [u8; 3]>>,
    #[serde(skip)]
    appearance: OnceLock<appearance_assets::Pack>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Record {
    origin: PathBuf,
    #[serde(default)]
    from_market: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    official_market: bool,
    #[cfg(windows)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    presentation: Option<market::Presentation>,
    package: Option<Package>,
    enabled: bool,
    values: Values,
    storage: Value,
    #[serde(default)]
    material_granted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_grant: Option<SourceGrant>,
    #[cfg(windows)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_refresh_seconds: Option<u32>,
    #[cfg(windows)]
    #[serde(default, skip_serializing_if = "is_false")]
    market_revoked: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    status_page_disabled: bool,
    #[serde(default, skip_serializing_if = "NoticeHistory::is_empty")]
    notice_history: NoticeHistory,
}

fn is_false(value: &bool) -> bool {
    !value
}

impl Record {
    /// 舊版快照可能存著尚未轉繁的清單；載入時只補顯示文字，設定值與封包內容不重寫。
    fn localize_display(&mut self) {
        if let Some(package) = &mut self.package {
            package.manifest.localize_display();
        }
        #[cfg(windows)]
        if let Some(presentation) = &mut self.presentation {
            presentation.localize_display();
        }
    }

    fn source_kind(&self) -> Option<SourceKind> {
        self.package.as_ref()?.manifest.source_kind()
    }

    fn source_grant(&self) -> Option<&SourceGrant> {
        let grant = self.source_grant.as_ref()?;
        if Some(grant.kind()) != self.source_kind() {
            return None;
        }
        #[cfg(windows)]
        if grant.kind() == SourceKind::Https && !self.network()?.accepts(grant) {
            return None;
        }
        Some(grant)
    }

    #[cfg(windows)]
    fn network(&self) -> Option<&source_https::Network> {
        self.package.as_ref()?.manifest.network.as_ref()
    }

    fn refresh_interval(&self) -> Duration {
        let seconds = self.package.as_ref().unwrap().manifest.refresh_seconds;
        #[cfg(windows)]
        let seconds = self.network().map_or(seconds, |network| {
            network.refresh_seconds(seconds, self.source_refresh_seconds)
        });
        Duration::from_secs(u64::from(seconds))
    }

    fn read_source(&self, grant: &SourceGrant, cancelled: &AtomicBool) -> std::io::Result<String> {
        #[cfg(windows)]
        if let SourceGrant::Https { url } = grant {
            let network = self.network().ok_or_else(|| std::io::Error::other("缺少 HTTPS 網絡配置"))?;
            let id = &self.package.as_ref().unwrap().manifest.id;
            return network.read(id, url, cancelled);
        }
        grant.read(cancelled)
    }

    fn valid(&self, id: &str) -> bool {
        if !script_settings::valid_storage(&self.storage) {
            return false;
        }
        self.package.as_ref().is_none_or(|package| {
            let manifest = &package.manifest;
            manifest.id == id
                && manifest.validate().is_ok()
                && self.valid_wireless_package(package)
                && manifest.cover.is_some() == package.cover.is_some()
                && !package.source.trim().is_empty()
                && package.source.len() as u64 <= manifest.source_limit()
                && package_data::valid_snapshot(&manifest.data, &package.data)
                && (package.canvas_images.is_empty()
                    || manifest.has_capability("status.canvas") && canvas_images::valid_snapshot(&package.canvas_images))
                && package.validate_page_shortcut_colors().is_ok()
                && package.validate_appearance().is_ok()
                && package.valid_font_assets()
                && manifest.settings.iter().all(|field| {
                    self.values
                        .get(&field.key)
                        .is_some_and(|value| valid_field(field, value))
                })
        })
    }

    fn valid_wireless_package(&self, package: &Package) -> bool {
        #[cfg(windows)]
        if package.manifest.is_wireless() {
            return self.from_market && package.wireless_table().is_ok();
        }
        !package.manifest.is_wireless()
    }

    fn is_page_shortcuts(&self) -> bool {
        self.package
            .as_ref()
            .is_some_and(|package| package.manifest.is_page_shortcuts())
    }

    #[cfg(feature = "sdk-material")]
    fn material_package(&self) -> Option<&Package> {
        #[cfg(windows)]
        if self.market_revoked {
            return None;
        }
        self.package
            .as_ref()
            .filter(|package| package.manifest.is_material())
    }
}

#[cfg(feature = "sdk-material")]
pub struct MaterialSpec<'a> {
    pub id: &'a str,
    pub source: &'a str,
    pub controls: bool,
    pub stiffness: f32,
    pub damping: f32,
    pub shadow: Option<f32>,
    pub glass_foreground: bool,
    pub params: [f32; 32],
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Output {
    #[serde(default)]
    card: String,
    #[serde(default)]
    marker: String,
    #[serde(default)]
    quota: Option<PluginQuota>,
    #[serde(default)]
    status: Option<PluginStatus>,
    #[serde(default)]
    canvas: Option<Value>,
    #[serde(default)]
    notice: Option<ScriptNotice>,
    pinned: Option<pinned::Pinned>,
    #[serde(default)]
    storage: Value,
    #[serde(default)]
    next_refresh_at: Option<i64>,
}

#[derive(Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum SourceInput {
    Unconfigured,
    Ready { text: String },
    Offline,
    #[cfg(windows)]
    #[serde(skip)]
    Multiple(BTreeMap<String, SourceInput>),
}

impl SourceInput {
    fn ready(&self) -> bool {
        match self {
            Self::Ready { .. } => true,
            #[cfg(windows)]
            Self::Multiple(sources) => sources.values().any(Self::ready),
            _ => false,
        }
    }
}

fn bounded(path: &Path, max: u64) -> Result<String> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(max + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        return Err(fail("文件過大"));
    }
    Ok(String::from_utf8(bytes)?)
}
fn valid_id(id: &str) -> bool {
    id.len() <= 64
        && id.contains('.')
        && id.split('.').all(|p| {
            p.starts_with(|c: char| c.is_ascii_lowercase())
                && p.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}
impl Manifest {
    fn has_settings(&self) -> bool {
        if !self.settings.is_empty() {
            return true;
        }
        #[cfg(windows)]
        if self.source_kind() == Some(SourceKind::Https)
            && self.network.as_ref().is_some_and(|network| network.min_refresh_seconds.is_some())
        {
            return true;
        }
        false
    }

    /// 只把作者清單裡的顯示文字轉成繁體；識別碼、版本、鍵值、預設值與資料來源都保持原樣。
    fn localize_display(&mut self) {
        self.name = crate::zh::to_traditional(&self.name);
        self.author = crate::zh::to_traditional(&self.author);
        if let Some(accent) = &mut self.title_accent {
            *accent = crate::zh::to_traditional(accent);
        }
        for field in &mut self.settings {
            field.localize_display();
        }
        #[cfg(windows)]
        {
            if let Some(network) = &mut self.network {
                if let Some(credential) = &mut network.credential {
                    credential.label = crate::zh::to_traditional(&credential.label);
                }
            }
            if let Some(attribution) = &mut self.attribution {
                attribution.label = crate::zh::to_traditional(&attribution.label);
            }
        }
    }

    fn decode(contents: &str) -> Result<Self> {
        #[derive(Deserialize)]
        struct ApiHeader {
            api: u64,
        }
        // 先檢查版本，使未來清單新增的字段能夠得到明確的升級提示。
        let ApiHeader { api } = serde_json::from_str(contents)?;
        if api > u64::from(API_VERSION) {
            return Err(fail(&format!(
                "需要更新島仔。此插件需要 API v{api}，當前支持 v{API_VERSION}"
            )));
        }
        let mut manifest: Self = serde_json::from_str(contents)?;
        // 上游清單的中文是簡體；顯示層一律轉成繁體（台灣字形），不改 id、版本、設定值與結構。
        manifest.localize_display();
        manifest.validate()?;
        if manifest.cover.is_none() {
            return Err(fail("請在插件清單中聲明 cover: \"cover.png\""));
        }
        Ok(manifest)
    }

    fn is_material(&self) -> bool {
        self.runtime == "material"
    }

    fn is_wireless(&self) -> bool {
        self.runtime == "assets" && self.id == WIRELESS_PLUGIN_ID
    }

    fn is_page_shortcuts(&self) -> bool {
        self.runtime == "assets" && self.capabilities.as_slice() == ["navigation.shortcuts"]
    }

    fn appearance_kind(&self) -> Option<appearance_assets::Kind> {
        appearance_assets::Kind::of(self)
    }

    fn source_kind(&self) -> Option<SourceKind> {
        if !cfg!(windows) || self.runtime != "js" || self.permissions.len() != 1 {
            return None;
        }
        match self.permissions.get(SOURCE_PERMISSION)?.as_str()? {
            "file" => Some(SourceKind::File),
            "loopback" => Some(SourceKind::Loopback),
            #[cfg(windows)]
            "https" => Some(SourceKind::Https),
            _ => None,
        }
    }

    /// 這個插件的輸出落在哪些位置，由能力表從能力和歸屬算出。
    fn surfaces(&self) -> Vec<Surface> {
        capabilities::check(&self.runtime, &self.owner, &self.capabilities).unwrap_or_default()
    }

    fn has_surface(&self, surface: Surface) -> bool {
        self.surfaces().contains(&surface)
    }

    fn is_quota_source(&self) -> bool {
        self.has_surface(Surface::Quota)
    }

    fn is_status_page(&self) -> bool {
        self.has_surface(Surface::StatusPage)
    }

    /// 舊版日程歸屬的插件用 card 返回日曆標註。
    fn legacy_calendar_card(&self) -> bool {
        self.owner == "calendar" && self.capabilities.iter().any(|c| c == "card")
    }

    fn source_limit(&self) -> u64 {
        if self.is_material() {
            MATERIAL_SOURCE_LIMIT
        } else if self.is_page_shortcuts() {
            PAGE_SHORTCUT_SOURCE_LIMIT
        } else if let Some(kind) = self.appearance_kind() {
            kind.limit()
        } else {
            SCRIPT_SOURCE_LIMIT
        }
    }

    fn is_retired_lunar_preview(&self) -> bool {
        self.id == "daozai.lunar-today" && self.version == "1.0.0" && self.owner == "tasks"
    }

    fn validate(&self) -> Result<()> {
        let m = self;
        if !valid_id(&m.id)
            || !(60..=86400).contains(&m.refresh_seconds)
            || m.api != API_VERSION
            || m.data_version != 1
            || m.name.trim().is_empty()
            || m.name.chars().count() > 80
            || m.author.trim().is_empty()
            || m.author.chars().count() > 80
            || m.version.split('.').count() != 3
            || !m
                .version
                .split('.')
                .all(|v| v.parse::<u32>().is_ok_and(|n| n.to_string() == v))
        {
            return Err(fail("請核對插件 ID、作者、版本和 API 版本"));
        }
        capabilities::check(&m.runtime, &m.owner, &m.capabilities).map_err(|e| fail(&e))?;
        if m.navigation_icon.is_some() && (m.owner != "status" || !m.has_capability("status.page")) {
            return Err(fail("navigationIcon 只適用於聲明瞭 status.page 的獨立功能插件"));
        }
        if m.title_accent.as_deref().is_some_and(|suffix| {
            !m.has_capability("status.page")
                || !crate::plugin_status::valid_text(suffix, 80)
                || !m.name.strip_suffix(suffix).is_some_and(|prefix| !prefix.trim().is_empty())
        }) {
            return Err(fail("titleAccent 必須是狀態頁插件名稱的一段末尾文字"));
        }
        if m.has_capability("event") != m.events.is_some() {
            return Err(fail("聲明 event 時須在 events 中訂閱一次 focus.finished"));
        }
        if (!m.data.is_empty() || m.timezones) && m.runtime != "js"
            || !package_data::valid_names(&m.data)
        {
            return Err(fail("只有腳本插件可聲明時區與包根目錄中的 JSON 數據文件"));
        }
        if let Some(layout) = &m.status_layout {
            if !m.is_status_page() {
                return Err(fail("statusLayout 只適用於聲明瞭 status.page 的插件"));
            }
            layout.validate().map_err(fail)?;
            if !layout.canvas_fields.is_empty() && !m.has_capability("action") {
                return Err(fail("畫布輸入框需要聲明 action"));
            }
            if !m.has_capability("status.canvas") && layout.components.iter().any(|component| {
                matches!(component, crate::plugin_status::StatusComponent::Canvas {})
            }) {
                return Err(fail("畫布佈局組件需要聲明 status.canvas"));
            }
            if !m.has_capability("action") && layout.components.iter().any(|component| {
                matches!(component, crate::plugin_status::StatusComponent::Actions {} | crate::plugin_status::StatusComponent::Fields {})
            }) {
                return Err(fail("狀態頁的按鈕和表單組件需要 action 能力"));
            }
        }
        #[cfg(windows)]
        {
            let https = m.source_kind() == Some(SourceKind::Https);
            let action = m.capabilities.iter().any(|capability| capability == "action");
            if let Some(network) = &m.network {
                let defaults = m.settings.iter().map(|field| {
                    (field.key.clone(), field.default.clone())
                }).collect();
                if (!https && !(action && network.only_hosts())) || !network.valid(&defaults) {
                    return Err(fail("HTTPS 數據源或宿主打開鏈接需要有效的域名白名單和地址"));
                }
            } else if https {
                return Err(fail("HTTPS 數據源需要聲明 network"));
            }
            if let Some(attribution) = &m.attribution {
                if !https || !m.network.as_ref().is_some_and(|network| attribution.valid(network)) {
                    return Err(fail("數據來源標註須使用已聲明的 HTTPS 域名和有效名稱"));
                }
            }
        }
        let material = m.is_material();
        if m.is_wireless() {
            if !cfg!(windows)
                || m.entry != "wireless-receivers.json"
                || m.capabilities.as_slice() != ["wireless.receivers"]
                || m.refresh_seconds != default_refresh()
                || m.material.is_some()
                || !m.permissions.is_empty()
                || !m.settings.is_empty()
            {
                return Err(fail("請核對無線設備兼容包的清單"));
            }
        } else if m.id == WIRELESS_PLUGIN_ID || m.capabilities.iter().any(|c| c == "wireless.receivers") {
            return Err(fail("此插件 ID 專用於無線設備兼容包"));
        } else if let Some(kind) = m.appearance_kind() {
            if m.capabilities.as_slice() != [kind.capability()]
                || m.entry != kind.entry()
                || m.refresh_seconds != default_refresh()
                || !m.permissions.is_empty()
                || !m.settings.is_empty()
                || m.material.is_some()
            {
                return Err(fail("外觀資源包只接受一項資源能力和無權限的指定入口"));
            }
        } else if m.runtime == "assets" {
            if !m.is_page_shortcuts()
                || m.entry != PAGE_SHORTCUT_ENTRY
                || m.refresh_seconds != default_refresh()
                || !m.permissions.is_empty()
                || !m.settings.is_empty()
                || m.material.is_some()
            {
                return Err(fail("快捷切換插件僅接受頁面管理歸屬、單一導航能力和無權限的資源入口"));
            }
        } else if material {
            if m.permissions.len() != 1
                || m.permissions
                    .get(BACKDROP_PERMISSION)
                    .and_then(Value::as_bool)
                    != Some(true)
                || !m.material.as_ref().is_some_and(|material| {
                    (0.2..=3.0).contains(&material.spring.stiffness)
                        && (0.1..=1.0).contains(&material.spring.damping)
                        && material
                            .shadow
                            .is_none_or(|shadow| (0.0..=1.5).contains(&shadow))
                })
            {
                return Err(fail(
                    "請核對材質歸屬、屏幕底片權限、能力、彈簧參數和投影強度",
                ));
            }
        } else if m.material.is_some()
            || (!m.permissions.is_empty() && m.source_kind().is_none())
        {
            return Err(fail("腳本插件只能申請 source.read 一項權限，且不能帶材質參數"));
        }
        if m.settings.len() > 32
            || (!m.settings.is_empty() && !m.capabilities.iter().any(|c| c == "settings"))
            || !valid_fields(&m.settings, material)
        {
            return Err(fail("設置項名稱、類型、默認值或顯示格式無效"));
        }
        let entry = Path::new(&m.entry);
        let allowed_name = |s: &str| {
            !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        };
        let suffix = if material {
            ".wgsl"
        } else if m.runtime == "assets" {
            ".json"
        } else {
            ".js"
        };
        if !m
            .entry
            .strip_suffix(suffix)
            .is_some_and(|name| name.split('/').all(allowed_name))
            || !entry
                .components()
                .all(|c| matches!(c, Component::Normal(_)))
        {
            return Err(fail("插件入口文件的類型或路徑不符合清單要求"));
        }
        if m.cover.as_deref().is_some_and(|cover| cover != COVER_FILE) {
            return Err(fail("清單 cover 必須是根目錄文件 cover.png"));
        }
        Ok(())
    }
}

fn validate_code_free_files(
    dir: &Path, manifest: &Path, entry: &Path, material: bool, fonts: Option<&font_assets::Entry>,
) -> Result<()> {
    let (package_type, entry_type) = if material { ("材質包", "WGSL") } else { ("外觀資源包", "JSON") };
    let mut folders = vec![dir.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for item in fs::read_dir(folder)? {
            let item = item?;
            let path = item.path();
            let kind = item.file_type()?;
            if kind.is_symlink() || path.canonicalize()? != path {
                return Err(fail(&format!("{package_type}內不能包含鏈接或重定向路徑")));
            }
            if kind.is_dir() {
                folders.push(path);
                continue;
            }
            let description = path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| {
                    matches!(ext.to_ascii_lowercase().as_str(), "md" | "txt" | "rst")
                })
                || path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        matches!(
                            name.to_ascii_uppercase().as_str(),
                            "README" | "LICENSE" | "COPYING" | "NOTICE" | "CHANGELOG"
                        )
                    });
            if !kind.is_file()
                || (path != entry
                    && path != manifest
                    && path != dir.join(COVER_FILE)
                    && !description
                    && !fonts.is_some_and(|fonts| fonts.files().any(|file| path == dir.join(file))))
            {
                return Err(fail(&format!(
                    "{package_type}只能包含 plugin.json、一個 {entry_type} 入口、cover.png 和說明文件",
                )));
            }
        }
    }
    Ok(())
}

impl Package {
    #[cfg(windows)]
    fn wireless_table(&self) -> Result<crate::bt::compat::Package> {
        let table = crate::bt::compat::Package::parse(self.source.as_bytes()).map_err(fail)?;
        if table.version() != self.manifest.version {
            return Err(fail("無線設備兼容數據與插件清單版本不一致"));
        }
        Ok(table)
    }

    fn read(path: &Path) -> Result<Self> {
        let manifest = Manifest::decode(&bounded(path, 32 * 1024)?)?;
        let entry = Path::new(&manifest.entry);
        let base = path
            .parent()
            .ok_or_else(|| fail("缺少包目錄"))?
            .canonicalize()?;
        let cover = CoverPng::read(&base.join(COVER_FILE))?;
        let resolved = base.join(entry).canonicalize()?;
        if !resolved.starts_with(&base) || !resolved.is_file() {
            return Err(fail("入口越出插件目錄"));
        }
        let source = bounded(&resolved, manifest.source_limit())?;
        let data = package_data::read_data(&manifest.data, |name| package_data::read_file(&base, name))?;
        let mut package = Self::from_entry(manifest, source, cover)?;
        package.data = data;
        let manifest = &package.manifest;
        if manifest.is_material() || manifest.appearance_kind().is_some() {
            if path.file_name().and_then(|name| name.to_str()) != Some("plugin.json") {
                return Err(fail(if manifest.is_material() {
                    "材質清單必須命名為 plugin.json"
                } else {
                    "外觀資源包清單必須命名為 plugin.json"
                }));
            }
            validate_code_free_files(&base, &base.join("plugin.json"), &resolved, manifest.is_material(), package.font_entry())?;
        }
        if let Some(entry) = package.font_entry() {
            package.font_assets = font_assets::read_local(entry, &base, package.font_base_size()?)?;
        }
        if package.manifest.has_capability("status.canvas") {
            package.canvas_images = canvas_images::read(&base)?;
        }
        Ok(package)
    }

    fn from_entry(manifest: Manifest, source: String, cover: CoverPng) -> Result<Self> {
        if source.trim().is_empty() {
            return Err(fail("插件入口文件為空"));
        }
        let package = Self {
            source,
            manifest,
            cover: Some(cover),
            data: BTreeMap::new(),
            canvas_images: BTreeMap::new(),
            font_assets: BTreeMap::new(),
            page_shortcut_colors: OnceLock::new(),
            appearance: OnceLock::new(),
        };
        package.validate_page_shortcut_colors()?;
        package.validate_appearance()?;
        #[cfg(windows)]
        if package.manifest.is_wireless() {
            package.wireless_table()?;
        }
        Ok(package)
    }

    fn validate_appearance(&self) -> Result<()> {
        if let Some(kind) = self.manifest.appearance_kind().filter(|_| self.appearance.get().is_none()) {
            let _ = self.appearance.set(appearance_assets::Pack::decode(kind, &self.source)?);
        }
        Ok(())
    }

    fn font_entry(&self) -> Option<&font_assets::Entry> {
        match self.appearance.get()? {
            appearance_assets::Pack::Font(entry) => Some(entry),
            _ => None,
        }
    }

    fn font_base_size(&self) -> Result<u64> {
        font_assets::base_size(&self.manifest, &self.source, self.cover.as_ref().map_or(0, |cover| cover.bytes().len()))
    }

    fn valid_font_assets(&self) -> bool {
        match self.font_entry() {
            Some(entry) => self.font_base_size()
                .is_ok_and(|used| font_assets::valid_snapshot(entry, &self.font_assets, used)),
            None => self.font_assets.is_empty(),
        }
    }

    fn validate_page_shortcut_colors(&self) -> Result<()> {
        if !self.manifest.is_page_shortcuts() || self.page_shortcut_colors.get().is_some() {
            return Ok(());
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct ShortcutEntry {
            colors: BTreeMap<String, String>,
        }

        let ShortcutEntry { colors } = serde_json::from_str(&self.source)
            .map_err(|_| fail("快捷切換入口應包含 colors 對象"))?;
        if colors.is_empty() {
            return Err(fail("請為至少一個功能頁提供選中顏色"));
        }
        let palette = colors
            .into_iter()
            .map(|(page, color)| {
                if !crate::app::PAGE_SHORTCUT_KEYS.contains(&page.as_str()) {
                    return Err(fail(&format!("未識別的快捷切換頁面：{page}")));
                }
                let hex = color
                    .strip_prefix('#')
                    .filter(|hex| hex.len() == 6 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
                    .ok_or_else(|| fail(&format!("{page} 的選中顏色須為 #RRGGBB")))?;
                let [_, red, green, blue] = u32::from_str_radix(hex, 16)?.to_be_bytes();
                Ok((page, [red, green, blue]))
            })
            .collect::<Result<_>>()?;
        let _ = self.page_shortcut_colors.set(palette);
        Ok(())
    }

    fn cover_png(&self) -> Option<Vec<u8>> {
        self.cover.as_ref().map(|cover| cover.bytes().to_vec())
    }

    fn settings_values(&self, previous: &Values) -> Values {
        self.manifest
            .settings
            .iter()
            .map(|field| {
                let value = previous
                    .get(&field.key)
                    .filter(|value| valid_field(field, value))
                    .unwrap_or(&field.default);
                (field.key.clone(), value.clone())
            })
            .collect()
    }

    fn render(&self, values: &Values, storage: &Value, today: &str) -> Result<Output> {
        self.render_input(values, storage, today, SourceInput::Unconfigured, Call::Refresh)
    }

    fn render_input(
        &self,
        values: &Values,
        storage: &Value,
        today: &str,
        source: SourceInput,
        call: Call,
    ) -> Result<Output> {
        self.render_input_with(values, storage, today, source, call, None, &mut Vec::new())
            .map(|(output, _)| output)
    }

    fn render_input_with(
        &self,
        values: &Values,
        storage: &Value,
        today: &str,
        source: SourceInput,
        call: Call,
        mock: Option<&devtools::MockInput>,
        logs: &mut Vec<String>,
    ) -> Result<(Output, String)> {
        let mut input = json!({"settings":values,"storage":storage,"today":today});
        let now = match mock.and_then(|mock| mock.now) {
            Some(millis) => chrono::DateTime::from_timestamp_millis(millis)
                .ok_or_else(|| fail("模擬輸入 now 超出 Unix 毫秒時間戳的範圍"))?,
            None => chrono::Utc::now(),
        };
        script_settings::time_input(
            &mut input, now, self.manifest.timezones, &self.manifest.settings, values,
        );
        if !self.data.is_empty() {
            input["data"] = serde_json::to_value(package_data::data_input(&self.data)?)?;
        }
        let require_source = self.manifest.source_kind().is_some();
        let source_ready = source.ready();
        if require_source {
            #[cfg(windows)]
            if let Some(sources) = self.manifest.network.as_ref().and_then(|n| n.sources.as_ref()) {
                let sources = match source {
                    SourceInput::Multiple(sources) => sources,
                    _ => sources.iter().map(|source| {
                        (source.key.clone(), SourceInput::Unconfigured)
                    }).collect(),
                };
                input["sources"] = serde_json::to_value(sources)?;
            } else {
                input["source"] = serde_json::to_value(source)?;
            }
            #[cfg(not(windows))]
            {
                input["source"] = serde_json::to_value(source)?;
            }
        }
        let invocation = call.prepare(&mut input);
        let mocked_ready = mock.and_then(|mock| mock.apply(&mut input));
        let ready = !require_source || mocked_ready.unwrap_or(source_ready);
        let raw = devtools::run_script(
            self.source.as_bytes(), &self.manifest.entry, input.to_string(), invocation, logs,
        )?;
        let (output, storage) = script_settings::split_output(&raw)?;
        let mut output: Output = serde_json::from_value(output)?;
        output.storage = storage;
        let surfaces = self.manifest.surfaces();
        let has = |surface| surfaces.contains(&surface);
        if self.manifest.legacy_calendar_card() {
            if !output.marker.is_empty() {
                return Err(fail("日程歸屬的插件請繼續用 card 返回日曆標註"));
            }
            output.marker = std::mem::take(&mut output.card);
            if surfaces.len() == 1 && output.marker.trim().is_empty() {
                return Err(fail("卡片文字需要 1 至 160 字"));
            }
        }
        if let Some(status) = &output.status {
            if !has(Surface::StatusPage) {
                return Err(fail("此插件未聲明狀態頁能力"));
            }
            if !self.manifest.has_capability("action") && status.has_interactions() {
                return Err(fail("此插件未聲明狀態頁交互能力"));
            }
            if !status.valid() {
                return Err(fail("狀態頁需要返回格式有效的 status 或 null"));
            }
            self.manifest.check_action_links(status)?;
        }
        if let Some(canvas) = output.canvas.take() {
            let invalid = |reason: &str| fail(&format!("插件畫面有誤：{reason}"));
            if !self.manifest.has_capability("status.canvas") {
                return Err(invalid("插件未聲明 status.canvas"));
            }
            if self.manifest.status_layout.as_ref().is_some_and(|layout| {
                !layout.components.iter().any(|component| {
                    matches!(component, crate::plugin_status::StatusComponent::Canvas {})
                })
            }) {
                return Err(invalid("有自定義佈局時，需要在 statusLayout 中加入 canvas 組件"));
            }
            let canvas = Canvas::parse(canvas).map_err(|error| invalid(&error))?;
            canvas.validate_images(|name| self.canvas_images.contains_key(name))
                .map_err(|error| invalid(&error))?;
            self.manifest.check_canvas_actions(&canvas.nodes)
                .map_err(|error| invalid(&error.to_string()))?;
            if output.status.is_none() {
                output.status = Some(serde_json::from_value(json!({
                    "state": "idle", "label": self.manifest.name
                }))?);
            }
            output.status.as_mut().unwrap().canvas = Some(Arc::new(canvas));
        }
        if let Some(quota) = &output.quota {
            if !has(Surface::Quota) {
                return Err(fail("此插件未聲明額度來源能力"));
            }
            if !quota.valid() {
                return Err(fail("額度來源需要返回格式有效的 quota 或 null"));
            }
        }
        if output.notice.as_ref().is_some_and(|notice| {
            !self.manifest.has_capability("notice") || !notice.valid()
        }) {
            return Err(fail("插件提醒需要 notice 能力以及格式有效的 id、title 和 body"));
        }
        if output.pinned.as_ref().is_some_and(|pinned| !has(Surface::IslandPinned) || !pinned.valid()) {
            return Err(fail("常駐位需要聲明 island.pinned 並返回符合格式的 pinned"));
        }
        if let Some(pinned) = &output.pinned {
            pinned.validate_canvas(self)?;
        }
        if !ready {
            output.status = None;
            output.quota = None;
            output.pinned = None;
        }
        if has(Surface::TasksCard) {
            // 只有卡片一種輸出時必須有字；同時有其他輸出時，空字表示這次不顯示卡片。
            let blank = output.card.trim().is_empty();
            if output.card.chars().count() > 160 || (blank && surfaces.len() == 1) {
                return Err(fail("卡片文字需要 1 至 160 字"));
            }
        } else if !output.card.is_empty() {
            return Err(fail("此插件未聲明卡片能力"));
        }
        if has(Surface::CalendarMarker) {
            if output.marker.chars().count() > 160 {
                return Err(fail("日曆標註最多 160 字"));
            }
        } else if !output.marker.is_empty() {
            return Err(fail("此插件未聲明日曆標註能力"));
        }
        if !output.storage.is_null() && !self.manifest.capabilities.iter().any(|s| s == "storage") {
            return Err(fail("未聲明私有存儲"));
        }
        Ok((output, raw))
    }
}

// 後台先寫好臨時快照。只有界面仍接受這次結果，才替換正式記錄。
struct Staged(PathBuf);
impl Drop for Staged {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
enum Rendered {
    Output(Output, Option<Staged>),
    StatusOffline,
}
struct Job {
    cancelled: Arc<AtomicBool>,
    reading_source: bool,
    #[cfg(windows)]
    fetch: std::sync::mpsc::Receiver<source_https::FetchState>,
    result: std::thread::JoinHandle<std::result::Result<Rendered, String>>,
}
pub struct LocalPlugins {
    jobs: BTreeMap<String, Job>,
    devtools: devtools::DevTools,
    pinned: pinned::PinnedState,
    #[cfg(windows)]
    wireless: crate::bt::compat::Store,
    #[cfg(windows)]
    market_catalog: Option<std::sync::Arc<market::Catalog>>,
    #[cfg(windows)]
    official_market: market_state::OfficialMarket,
    #[cfg(windows)]
    market_job: Option<market_state::MarketJob>,
    #[cfg(windows)]
    market_favorites: BTreeMap<String, String>,
    #[cfg(windows)]
    market_metrics: market_metrics::MarketMetrics,
    invalid: std::collections::BTreeSet<String>,
    dir: PathBuf,
    records: BTreeMap<String, Record>,
    appearance: appearance_assets::Selection,
    active_page_shortcuts: Option<String>,
    cards: BTreeMap<String, String>,
    markers: BTreeMap<String, String>,
    /// 上一輪開著的輸出位置。模塊重新打開時，讓其他歸屬的插件立刻補算這個位置的內容。
    live: Vec<Surface>,
    quota: BTreeMap<String, PluginQuota>,
    status: BTreeMap<String, std::result::Result<(u64, PluginStatus), StatusHealth>>,
    interactions: InteractionState,
    calendar_day: String,
    next: BTreeMap<String, Instant>,
    #[cfg(windows)]
    source_fetch: BTreeMap<String, source_https::FetchState>,
    errors: BTreeMap<String, String>,
    notice: String,
    pending: Option<String>,
    focus: Option<String>,
    drafts: BTreeMap<String, Values>,
    card_index: usize,
    #[cfg(feature = "sdk-material")]
    selected_material: Option<String>,
}
impl LocalPlugins {
    pub fn open(dir: PathBuf) -> Self {
        let mut s = Self {
            jobs: BTreeMap::new(),
            devtools: devtools::DevTools::default(),
            pinned: pinned::PinnedState::open(&dir),
            #[cfg(windows)]
            wireless: Default::default(),
            #[cfg(windows)]
            market_catalog: None,
            #[cfg(windows)]
            official_market: Default::default(),
            #[cfg(windows)]
            market_job: None,
            #[cfg(windows)]
            market_favorites: Default::default(),
            #[cfg(windows)]
            market_metrics: market_metrics::MarketMetrics::open(&dir),
            invalid: Default::default(),
            #[cfg(feature = "sdk-material")]
            selected_material: Self::read_material_id(&dir),
            appearance: appearance_assets::Selection::read(&dir),
            dir,
            records: BTreeMap::new(),
            active_page_shortcuts: None,
            cards: BTreeMap::new(),
            markers: BTreeMap::new(),
            live: Vec::new(),
            quota: BTreeMap::new(),
            status: BTreeMap::new(),
            interactions: InteractionState::default(),
            calendar_day: String::new(),
            next: BTreeMap::new(),
            #[cfg(windows)]
            source_fetch: BTreeMap::new(),
            errors: BTreeMap::new(),
            notice: String::new(),
            pending: None,
            focus: None,
            drafts: BTreeMap::new(),
            card_index: 0,
        };
        let read = (|| -> Result<()> {
            fs::create_dir_all(&s.dir)?;
            for item in fs::read_dir(&s.dir)? {
                let p = item?.path();
                if p.extension().and_then(|v| v.to_str()) != Some("json") {
                    continue;
                }
                let id = p
                    .file_stem()
                    .and_then(|v| v.to_str())
                    .and_then(|s| s.strip_prefix("local-"))
                    .unwrap_or_default();
                if !valid_id(id) {
                    continue;
                }
                // 快照中的腳本、包數據和設置字符串在 JSON 中還會轉義。
                let parsed = bounded(&p, font_assets::SNAPSHOT_LIMIT)
                    .ok()
                    .and_then(|v| {
                        let mut record = serde_json::from_str::<Record>(&v).ok()?;
                        record.localize_display();
                        (v.len() <= 4 * 1024 * 1024 || record.package.as_ref().is_some_and(|package| {
                            package.manifest.has_capability("appearance.font")
                        })).then_some(record)
                    });
                let valid = parsed.as_ref().is_some_and(|r| r.valid(id));
                if valid {
                    s.records.insert(id.into(), parsed.unwrap());
                } else {
                    s.invalid.insert(id.into());
                }
            }
            Ok(())
        })();
        if let Err(e) = read {
            s.notice = e.to_string()
        }
        s.restore_pins();
        s.refresh_page_shortcuts();
        s.retain_available_appearance();
        #[cfg(windows)]
        {
            s.restore_market_catalog();
            s.restore_official_revocations();
            s.sync_wireless();
        }
        s
    }

    #[cfg(windows)]
    pub(crate) fn wireless_store(&self) -> crate::bt::compat::Store {
        self.wireless.clone()
    }

    #[cfg(windows)]
    fn sync_wireless(&self) {
        let table = self
            .records
            .get(WIRELESS_PLUGIN_ID)
            .filter(|record| record.from_market && record.enabled)
            .and_then(|record| record.package.as_ref())
            .and_then(|package| package.wireless_table().ok());
        self.wireless.set(table);
    }

    fn path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("local-{id}.json"))
    }
    fn put(&mut self, id: &str, r: Record) -> Result<()> {
        let shortcuts_changed = r.is_page_shortcuts()
            || self.records.get(id).is_some_and(Record::is_page_shortcuts);
        if r.enabled
            && r.is_page_shortcuts()
            && self.active_page_shortcuts.as_deref().is_some_and(|active| active != id)
        {
            return Err(fail("只能啟用一個快捷切換插件，請先停用當前使用的插件"));
        }
        if !r.enabled || r.package.is_none() {
            self.forget_appearance(id)?;
        }
        crate::model::atomic_write(&self.path(id), &serde_json::to_vec(&r)?)?;
        self.records.insert(id.into(), r);
        self.restore_pins();
        if shortcuts_changed {
            self.refresh_page_shortcuts();
        }
        #[cfg(windows)]
        {
            self.cancel_market(id);
            if id == WIRELESS_PLUGIN_ID {
                self.sync_wireless();
            }
        }
        Ok(())
    }

    fn refresh_page_shortcuts(&mut self) {
        self.active_page_shortcuts = self
            .records
            .iter()
            .find(|(_, record)| record.enabled && record.is_page_shortcuts())
            .map(|(id, _)| id.clone());
    }

    pub(crate) fn active_page_shortcut_colors(&self) -> Option<&BTreeMap<String, [u8; 3]>> {
        self.records
            .get(self.active_page_shortcuts.as_deref()?)?
            .package
            .as_ref()?
            .page_shortcut_colors
            .get()
    }

    #[cfg(feature = "sdk-material")]
    fn read_material_id(dir: &Path) -> Option<String> {
        let text = bounded(&dir.join("active-material.json"), 128).ok()?;
        serde_json::from_str::<Option<String>>(&text)
            .ok()?
            .filter(|id| valid_id(id))
    }

    #[cfg(feature = "sdk-material")]
    pub fn startup_material(dir: &Path) -> bool {
        Self::read_material_id(dir).is_some_and(|id| {
            bounded(&dir.join(format!("local-{id}.json")), 2 * 1024 * 1024)
                .ok()
                .and_then(|text| serde_json::from_str::<Record>(&text).ok())
                .is_some_and(|record| {
                    record.material_granted
                        && record.material_package().is_some()
                        && record.valid(&id)
                })
        })
    }

    #[cfg(feature = "sdk-material")]
    pub fn selected_material_id(&self) -> Option<&str> {
        self.selected_material
            .as_deref()
            .filter(|id| self.material_permission_granted(id))
    }

    #[cfg(feature = "sdk-material")]
    pub fn material_permission_granted(&self, id: &str) -> bool {
        self.records
            .get(id)
            .is_some_and(|record| record.material_granted && record.material_package().is_some())
    }

    #[cfg(feature = "sdk-material")]
    pub fn activate_material(&mut self, id: Option<&str>, grant: bool) -> Result<()> {
        if let Some(id) = id {
            let mut record = self.installed(id)?;
            #[cfg(windows)]
            if record.market_revoked {
                return Err(fail("當前插件版本已撤回，請更新後啟用"));
            }
            if record.material_package().is_none() {
                return Err(fail("所選插件不提供 GPU 材質"));
            }
            if !record.material_granted {
                if !grant {
                    return Err(fail("請先確認讀取島體背後屏幕畫面的權限"));
                }
                record.material_granted = true;
                self.put(id, record)?;
            }
        }
        if self.selected_material.as_deref() != id {
            crate::model::atomic_write(
                &self.dir.join("active-material.json"),
                &serde_json::to_vec(&id)?,
            )?;
            self.selected_material = id.map(str::to_owned);
        }
        Ok(())
    }

    #[cfg(feature = "sdk-material")]
    pub fn active_material(&self) -> Option<MaterialSpec<'_>> {
        let id = self.selected_material_id()?;
        let record = self.records.get(id)?;
        let package = record.material_package()?;
        let material = package.manifest.material.as_ref()?;
        let mut params = [0.0; 32];
        for (param, field) in params.iter_mut().zip(&package.manifest.settings) {
            *param = record.values.get(&field.key)?.parse().ok()?;
        }
        Some(MaterialSpec {
            id,
            source: &package.source,
            controls: material.controls,
            stiffness: material.spring.stiffness,
            damping: material.spring.damping,
            shadow: material.shadow,
            glass_foreground: matches!(material.foreground, MaterialForeground::Glass),
            params,
        })
    }

    #[cfg(feature = "sdk-material")]
    pub fn set_material_parameter(&mut self, id: &str, key: &str, value: f32) -> Result<()> {
        let mut record = self.installed(id)?;
        let field = record
            .material_package()
            .and_then(|package| {
                package
                    .manifest
                    .settings
                    .iter()
                    .find(|field| field.key == key)
            })
            .ok_or_else(|| fail("找不到該材質參數"))?;
        let value = value.to_string();
        if !valid_field(field, &value) {
            return Err(fail("材質參數超出有效範圍"));
        }
        if record.values.get(key) != Some(&value) {
            record.values.insert(key.to_owned(), value);
            self.put(id, record)?;
        }
        Ok(())
    }
    fn forget_view(&mut self, id: &str) {
        if let Some(job) = self.jobs.get(id) {
            job.cancelled.store(true, Ordering::Release);
        }
        self.cards.remove(id);
        self.markers.remove(id);
        self.quota.remove(id);
        self.status.remove(id);
        self.interactions.forget(id);
        self.pinned.set_output(id, None);
        self.next.remove(id);
        #[cfg(windows)]
        self.source_fetch.remove(id);
        self.errors.remove(id);
        self.drafts.remove(id);
    }
    pub(crate) fn install(&mut self, path: &Path) -> Result<()> {
        let path = path.canonicalize()?;
        let pkg = Package::read(&path)?;
        if pkg.manifest.is_wireless() {
            return Err(fail("請從插件集市安裝無線設備兼容包"));
        }
        if !cfg!(all(windows, feature = "sdk-material")) && pkg.manifest.is_material() {
            return Err(fail("當前版本未啟用 Windows GPU 材質支持"));
        }
        let id = pkg.manifest.id.clone();
        let mut record = Record {
            origin: path.clone(),
            from_market: false,
            official_market: false,
            #[cfg(windows)]
            presentation: None,
            values: Values::new(),
            enabled: false,
            package: None,
            storage: Value::Null,
            material_granted: false,
            source_grant: None,
            #[cfg(windows)]
            source_refresh_seconds: None,
            #[cfg(windows)]
            market_revoked: false,
            status_page_disabled: false,
            notice_history: NoticeHistory::default(),
        };
        if let Some(old) = self.records.get(&id) {
            if old.package.is_some() {
                return Err(fail("此插件 ID 已安裝"));
            }
            if old.origin != path || old.from_market {
                return Err(fail("保留數據屬於原本地清單，請從原位置重裝"));
            }
            record.storage = old.storage.clone();
            record.notice_history = old.notice_history.clone();
            record.values = old.values.clone();
        } else if self.path(&id).exists() && !self.invalid.contains(&id) {
            return Err(fail("已有無法讀取的插件記錄"));
        }
        record.values = pkg.settings_values(&record.values);
        record.package = Some(pkg);
        self.put(&id, record)?;
        self.invalid.remove(&id);
        Ok(())
    }
    fn installed(&self, id: &str) -> Result<Record> {
        self.records
            .get(id)
            .filter(|r| r.package.is_some())
            .cloned()
            .ok_or_else(|| fail("插件未安裝"))
    }
    pub(crate) fn enable(&mut self, id: &str, on: bool) -> Result<()> {
        let mut r = self.installed(id)?;
        #[cfg(windows)]
        if on && r.market_revoked {
            return Err(fail("當前插件版本已撤回，請更新後啟用"));
        }
        if r.package.as_ref().unwrap().manifest.is_material() {
            #[cfg(feature = "sdk-material")]
            if on || self.selected_material_id() == Some(id) {
                return self.activate_material(on.then_some(id), false);
            } else {
                return Ok(());
            }
            #[cfg(not(feature = "sdk-material"))]
            return Err(fail("當前版本未啟用 GPU 材質支持"));
        }
        r.enabled = on;
        self.put(id, r)?;
        self.forget_view(id);
        Ok(())
    }

    pub(crate) fn set_status_page_enabled(&mut self, id: &str, on: bool) -> Result<()> {
        let mut record = self.installed(id)?;
        if !record.package.as_ref().unwrap().manifest.is_status_page() {
            return Err(fail("此插件沒有獨立狀態頁"));
        }
        let disabled = !on;
        if record.status_page_disabled != disabled {
            record.status_page_disabled = disabled;
            self.put(id, record)?;
            self.forget_view(id);
        }
        Ok(())
    }

    #[cfg(windows)]
    fn source_kind(&self, id: &str) -> Option<SourceKind> {
        self.records.get(id)?.source_kind()
    }

    #[cfg(windows)]
    fn source_grant(&self, id: &str) -> Option<&SourceGrant> {
        self.records.get(id)?.source_grant()
    }

    #[cfg(windows)]
    fn set_source_grant(&mut self, id: &str, grant: Option<SourceGrant>) -> Result<()> {
        self.set_source_grant_with_secret(id, grant, None)
    }

    #[cfg(windows)]
    fn set_source_grant_with_secret(&mut self, id: &str, grant: Option<SourceGrant>, secret: Option<&str>) -> Result<()> {
        let mut record = self.installed(id)?;
        let kind = record.source_kind().ok_or_else(|| fail("此插件未聲明數據源"))?;
        if grant.as_ref().is_some_and(|grant| grant.kind() != kind) {
            return Err(fail("數據源類型與插件申請不一致"));
        }
        let credential = record.network().and_then(|network| network.credential.as_ref());
        let previous_secret = if let Some(grant) = grant.as_ref().filter(|grant| grant.kind() == SourceKind::Https) {
            if !record.network().is_some_and(|network| network.accepts(grant)) {
                return Err(fail("授權範圍與插件的 HTTPS 數據源聲明不一致"));
            }
            if let Some(config) = credential {
                let secret = secret.ok_or_else(|| fail("請輸入數據源密鑰"))?;
                config.header_value(secret)?;
                self.forget_view(id);
                let previous = source_https::load_secret(id)?;
                source_https::save_secret(id, secret)?;
                Some(previous)
            } else {
                None
            }
        } else {
            if credential.is_some() && grant.is_none() {
                self.forget_view(id);
                source_https::delete_secret(id)?;
            }
            None
        };
        if record.source_grant != grant {
            record.source_grant = grant;
            if let Err(error) = self.put(id, record) {
                match previous_secret {
                    Some(Some(previous)) => source_https::save_secret(id, &previous)?,
                    Some(None) => source_https::delete_secret(id)?,
                    None => {}
                }
                return Err(error);
            }
            self.forget_view(id);
        }
        Ok(())
    }
    fn save(&mut self, id: &str, v: Values) -> Result<()> {
        let mut r = self.installed(id)?;
        #[cfg(windows)]
        let v = {
            let mut values = v;
            if let Some(seconds) = values.remove(source_https::REFRESH_FIELD) {
                let min = r.network().and_then(|network| network.min_refresh_seconds)
                    .ok_or_else(|| fail("此插件未開放刷新間隔設置"))?;
                let seconds = seconds.parse::<u32>().ok()
                    .filter(|value| (min..=source_https::MAX_REFRESH_SECONDS).contains(value))
                    .ok_or_else(|| fail("刷新間隔超出允許範圍"))?;
                r.source_refresh_seconds = Some(seconds);
            }
            values
        };
        if !r
            .package
            .as_ref()
            .unwrap()
            .manifest
            .settings
            .iter()
            .all(|f| v.get(&f.key).is_some_and(|s| valid_field(f, s)))
        {
            return Err(fail("請檢查設置格式和取值範圍"));
        }
        r.values = v;
        self.put(id, r)?;
        self.forget_view(id);
        Ok(())
    }
    fn uninstall(&mut self, id: &str, keep: bool) -> Result<()> {
        let mut r = self.installed(id)?;
        #[cfg(windows)]
        if r.network().is_some_and(|network| network.credential.is_some()) {
            self.forget_view(id);
            source_https::delete_secret(id)?;
        }
        #[cfg(feature = "sdk-material")]
        if self.selected_material.as_deref() == Some(id) {
            self.activate_material(None, false)?;
        }
        if keep {
            r.package = None;
            r.enabled = false;
            r.material_granted = false;
            r.source_grant = None;
            r.status_page_disabled = false;
            self.put(id, r)?
        } else {
            self.forget_appearance(id)?;
            fs::remove_file(self.path(id))?;
            self.records.remove(id);
            if r.is_page_shortcuts() {
                self.refresh_page_shortcuts();
            }
            #[cfg(windows)]
            {
                self.cancel_market(id);
                if id == WIRELESS_PLUGIN_ID {
                    self.sync_wireless();
                }
            }
        }
        self.forget_view(id);
        self.restore_pins();
        Ok(())
    }
    /// 移除重啟檢查沒通過的安裝記錄。插件已經停止運行，這裡只刪掉留下的記錄文件。
    fn remove_invalid(&mut self, id: &str) -> Result<()> {
        if !self.invalid.contains(id) {
            return Err(fail("沒有需要移除的記錄"));
        }
        match fs::remove_file(self.path(id)) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
            _ => {}
        }
        self.invalid.remove(id);
        Ok(())
    }
    pub fn pause_owner(&mut self, owner: &str) {
        let records = &self.records;
        let belongs = |id: &str| {
            records
                .get(id)
                .and_then(|record| record.package.as_ref())
                .is_some_and(|package| package.manifest.owner == owner)
        };
        match owner {
            "tasks" => self.cards.clear(),
            "calendar" => self.markers.clear(),
            "quota" => self.quota.clear(),
            "status" => self.status.clear(),
            _ => {}
        }
        self.next.retain(|id, _| !belongs(id));
        self.status.retain(|id, _| !belongs(id));
        self.interactions.retain(|id| !belongs(id));
        #[cfg(windows)]
        self.source_fetch.retain(|id, _| !belongs(id));
        self.pinned.retain_outputs(|id| !belongs(id));
        for (id, job) in &self.jobs {
            if belongs(id) {
                job.cancelled.store(true, Ordering::Release);
            }
        }
    }

    #[cfg(test)]
    fn tick(&mut self, tasks_on: bool, calendar_on: bool, today: &str) {
        self.tick_features(tasks_on, calendar_on, false, today);
    }

    pub(crate) fn tick_features(&mut self, tasks_on: bool, calendar_on: bool, quota_on: bool, today: &str) {
        self.poll_local_reloads(Instant::now());
        #[cfg(windows)]
        self.poll_market();
        let new_day = calendar_on && self.calendar_day != today;
        for (owner, surface, on) in [
            ("tasks", Surface::TasksCard, tasks_on),
            ("calendar", Surface::CalendarMarker, calendar_on),
            ("quota", Surface::Quota, quota_on),
        ] {
            let was_on = self.live.contains(&surface);
            if !on {
                self.pause_owner(owner);
                self.live.retain(|live| *live != surface);
            } else if !was_on || (surface == Surface::CalendarMarker && new_day) {
                if was_on {
                    // 換日：昨天的標註作廢，日程歸屬的插件也重新計算。
                    self.pause_owner(owner);
                }
                self.recompute_surface(owner, surface);
                if !was_on {
                    self.live.push(surface);
                }
            }
        }
        if calendar_on {
            self.calendar_day = today.to_owned();
        }
        let finished: Vec<_> = self
            .jobs
            .iter()
            .filter(|(_, j)| j.result.is_finished())
            .map(|(id, _)| id.clone())
            .collect();
        for id in finished {
            let job = self.jobs.remove(&id).unwrap();
            if job.cancelled.load(Ordering::Acquire) {
                continue;
            }
            #[cfg(windows)]
            if let Ok(fetch) = job.fetch.try_recv() {
                if let Some(next) = fetch.next_due() {
                    self.next.insert(id.clone(), next);
                }
                self.source_fetch.insert(id.clone(), fetch);
            }
            let result = job
                .result
                .join()
                .unwrap_or_else(|_| Err("插件線程退出".into()))
                .and_then(|rendered| {
                    let Rendered::Output(mut output, staged) = rendered else {
                        return Ok(None);
                    };
                    if let Some(staged) = staged {
                        crate::model::replace_file(&staged.0, &self.path(&id))
                            .map_err(|e| e.to_string())?;
                        self.records.get_mut(&id).unwrap().storage = std::mem::take(&mut output.storage);
                    }
                    Ok(Some(output))
                });
            match result {
                Ok(Some(Output { card, marker, quota, status, notice, next_refresh_at, pinned, .. })) => {
                    self.interactions.forms.remove(&id);
                    #[cfg(windows)]
                    if let Some(notice) = notice {
                        self.interactions.notices.insert(id.clone(), notice);
                    }
                    #[cfg(not(windows))]
                    let _ = notice;
                    self.errors.remove(&id);
                    let Some(manifest) = self
                        .records
                        .get(&id)
                        .and_then(|record| record.package.as_ref())
                        .map(|package| &package.manifest)
                    else {
                        continue;
                    };
                    if let Some(at) = next_refresh_at {
                        if manifest.capabilities.iter().any(|cap| cap == "timer") {
                            let delay = script_settings::next_delay(chrono::Utc::now().timestamp(), at, manifest.refresh_seconds);
                            let now = Instant::now();
                            if let Some(next) = self.next.get_mut(&id) {
                                *next = (*next).min(now + delay).max(now + Duration::from_secs(60));
                            }
                        }
                    }
                    let live = self.live.clone();
                    for surface in manifest.surfaces() {
                        // 模塊關著時這一位置的結果直接丟掉，重新打開時會補算。
                        let shown = surface == Surface::StatusPage || live.contains(&surface);
                        match surface {
                            Surface::IslandPinned => self.pinned.set_output(&id, pinned.as_ref()),
                            Surface::StatusPage => match &status {
                                Some(status) => {
                                    self.interactions.revision += 1;
                                    self.status.insert(id.clone(), Ok((self.interactions.revision, status.clone())));
                                }
                                None => {
                                    self.status.remove(&id);
                                }
                            },
                            Surface::Quota => match &quota {
                                Some(quota) if shown => {
                                    self.quota.insert(id.clone(), quota.clone());
                                }
                                _ => {
                                    self.quota.remove(&id);
                                }
                            },
                            Surface::TasksCard => {
                                if shown && !card.trim().is_empty() {
                                    self.cards.insert(id.clone(), card.clone());
                                } else {
                                    self.cards.remove(&id);
                                }
                            }
                            Surface::CalendarMarker => {
                                if shown && !marker.trim().is_empty() {
                                    self.markers.insert(id.clone(), marker.clone());
                                } else {
                                    self.markers.remove(&id);
                                }
                            }
                        }
                    }
                }
                Ok(None) => {
                    self.errors.remove(&id);
                    self.interactions.forms.remove(&id);
                    self.pinned.set_output(&id, None);
                    self.status.insert(id, Err(StatusHealth::Offline));
                }
                Err(e) => {
                    self.cards.remove(&id);
                    self.markers.remove(&id);
                    self.quota.remove(&id);
                    self.status.remove(&id);
                    self.interactions.forms.remove(&id);
                    self.pinned.set_output(&id, None);
                    self.errors.insert(id, e);
                }
            }
        }
        if self.jobs.len() >= 2 {
            return;
        }
        let now = Instant::now();
        let due: Vec<_> = self
            .records
            .iter()
            .filter(|(id, r)| {
                !self.jobs.contains_key(*id)
                    && !self.interactions.notices.contains_key(*id)
                    && r.enabled
                    && r.package.as_ref().is_some_and(|pkg| {
                        r.owner_on(tasks_on, calendar_on, quota_on)
                            && (self.interactions.has_call(id) || self.next.get(*id).is_none_or(|t| {
                                *t <= now && pkg.manifest.has_capability("timer")
                            }))
                    })
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in due {
            if self.jobs.len() >= 2 {
                break;
            }
            let reading_source = self.records[&id].source_grant().is_some();
            // 慢數據源只佔一個後台名額，普通插件仍可刷新。
            if reading_source && self.jobs.values().any(|job| job.reading_source) {
                continue;
            }
            let call = self.interactions.pop_call(&id);
            let mut r = self.records[&id].clone();
            let today = today.to_owned();
            let interval = r.refresh_interval();
            self.next.insert(id.clone(), now + interval);
            static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let seq = SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let stage = self
                .path(&id)
                .with_extension(format!("{}-{seq}.pending", std::process::id()));
            let cancelled = Arc::new(AtomicBool::new(false));
            let worker_cancelled = Arc::clone(&cancelled);
            #[cfg(windows)]
            let (send_fetch, fetch_report) = std::sync::mpsc::channel();
            #[cfg(windows)]
            let mut fetch = self.source_fetch.get(&id).cloned().unwrap_or_default();
            let script_logs = self.devtools.logs.clone();
            let plugin_id = id.clone();
            let result = std::thread::Builder::new().name("daozai-plugin-read".into()).spawn(move || {
                let source = match r.source_grant() {
                    #[cfg(windows)]
                    Some(SourceGrant::HttpsSources { .. }) => {
                        let package = r.package.as_ref().unwrap();
                        let source = r.network().unwrap().read_sources(
                            &package.manifest.id, &r.values, &worker_cancelled, &mut fetch, interval,
                        );
                        let _ = send_fetch.send(fetch);
                        source
                    }
                    #[cfg(windows)]
                    Some(grant @ SourceGrant::Https { .. }) => {
                        let source = fetch.read("", Instant::now(), interval, || r.read_source(grant, &worker_cancelled));
                        let _ = send_fetch.send(fetch);
                        source
                    }
                    Some(grant) => match r.read_source(grant, &worker_cancelled) {
                        Ok(text) => SourceInput::Ready { text },
                        Err(_) => SourceInput::Offline,
                    },
                    None => SourceInput::Unconfigured,
                };
                if worker_cancelled.load(Ordering::Acquire) {
                    return Err("讀取已取消".into());
                }
                let package = r.package.as_ref().unwrap();
                if matches!(&call, Call::Refresh)
                    && package.manifest.is_status_page()
                    && matches!(&source, SourceInput::Offline)
                {
                    return Ok(Rendered::StatusOffline);
                }
                let mut logs = Vec::new();
                #[cfg(windows)]
                let status_offline = package.manifest.is_status_page()
                    && matches!(&source, SourceInput::Multiple(_))
                    && !source.ready();
                let output = package
                    .render_input_with(&r.values, &r.storage, &today, source, call, None, &mut logs);
                if worker_cancelled.load(Ordering::Acquire) {
                    return Err("讀取已取消".into());
                }
                if !r.from_market {
                    if let Err(error) = &output {
                        let detail = error.to_string();
                        if detail.starts_with("插件畫面有誤") {
                            logs.push(detail);
                        }
                    }
                    script_logs.append(&plugin_id, logs);
                }
                let (output, _) = output.map_err(|error| error.to_string())?;
                #[cfg(windows)]
                if status_offline {
                    return Ok(Rendered::StatusOffline);
                }
                let staged = if r.storage != output.storage {
                    r.storage = output.storage.clone();
                    let data = serde_json::to_vec(&r).map_err(|e| e.to_string())?;
                    crate::model::atomic_write(&stage, &data).map_err(|e| e.to_string())?;
                    Some(Staged(stage))
                } else {
                    None
                };
                Ok(Rendered::Output(output, staged))
            });
            match result {
                Ok(result) => {
                    self.jobs.insert(id, Job {
                        cancelled,
                        reading_source,
                        #[cfg(windows)]
                        fetch: fetch_report,
                        result,
                    });
                }
                Err(_) => {
                    self.status.remove(&id);
                    self.errors.insert(id, "暫時無法啟動插件刷新，請稍後重試".into());
                }
            }
        }
    }

    /// 某個模塊重新打開或換日時，讓帶有這個輸出位置、但歸屬在別處的插件馬上重算。
    fn recompute_surface(&mut self, owner: &str, surface: Surface) {
        let ids: Vec<_> = self
            .records
            .iter()
            .filter(|(_, record)| {
                record.package.as_ref().is_some_and(|package| {
                    package.manifest.owner != owner && package.manifest.has_surface(surface)
                })
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            if let Some(job) = self.jobs.get(&id) {
                job.cancelled.store(true, Ordering::Release);
            }
            self.next.remove(&id);
        }
    }

    pub(crate) fn quota_sources(&self) -> impl Iterator<Item = (&str, &str, &PluginQuota)> {
        self.quota.iter().filter_map(|(id, quota)| {
            let record = self.records.get(id).filter(|record| record.enabled)?;
            let package = record.package.as_ref()?;
            Some((id.as_str(), package.manifest.name.as_str(), quota))
        })
    }

    pub(crate) fn refresh_quota(&mut self) {
        self.next.retain(|id, _| {
            !self.records.get(id).and_then(|record| record.package.as_ref())
                .is_some_and(|package| package.manifest.is_quota_source())
        });
    }

    pub(crate) fn status_page_preferences(&self) -> impl Iterator<Item = (&str, &str, bool, bool)> {
        self.records.iter().filter_map(|(id, record)| {
            let package = record
                .package
                .as_ref()
                .filter(|package| package.manifest.is_status_page())?;
            Some((
                id.as_str(),
                package.manifest.name.as_str(),
                record.enabled,
                !record.status_page_disabled,
            ))
        })
    }

    pub(crate) fn status_pages(&self) -> impl Iterator<Item = (&str, &str)> {
        self.status_page_preferences()
            .filter_map(|(id, name, plugin_on, page_on)| (plugin_on && page_on).then_some((id, name)))
    }

    pub(crate) fn status_layout(&self, id: &str) -> Option<&StatusLayout> {
        self.records
            .get(id)?
            .package
            .as_ref()?
            .manifest
            .status_layout
            .as_ref()
    }

    pub(crate) fn navigation_icon(&self, id: &str) -> Option<&NavigationIcon> {
        self.records.get(id)?.package.as_ref()?.manifest.navigation_icon.as_ref()
    }

    pub(crate) fn status_title_accent(&self, id: &str) -> Option<(&str, Option<[u8; 3]>)> {
        let manifest = &self.records.get(id)?.package.as_ref()?.manifest;
        let suffix = manifest.title_accent.as_deref()?;
        let color = manifest.status_layout.as_ref().and_then(|layout| layout.style.accent_rgb());
        Some((suffix, color))
    }

    #[cfg(windows)]
    pub(crate) fn source_attribution(
        &self,
        id: &str,
    ) -> Option<(&str, Option<&str>, Option<&chrono::DateTime<chrono::Local>>)> {
        let attribution = self.records.get(id)?.package.as_ref()?.manifest.attribution.as_ref()?;
        let last_success = self.source_fetch.get(id).and_then(|fetch| fetch.last_success.as_ref());
        Some((&attribution.label, attribution.url.as_deref(), last_success))
    }

    pub(crate) fn status_page(&self, id: &str) -> Option<(&str, Option<&PluginStatus>, StatusHealth)> {
        let record = self
            .records
            .get(id)
            .filter(|record| record.enabled && !record.status_page_disabled)?;
        let package = record
            .package
            .as_ref()
            .filter(|package| package.manifest.is_status_page())?;
        let (snapshot, health) = match self.status.get(id) {
            Some(Ok((_, snapshot))) if snapshot.fresh_at(chrono::Utc::now().timestamp()) => {
                (Some(snapshot), StatusHealth::Ready)
            }
            Some(Ok(_)) => (None, StatusHealth::Stale),
            Some(Err(health)) => (None, *health),
            None if record.source_kind().is_some() && record.source_grant().is_none() => {
                (None, StatusHealth::Unconfigured)
            }
            None if self.errors.get(id).is_some_and(|error| error.starts_with("插件畫面有誤")) => {
                (None, StatusHealth::CanvasInvalid)
            }
            None if self.errors.contains_key(id) => (None, StatusHealth::Invalid),
            None => (None, StatusHealth::Waiting),
        };
        Some((package.manifest.name.as_str(), snapshot, health))
    }
    pub fn has_card(&self) -> bool {
        !self.cards.is_empty()
    }

    pub fn calendar_annotation_enabled(&self) -> bool {
        self.records.values().any(|record| {
            record.enabled
                && record
                    .package
                    .as_ref()
                    .is_some_and(|package| package.manifest.has_surface(Surface::CalendarMarker))
        })
    }

    pub fn calendar_annotation(&self, today: &str) -> Option<&str> {
        if self.calendar_day != today {
            return None;
        }
        self.markers.values().next().map(String::as_str)
    }
}

impl Drop for LocalPlugins {
    fn drop(&mut self) {
        for job in self.jobs.values() {
            job.cancelled.store(true, Ordering::Release);
        }
    }
}

#[cfg(test)]
#[path = "local_management_test.rs"]
mod management_test;
#[cfg(test)]
#[path = "local_tests.rs"]
mod tests;
#[cfg(all(test, windows))]
#[path = "source_lifecycle_tests.rs"]
mod source_lifecycle_tests;
#[cfg(all(test, windows))]
#[path = "quota_source_tests.rs"]
mod quota_source_tests;
#[cfg(all(test, windows))]
#[path = "status_source_tests.rs"]
mod status_source_tests;
#[cfg(test)]
#[path = "script_input_tests.rs"]
mod script_input_tests;

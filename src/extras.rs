//! 設定、通知隊列與鬼島表情。
use crate::settings_theme::color as settings_color;

/// 贊助頁：原本放在「關於」最底下的支援作者（收款碼）區塊，獨立成一頁，排在「關於」上面。
pub(crate) const SETTINGS_TAB_SUPPORT: usize = 17;
/// 贊助頁在正式版與開發版都保留；「關於」頁本身不再出現任何贊助內容。
pub(crate) fn support_page_enabled() -> bool {
    true
}
use crate::app::{
    App, Cmd, Env, NotificationSound, Phase, HUD_TRACK_W, HUD_TRACK_X, HUD_W, PAGE_NAMES,
};
use crate::paint::{Card, H};
use crate::theme::{self, size, W};
use eframe::egui::{self, Color32, Rect};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AlertSource {
    Internal,
    MailCode(String,u32),
    Messenger(crate::app::Messenger),
    External(String),
    #[cfg(windows)]
    Plugin { source: String, id: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HealthReminder {
    Water,
    Sit,
}
impl HealthReminder {
    pub(crate) const ALL: [Self; 2] = [Self::Water, Self::Sit];
    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Water => "喝水時間到了",
            Self::Sit => crate::app::SIT_TITLE,
        }
    }
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Water => "待喝水",
            Self::Sit => "待活動",
        }
    }
    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Water => "health-water",
            Self::Sit => "health-sit",
        }
    }
    pub(crate) fn icon(self, c: &Card, x: f32, y: f32, scale: f32) {
        let c = &c.scaled(x, y, scale);
        match self {
            Self::Water => {
                c.poly(
                    &[
                        (x - 7.0, y - 1.0),
                        (x + 5.0, y - 1.0),
                        (x + 3.5, y + 8.0),
                        (x - 5.5, y + 8.0),
                    ],
                    theme::BLUE,
                );
                c.path(
                    &[
                        (x - 9.0, y - 10.0),
                        (x - 6.5, y + 9.0),
                        (x + 4.5, y + 9.0),
                        (x + 7.0, y - 10.0),
                    ],
                    1.6,
                    theme::CYAN,
                );
                c.seg((x - 9.0, y - 10.0), (x + 7.0, y - 10.0), 1.6, theme::CYAN);
                c.path(
                    &[
                        (x + 6.5, y - 6.0),
                        (x + 10.5, y - 6.0),
                        (x + 11.5, y - 3.0),
                        (x + 10.0, y + 2.0),
                        (x + 5.5, y + 3.0),
                    ],
                    1.6,
                    theme::CYAN,
                );
                c.seg(
                    (x - 5.0, y - 6.5),
                    (x - 4.2, y - 2.5),
                    1.0,
                    Color32::from_white_alpha(170),
                );
            }
            Self::Sit => crate::paint::chair(c, x, y, 0.6, theme::SOFT_GREEN),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Alert {
    pub title: String,
    pub source: AlertSource,
    pub body: String,
    pub page: usize,
    pub at: f64,
    pub duration: f64,
    pub bluetooth: Option<(crate::bt::DeviceKind, bool)>,
    pub meeting: Option<(crate::mail::Meeting, String)>,
    pub unread: Option<crate::app::Unread>,
}
impl Alert {
    pub(crate) fn health(&self) -> Option<HealthReminder> {
        if self.source != AlertSource::Internal {
            return None;
        }
        HealthReminder::ALL
            .into_iter()
            .find(|kind| self.title == kind.title())
    }
    pub(crate) fn messenger(&self) -> Option<crate::app::Messenger> {
        match &self.source {
            AlertSource::Messenger(client) => Some(*client),
            AlertSource::External(id) => {
                crate::app::Messenger::from_notification_id(id).filter(|client| {
                    client.adapter().capture == crate::messenger::Capture::SystemToast
                })
            }
            _ => None,
        }
    }
    pub(crate) fn is_hud(&self) -> bool {
        self.source == AlertSource::Internal
            && matches!(
                self.title.as_str(),
                "系統音量"
                    | "大寫鎖定"
                    | "數字鍵盤"
                    | "鍵盤輸入語言"
                    | "網路連線"
                    | "電源已連線"
                    | "已切換電池供電"
                    | "藍牙裝置"
            )
    }
}

fn reminder_module(title:&str)->Option<usize> {
    use crate::app::*;
    match title {
        "島仔已暫停音樂功能"=>Some(MUSIC_PAGE),
        "專注計時"=>Some(FOCUS_PAGE),
        "喝水時間到了"|SIT_TITLE|"今日飲水目標達成"=>Some(WATER_PAGE),
        "待辦到時間了"=>Some(TODO_PAGE),
        "新郵件"=>Some(MAIL_PAGE),
        "會議提醒"=>Some(CALENDAR_PAGE),
        _=>None,
    }
}

fn mail_invitation(title:&str,body:&str)->bool {
    title=="會議提醒" && body.ends_with(" · 郵件邀請")
}

fn bluetooth_notice_content(body:&str)->(&str,&str,Option<u8>){
    if let Some(name)=body.strip_suffix(" · 已斷開") {return (name,"已斷開",None);}
    if body=="已斷開連線" {return ("藍牙裝置","已斷開",None);}
    if let Some((name,level))=body.rsplit_once(" · 電量 ") {
        if let Some(level)=level.strip_suffix('%').and_then(|s|s.parse::<u8>().ok()).filter(|v|*v<=100){
            return (name,"已連線",Some(level));
        }
    }
    // 電量事件：「名稱 · 開始充電 · 64%」。
    if let Some((rest,level))=body.rsplit_once(" · ") {
        if let (Some(level),Some((name,status)))=(level.strip_suffix('%').and_then(|s|s.parse::<u8>().ok()).filter(|v|*v<=100),rest.rsplit_once(" · ")) {
            return (name,status,Some(level));
        }
    }
    (body,"已連線",None)
}

/// 每種裝置一個圖標；入耳、骨傳導、耳夾等各有外形，AirPods 系列按帶杆樣式單獨畫。
pub(crate) fn device_icon_source(kind: crate::bt::DeviceKind) -> egui::ImageSource<'static> {
    use crate::bt::DeviceKind::*;
    match kind {
        Headphones=>egui::include_image!("../assets/device-icons/headphones-round-bold.svg"),
        Earbuds=>egui::include_image!("../assets/device-icons/earbuds.svg"),
        EarbudsStem=>egui::include_image!("../assets/device-icons/earbuds-stem.svg"),
        EarbudsStemTips=>egui::include_image!("../assets/device-icons/earbuds-stem-tips.svg"),
        OpenEar=>egui::include_image!("../assets/device-icons/open-ear.svg"),
        Bone=>egui::include_image!("../assets/device-icons/bone.svg"),
        HearingAid=>egui::include_image!("../assets/device-icons/hearing-aid.svg"),
        Speaker=>egui::include_image!("../assets/device-icons/speaker-bold.svg"),
        Mic=>egui::include_image!("../assets/device-icons/mic.svg"),
        Keyboard=>egui::include_image!("../assets/device-icons/keyboard-bold.svg"),
        Mouse=>egui::include_image!("../assets/device-icons/mouse-bold.svg"),
        Trackpad=>egui::include_image!("../assets/device-icons/trackpad.svg"),
        Pen=>egui::include_image!("../assets/device-icons/pen.svg"),
        Presenter=>egui::include_image!("../assets/device-icons/presenter.svg"),
        Remote=>egui::include_image!("../assets/device-icons/remote.svg"),
        Gamepad=>egui::include_image!("../assets/device-icons/gamepad-bold.svg"),
        JoyCon=>egui::include_image!("../assets/device-icons/joycon.svg"),
        Phone=>egui::include_image!("../assets/device-icons/smartphone-bold.svg"),
        Tablet=>egui::include_image!("../assets/device-icons/tablet.svg"),
        Watch=>egui::include_image!("../assets/device-icons/watch.svg"),
        Band=>egui::include_image!("../assets/device-icons/band.svg"),
        Health=>egui::include_image!("../assets/device-icons/health.svg"),
        Receiver24G=>egui::include_image!("../assets/device-icons/usb-receiver.svg"),
        Other=>egui::include_image!("../assets/device-icons/bluetooth-linear.svg"),
    }
}
pub(crate) const ALL_DEVICE_KINDS:[crate::bt::DeviceKind;24]={
    use crate::bt::DeviceKind::*;
    [Headphones,Earbuds,EarbudsStem,EarbudsStemTips,OpenEar,Bone,HearingAid,Speaker,Mic,Keyboard,Mouse,Trackpad,Pen,Presenter,Remote,Gamepad,JoyCon,Phone,Tablet,Watch,Band,Health,Receiver24G,Other]
};
pub(crate) fn device_icon(c: &Card, kind: crate::bt::DeviceKind, x: f32, y: f32, size: f32) {
    device_icon_tinted(c,kind,x,y,size,Color32::WHITE);
}
pub(crate) fn device_icon_tinted(c: &Card, kind: crate::bt::DeviceKind, x: f32, y: f32, size: f32, tint: Color32) {
    let icon=device_icon_source(kind);
    let rect=c.rect(x,y,size,size);
    let egui::ImageSource::Bytes{uri,bytes}=icon else{return};
    let pixels=(rect.width()*c.ui.ctx().pixels_per_point()).ceil() as u32;
    let icon=egui::Image::from_bytes(format!("{uri}/{pixels}.svg"),bytes);
    if let Ok(egui::load::TexturePoll::Ready { texture })=icon.fit_to_exact_size(rect.size()).load_for_size(c.ui.ctx(),rect.size()) {
        crate::skin_paint::mark_monochrome(texture.id);
        c.painter.image(texture.id,rect,Rect::from_min_max(egui::Pos2::ZERO,egui::pos2(1.0,1.0)),tint);
    }
}

pub(crate) use crate::pet::PetState;

pub(crate) fn notice_badge(c: &Card, x: f32, y: f32, a: &Alert) {
    if a.source == AlertSource::Internal && a.title == "藍牙裝置" {
        c.circle(
            x,
            y,
            11.0,
            if a.body.contains("已斷開") {
                Color32::from_gray(95)
            } else {
                theme::BLUE
            },
        );
        device_icon(
            c,
            a.bluetooth.map_or(crate::bt::DeviceKind::Other, |v| v.0),
            x - 7.5,
            y - 7.5,
            15.0,
        );
    } else {
        notice_icon(&c.scaled(x, y, 0.85), x, y, a);
    }
}
pub(crate) fn messenger_icon(c: &Card, x: f32, y: f32, source: crate::app::Messenger) {
    (source.adapter().paint_icon)(c, x, y);
}
fn notice_icon(c: &Card, x: f32, y: f32, a: &Alert) {
    if a.source != AlertSource::Internal {
        crate::paint::bell(c, x, y, theme::WHITE, 0.0);
        return;
    }
    match a.title.as_str() {
        "系統音量" => {
            c.poly(
                &[
                    (x - 9.0, y - 3.0),
                    (x - 5.0, y - 3.0),
                    (x, y - 7.0),
                    (x, y + 7.0),
                    (x - 5.0, y + 3.0),
                    (x - 9.0, y + 3.0),
                ],
                theme::WHITE,
            );
            c.arc(x, y, 5.0, -0.8, 0.8, 1.5, theme::WHITE);
            c.arc(x, y, 8.0, -0.7, 0.7, 1.3, theme::WHITE);
        }
        "大寫鎖定" => {
            let ink = if a.body == "已開啟" {
                theme::GREEN
            } else {
                theme::GRAY
            };
            c.poly(&[(x - 8.0, y - 1.0), (x, y - 9.0), (x + 8.0, y - 1.0)], ink);
            c.fill(x - 3.0, y - 1.5, 6.0, 6.5, 0.5, ink);
            c.fill(x - 4.0, y + 7.0, 8.0, 2.0, 0.8, ink);
        }
        "數字鍵盤" => {
            let ink = if a.body == "已開啟" {
                theme::GREEN
            } else {
                theme::GRAY
            };
            c.stroke(x - 9.0, y - 8.0, 18.0, 16.0, 3.0, 1.2, ink);
            for row in 0..3 {
                for col in 0..3 {
                    c.fill(
                        x - 6.5 + col as f32 * 5.0,
                        y - 5.5 + row as f32 * 4.5,
                        2.4,
                        2.0,
                        0.4,
                        ink,
                    );
                }
            }
            c.fill(x - 5.0, y + 5.0, 10.0, 1.8, 0.6, ink);
        }
        "鍵盤輸入語言" => {
            c.stroke(x - 10.0, y - 8.5, 20.0, 17.0, 4.0, 1.2, theme::GRAY);
            let label = match a.body.as_str() {
                "中文" => "中",
                "英語" => "EN",
                _ => "Aa",
            };
            c.centered_text(
                x,
                y,
                label,
                if label == "EN" { 9.0 } else { 11.0 },
                W::Semibold,
                theme::WHITE,
                H::Center,
            );
        }
        "網路連線" => {
            let connected = a.body.contains("已連線");
            let ink = if connected { theme::GREEN } else { theme::GRAY };
            if a.body.starts_with("Wi-Fi") {
                for r in [4.0, 8.0, 12.0] {
                    c.arc(x, y + 6.0, r, -2.35, -0.79, 1.7, ink);
                }
                c.circle(x, y + 6.0, 1.6, ink);
            } else if a.body.starts_with("乙太網") {
                c.stroke(x - 8.0, y - 8.0, 16.0, 14.0, 2.5, 1.6, ink);
                for dx in [-4.0, -1.3, 1.3, 4.0] {
                    c.seg((x + dx, y - 5.0), (x + dx, y - 1.0), 1.2, ink);
                }
                c.path(
                    &[
                        (x - 4.0, y + 6.0),
                        (x - 4.0, y + 9.0),
                        (x + 4.0, y + 9.0),
                        (x + 4.0, y + 6.0),
                    ],
                    1.6,
                    ink,
                );
            } else {
                c.ring(x, y, 8.0, 1.4, ink);
                c.seg((x - 8.0, y), (x + 8.0, y), 1.0, ink);
                c.stroke(x - 3.5, y - 8.0, 7.0, 16.0, 3.5, 1.0, ink);
            }
            if !connected {
                c.seg((x - 8.0, y - 8.0), (x + 9.0, y + 9.0), 2.0, theme::WHITE);
            }
        }
        "電源已連線" | "已切換電池供電" | "電量偏低" => {
            let low = a.title == "電量偏低";
            let ink = if low {
                theme::RED_TOP
            } else if a.title == "電源已連線" {
                theme::GREEN
            } else {
                theme::WHITE
            };
            c.stroke(x - 11.0, y - 6.0, 20.0, 12.0, 3.0, 1.5, ink);
            c.fill(x + 10.0, y - 2.5, 2.0, 5.0, 0.8, ink);
            if low {
                c.fill(x - 8.0, y - 3.5, 3.5, 7.0, 1.0, ink);
            } else if a.title == "電源已連線" {
                c.poly(&[(x, y - 4.5), (x - 4.0, y + 0.8), (x + 0.5, y + 0.8)], ink);
                c.poly(
                    &[(x + 0.5, y - 0.8), (x + 4.0, y - 0.8), (x - 1.0, y + 4.5)],
                    ink,
                );
            } else {
                c.fill(x - 8.0, y - 3.5, 12.0, 7.0, 1.0, ink);
            }
        }
        "喝水時間到了" => HealthReminder::Water.icon(c, x, y, 1.0),
        crate::app::SIT_TITLE => HealthReminder::Sit.icon(c, x, y, 1.0),
        "待辦到時間了" => crate::paint::checklist(
            c,
            x,
            y,
            23.0,
            Color32::from_rgb(190, 180, 255),
            Color32::from_rgb(43, 35, 66),
        ),
        "會議提醒" => {
            let label = a
                .meeting
                .as_ref()
                .map(|(_, source)| source.as_str())
                .unwrap_or("");
            let source = if label == "本機行程" {
                crate::calendar_ui::Source::Local
            } else if label.starts_with("郵件") {
                crate::calendar_ui::Source::Mail("")
            } else {
                crate::calendar_ui::Source::Calendar(
                    crate::calendar::Provider::ALL
                        .into_iter()
                        .find(|provider| provider.label() == label),
                )
            };
            crate::calendar_ui::source_icon(c, source, x - 14.0, y - 14.0, 28.0);
        }
        "專注計時" => {
            crate::paint::stopwatch(&c.scaled(x, y, 0.65), x, y, 8.0, 0.0, theme::RED_TOP)
        }
        _ => crate::paint::bell(&c.scaled(x, y, 1.7), x, y, theme::GRAY, 0.0),
    }
}

impl App {
    pub(crate) fn health_badges(&self) -> Vec<HealthReminder> {
        if self.fullscreen_hidden || !self.module_on(crate::app::WATER_PAGE) {
            return Vec::new();
        }
        HealthReminder::ALL
            .into_iter()
            .filter(|kind| {
                self.health_pending[*kind as usize]
                    && (match kind {
                        HealthReminder::Water => self.water.remind,
                        HealthReminder::Sit => self.settings.sit_remind,
                    })
                    && (self.open
                        || !self
                            .notice
                            .as_ref()
                            .is_some_and(|a| a.health() == Some(*kind)))
            })
            .collect()
    }
    pub(crate) fn confirm_health_reminder(&mut self, kind: HealthReminder) {
        self.health_pending[kind as usize] = false;
        crate::model::save_health_pending(self.health_pending);
        match kind {
            HealthReminder::Water => self.last_reminder = self.t,
            HealthReminder::Sit => self.sit_start = self.t,
        }
        self.alerts.retain(|a| a.health() != Some(kind));
        if self
            .notice
            .as_ref()
            .is_some_and(|a| a.health() == Some(kind))
        {
            self.notice = None;
        }
        if self
            .hud_notice
            .as_ref()
            .is_some_and(|a| a.health() == Some(kind))
        {
            self.hud_notice = None;
        }
    }
    pub(crate) fn phase_secs(&self, phase: Phase) -> f32 {
        (match phase {
            Phase::Focus => self.settings.focus_minutes,
            Phase::Rest => self.settings.rest_minutes,
        }) as f32
            * 60.0
    }
    /// 已連續久坐的分鐘數。
    pub(crate) fn sat_minutes(&self) -> u32 {
        ((self.t - self.sit_start).max(0.0) / 60.0) as u32
    }
    /// 下一次久坐提醒的時刻：從上次確認或起身起算滿 sit_minutes。
    pub(crate) fn sit_due(&self) -> f64 {
        self.sit_start + self.settings.sit_minutes as f64 * 60.0
    }
    /// 「起身了」：久坐計時歸零。
    pub(crate) fn stood_up(&mut self) {
        self.confirm_health_reminder(HealthReminder::Sit);
        self.pet.react(PetState::Happy);
        self.say("久坐計時已重新開始");
    }
    /// 現在是否在午休免打擾時段內（支援跨午夜；起止相同視為未設定）。
    pub(crate) fn lunch_now(&self, env: &Env) -> bool {
        let (a, b) = (self.settings.lunch_start, self.settings.lunch_end);
        let m = env.hour() as u32 * 60 + env.minute();
        self.settings.lunch_rest
            && a != b
            && if a < b {
                m >= a && m < b
            } else {
                m >= a || m < b
            }
    }
    /// 喝水和久坐提醒此刻能否彈出：在提醒時段內，且不在午休。
    pub(crate) fn remind_window(&self, env: &Env) -> bool {
        self.water_hours(env.hour() as u32) && !self.lunch_now(env)
    }
    pub(crate) fn water_hours(&self, hour: u32) -> bool {
        let (a, b) = (self.settings.water_start, self.settings.water_end);
        if a == b {
            true
        } else if a < b {
            hour >= a && hour < b
        } else {
            hour >= a || hour < b
        }
    }
    pub(crate) fn pet_state(&self) -> PetState {
        if let Some((state, _)) = self.pet.reaction {
            state
        } else if self.pet.waking_until > self.t {
            PetState::Waking
        } else if self.pomo.running && self.pomo.phase == Phase::Focus {
            PetState::Focused
        } else if self.pet.worried {
            PetState::Worried
        } else if self.media.playing && self.media.is_music() {
            if self.notice.is_some() || self.hud_notice.is_some() {
                PetState::Awake
            } else {
                PetState::Music
            }
        } else if self.pet.hovered {
            PetState::Curious
        } else if self.idle_for() >= 180.0 {
            PetState::Sleeping
        } else if self.idle_for() >= self.opts.pet_idle {
            PetState::Dozing
        } else {
            PetState::Awake
        }
    }
    pub fn alert(
        &mut self,
        title: impl Into<String>,
        body: impl Into<String>,
        page: usize,
        system: bool,
    ) {
        let title = title.into();
        let transient = matches!(
            title.as_str(),
            "系統音量"
                | "大寫鎖定"
                | "數字鍵盤"
                | "鍵盤輸入語言"
                | "網路連線"
                | "電源已連線"
                | "已切換電池供電"
                | "藍牙裝置"
        );
        let a = Alert {
            source: AlertSource::Internal,
            title,
            body: body.into(),
            page,
            at: self.t,
            duration: if transient { 2.5 } else { 8.0 },
            bluetooth: None,
            meeting: None,
            unread: None,
        };
        self.enqueue_alert(a, system);
    }
    pub(crate) fn meeting_alert(
        &mut self,
        meeting: crate::mail::Meeting,
        source: String,
        page: usize,
    ) {
        self.enqueue_alert(
            Alert {
                source: AlertSource::Internal,
                title: "會議提醒".into(),
                body: format!("{} · {} · {}", meeting.title, meeting.at, source),
                page,
                at: self.t,
                duration: 12.0,
                bluetooth: None,
                meeting: Some((meeting, source)),
                unread: None,
            },
            true,
        );
    }
    pub(crate) fn new_mail_alert(&mut self, body: String) {
        self.alert("新郵件",body,crate::app::MAIL_PAGE,false);
        self.mail.bell_at=Some(self.t);
    }
    pub(crate) fn mail_code_alert(&mut self, account: String, uid: u32, body: String) {
        self.enqueue_alert(Alert {
            source: AlertSource::MailCode(account,uid), title: "新郵件".into(), body,
            page: crate::app::MAIL_PAGE, at: self.t, duration: 8.0, bluetooth: None, meeting: None, unread: None,
        }, false);
    }
    pub(crate) fn bluetooth_alert(&mut self, device: &crate::bt::Device, connected: bool) {
        if !self.settings.bluetooth_notifications { return; }
        if connected && device.kind.is_audio() {
            self.alerts.retain(|a|a.title!="系統音量");
            if self.notice.as_ref().is_some_and(|a|a.title=="系統音量") {self.notice=None;}
        }
        let a = Alert {
            source: AlertSource::Internal,
            title: "藍牙裝置".into(),
            body: if connected {
                device.summary()
            } else {
                format!("{} · 已斷開", device.name)
            },
            page: 5,
            at: self.t,
            duration: 2.5,
            bluetooth: Some((device.kind, connected)),
            meeting: None,
            unread: None,
        };
        self.enqueue_alert(a, false);
    }
    pub(crate) fn enqueue_alert(&mut self, a: Alert, system: bool) {
        if a.source==AlertSource::Internal &&
            (reminder_module(&a.title).is_some_and(|page|!self.module_on(page))
                || mail_invitation(&a.title,&a.body)&&!self.module_on(crate::app::MAIL_PAGE)) {return;}
        if let Some(kind)=a.health() {
            if self.health_pending[kind as usize]{return;}
            self.health_pending[kind as usize]=true;
            crate::model::save_health_pending(self.health_pending);
        }
        // 通知僅儲存在本次運行的內存中。
        if matches!(a.source, AlertSource::Internal | AlertSource::MailCode(..)) {
            self.history.push_front(Alert { source: AlertSource::Internal, ..a.clone() });
        }
        self.history.truncate(50);
        if self.fullscreen_hidden && a.health().is_some() {
            return;
        }
        if self.settings.notifications {
            const MAX_PENDING_ALERTS: usize = 8;
            #[cfg(windows)]
            if let AlertSource::Plugin { source, .. } = &a.source {
                let pending = self.alerts.iter_mut().find(|alert| {
                    matches!(&alert.source, AlertSource::Plugin { source: current, .. } if current == source)
                });
                if let Some(pending) = pending {
                    *pending = a;
                } else if self.alerts.len() < MAX_PENDING_ALERTS {
                    self.alerts.push_back(a);
                }
                return;
            }
            if system && a.source==AlertSource::Internal && self.settings.system_notify {
                let sound = if !self.settings.sound { NotificationSound::Silent }
                    else if a.health().is_some() { NotificationSound::Health }
                    else { NotificationSound::System };
                self.pending.push(Cmd::Notify(a.title.clone(), a.body.clone(), sound));
            }
            let same_event = |old: &Alert| match (&a.meeting, &old.meeting) {
                (Some((next, _)), Some((previous, _))) => {
                    crate::calendar::dedupe_key(next) == crate::calendar::dedupe_key(previous)
                }
                _ => {
                    old.source == a.source
                        && old.title == a.title
                        && old.unread == a.unread
                        && (a.is_hud() || old.body == a.body)
                }
            };
            if a.source != AlertSource::Internal || a.title != "藍牙裝置" {
                self.alerts.retain(|old| !same_event(old));
            }
            if (a.source != AlertSource::Internal || a.title != "藍牙裝置")
                && self.notice.as_ref().is_some_and(same_event)
            {
                self.notice = Some(a);
                return;
            }
            let index = self.alerts.len();
            #[cfg(windows)]
            let index = {
                if self.notice.as_ref().is_some_and(|alert| matches!(alert.source, AlertSource::Plugin { .. })) {
                    self.notice = None;
                }
                self.alerts.iter().position(|alert| matches!(alert.source, AlertSource::Plugin { .. })).unwrap_or(index)
            };
            self.alerts.insert(index, a);
            while self.alerts.len() > MAX_PENDING_ALERTS {
                #[cfg(windows)]
                if let Some(index) = self.alerts.iter().position(|alert| matches!(alert.source, AlertSource::Plugin { .. })) {
                    self.alerts.remove(index);
                    continue;
                }
                self.alerts.pop_front();
            }
        }
    }
    /// 清除模塊尚在顯示、排隊或等待交給系統的提醒。跳轉頁碼可能與其他提醒共用。
    pub(crate) fn clear_module_reminders(&mut self,page:usize) {
        let belongs=|title:&str,body:&str|reminder_module(title)==Some(page)
            || page==crate::app::MAIL_PAGE&&mail_invitation(title,body);
        let matches=|a:&Alert|matches!(a.source,AlertSource::Internal|AlertSource::MailCode(..))&&belongs(&a.title,&a.body);
        self.alerts.retain(|a|!matches(a));
        if self.notice.as_ref().is_some_and(matches){self.notice=None;}
        if self.hud_notice.as_ref().is_some_and(matches){self.hud_notice=None;}
        self.pending.retain(|cmd|!matches!(cmd,Cmd::Notify(title,body,_) if belongs(title,body)));
    }
    /// 該來源的所有帳號都已查看。
    pub fn viewed_messenger(&mut self, source: crate::app::Messenger) {
        self.pending_messengers.retain(|u| u.source != source);
        self.clear_messenger_notices(source);
    }
    /// 只清已查看的帳號；無法確認帳號的 QQ 提醒由使用者明確關閉。
    pub fn viewed_client(&mut self, source: crate::app::Messenger, pid: u32) {
        self.pending_messengers
            .retain(|u| !u.viewed_by(source, pid));
        if pid != 0 {
            self.clear_account_notices(source, pid);
        }
        if !source.adapter().keep_unidentified {
            self.clear_account_notices(source, 0);
        }
        if !self.pending_messengers.iter().any(|u| u.source == source) {
            self.clear_messenger_notices(source);
        }
    }
    pub fn confirm_messenger_open(
        &mut self,
        unread: crate::app::Unread,
        foreground: Option<(crate::app::Messenger, u32)>,
    ) -> bool {
        if unread.pid == 0 || foreground != Some((unread.source, unread.pid)) {
            return false;
        }
        self.viewed_client(unread.source, unread.pid);
        true
    }
    pub fn discard_unread(&mut self, unread: crate::app::Unread) {
        if !self.pending_messengers.contains(&unread) {
            return;
        }
        self.pending_messengers.retain(|current| *current != unread);
        self.clear_account_notices(unread.source, unread.pid);
        if !self
            .pending_messengers
            .iter()
            .any(|current| current.source == unread.source)
        {
            self.clear_messenger_notices(unread.source);
        }
    }
    fn clear_account_notices(&mut self, source: crate::app::Messenger, pid: u32) {
        let matches = |notice: &Alert| {
            notice
                .unread
                .is_some_and(|u| u.source == source && u.pid == pid)
        };
        self.alerts.retain(|notice| !matches(notice));
        if self.notice.as_ref().is_some_and(matches) {
            self.notice = None;
            self.notification_bell_until = 0.0;
        }
    }
    fn clear_messenger_notices(&mut self, source: crate::app::Messenger) {
        self.alerts.retain(|a| a.messenger() != Some(source));
        if self
            .notice
            .as_ref()
            .is_some_and(|a| a.messenger() == Some(source))
        {
            self.notice = None;
            self.notification_bell_until = 0.0;
        }
    }
    fn mark_messenger(&mut self, source: crate::app::Messenger, pid: u32, window: isize) {
        if let Some(index) = self
            .pending_messengers
            .iter()
            .position(|u| u.source == source && u.pid == pid)
        {
            if window != 0 && self.pending_messengers[index].window != window {
                self.pending_messengers[index].window = window;
                if source.adapter().capture == crate::messenger::Capture::Taskbar {
                    let unread = self.pending_messengers[index];
                    for alert in self.alerts.iter_mut().chain(self.notice.iter_mut()) {
                        if alert
                            .unread
                            .is_some_and(|old| old.source == source && old.pid == pid)
                        {
                            alert.unread = Some(unread);
                        }
                    }
                } else {
                    self.clear_account_notices(source, pid);
                }
            }
            return;
        }
        self.pending_messengers.push(crate::app::Unread {
            source,
            pid,
            window,
        });
    }
    /// 橫幅「查看」：跳到該來源最近一條未讀的窗口。
    pub(crate) fn latest_unread(&self, source: crate::app::Messenger) -> crate::app::Unread {
        self.pending_messengers
            .iter()
            .rev()
            .find(|u| u.source == source)
            .copied()
            .unwrap_or(crate::app::Unread::of(source))
    }
    pub fn receive_wechat_reminder(&mut self) {
        self.receive_wechat_reminder_from(0, 0);
    }
    /// 微信窗口閃動：`pid` 是該帳號的進程，`window` 是閃動的窗口。
    pub fn receive_wechat_reminder_from(&mut self, pid: u32, window: isize) {
        self.receive_messenger_reminder(crate::app::Unread {
            source: crate::app::Messenger::Wechat,
            pid,
            window,
        });
    }
    pub fn receive_messenger_reminder(&mut self, unread: crate::app::Unread) {
        let client = unread.source;
        if client.adapter().capture != crate::messenger::Capture::Taskbar
            || !client.enabled(&self.settings)
        {
            return;
        }
        let pending = self
            .pending_messengers
            .iter()
            .any(|old| old.source == client && old.pid == unread.pid);
        self.mark_messenger(client, unread.pid, unread.window);
        if pending {
            return;
        }
        self.notification_bell_at = self.t;
        self.notification_bell_until = self.t + 4.5;
        self.enqueue_alert(
            Alert {
                source: AlertSource::Messenger(client),
                title: client.name().into(),
                body: format!("有新訊息 · 請到{}檢視", client.name()),
                page: usize::MAX,
                at: self.t,
                duration: 4.5,
                bluetooth: None,
                meeting: None,
                unread: Some(unread),
            },
            false,
        );
    }
    pub fn set_messenger_status(&mut self, client: crate::app::Messenger, status: String) {
        self.messenger_statuses.insert(client, status);
    }
    pub fn messenger_status(&self, client: crate::app::Messenger) -> &str {
        self.messenger_statuses
            .get(&client)
            .map(String::as_str)
            .unwrap_or("")
    }
    /// 啟動時把通知中心裡還在的未讀補上；點開後仍由 clear 歸零，不會被舊快照復活。
    pub fn seed_social(&mut self, seeds: Vec<crate::system::SocialSeed>) {
        if !self.settings.read_notifications {
            return;
        }
        for seed in seeds {
            self.social.seed(
                seed.social,
                seed.count,
                seed.app_id.as_deref(),
                seed.launch.as_deref(),
            );
        }
        self.social.persist();
    }
    pub fn receive_notification(&mut self, app: String, id: String, body: String) {
        self.receive_notification_from(app, id, body, None);
    }
    pub fn receive_notification_from(
        &mut self,
        app: String,
        id: String,
        body: String,
        unread: Option<crate::app::Unread>,
    ) {
        self.receive_notification_from_with_launch(app, id, body, unread, None);
    }
    /// `launch` 是 Windows 通知原始的深層連結；LINE 會帶 `line://…`，可回到同一個聊天室。
    pub fn receive_notification_from_with_launch(
        &mut self,
        app: String,
        id: String,
        body: String,
        unread: Option<crate::app::Unread>,
        launch: Option<String>,
    ) {
        if !self.settings.read_notifications
            || id.is_empty()
            || !crate::messenger::NotificationFilter::for_settings(&self.settings).allows(&id)
        {
            return;
        }
        if crate::messenger::suppresses_toast(
            &app,
            &id,
            crate::messenger::toast_suppression(&self.settings),
        ) {
            return;
        }
        let social_source = crate::social::detect_relayed(&app, &id, &body);
        let notification_id = id.clone();
        let client = crate::app::Messenger::from_notification_id(&id)
            .filter(|client| client.adapter().capture == crate::messenger::Capture::SystemToast);
        let id = client
            .map(|client| client.adapter().notification_id.to_string())
            .unwrap_or(id);
        // 鬼島自己發出的系統提醒會被通知中心回傳；已在歷史裡的同一內容不再入隊，避免重複顯示。
        if crate::messenger::Messenger::from_notification_id(&id).is_none()
            && self
                .history
                .iter()
                .any(|a| format!("{} · {}", a.title, a.body) == body)
        {
            return;
        }
        if !self.notification_apps.observe(&id, &app) || !self.settings.notifications {
            return;
        }
        if let Some(social) = social_source {
            self.social
                .bump_with_launch(social, &notification_id, launch.as_deref());
            self.social.persist();
        }
        let unread = client.map(|client| {
            unread
                .filter(|u| u.source == client && u.pid != 0)
                .unwrap_or(crate::app::Unread::of(client))
        });
        if let Some(unread) = unread {
            self.mark_messenger(unread.source, unread.pid, unread.window);
        }
        self.notification_bell_at = self.t;
        self.notification_bell_until = self.t + 4.5;
        let title = app.chars().filter(|c| !c.is_control()).take(80).collect();
        let body = if self.settings.notification_preview {
            let mut text = body
                .chars()
                .filter(|c| !c.is_control())
                .take(300)
                .collect::<String>();
            if let Some(code) = crate::otp::extract("", &text, &[]) {
                text = crate::mail_privacy::mask(&text, &code.text);
            }
            text
        } else {
            "收到新通知 · 內容已隱藏".into()
        };
        self.enqueue_alert(
            Alert {
                source: AlertSource::External(id),
                title,
                body,
                page: usize::MAX,
                at: self.t,
                duration: 4.5,
                bluetooth: None,
                meeting: None,
                unread,
            },
            false,
        );
    }
    pub fn reveal_messenger_notice(&mut self, unread: crate::app::Unread) {
        if !unread.source.adapter().dismiss_unread || !self.pending_messengers.contains(&unread) {
            return;
        }
        let body = if unread.pid == 0 {
            "訊息帳號暫無法確認"
        } else {
            "暫時無法開啟對應視窗"
        };
        let source = if unread.source.adapter().capture == crate::messenger::Capture::Taskbar {
            AlertSource::Messenger(unread.source)
        } else {
            AlertSource::External(unread.source.adapter().notification_id.into())
        };
        self.notice = Some(Alert {
            source,
            title: unread.source.name().into(),
            body: body.into(),
            page: usize::MAX,
            at: self.t,
            duration: 4.5,
            bluetooth: None,
            meeting: None,
            unread: Some(unread),
        });
        self.notification_bell_at = self.t;
        self.notification_bell_until = self.t + 4.5;
    }
    pub(crate) fn tick_alerts(&mut self) {
        self.pending_messengers.retain(|u|u.source.enabled(&self.settings) && (u.source.adapter().capture!=crate::messenger::Capture::SystemToast
            || self.notification_apps.items.iter().any(|(key,_,on)|key==u.source.adapter().notification_id && *on)));
        let allowed=|a:&Alert|match &a.source {
            AlertSource::Internal=>!self.fullscreen_hidden || a.health().is_none(),
            AlertSource::MailCode(..)=>!self.fullscreen_hidden,
            AlertSource::Messenger(client)=>client.enabled(&self.settings),
            AlertSource::External(id)=>self.settings.read_notifications && crate::messenger::NotificationFilter::for_settings(&self.settings).allows(id) && self.settings.notifications && !crate::messenger::suppresses_toast(&a.title,id,crate::messenger::toast_suppression(&self.settings)) && self.notification_apps.items.iter().any(|(key,_,on)|key==id && *on),
            #[cfg(windows)]
            AlertSource::Plugin { .. } => self.settings.notifications,
        };
        self.alerts.retain(&allowed);
        if self.notice.as_ref().is_some_and(|a| !allowed(a)) {
            self.notice = None;
        }
        if self.hud_notice.as_ref().is_some_and(|a| !allowed(a)) {
            self.hud_notice = None;
        }
        if self.fullscreen_hidden {
            self.pending.retain(|cmd|!matches!(cmd,Cmd::Notify(title,..)if HealthReminder::ALL.into_iter().any(|kind|title==kind.title())));
        }
        if !self.settings.notification_preview {
            for a in self
                .alerts
                .iter_mut()
                .chain(self.notice.iter_mut())
                .chain(self.hud_notice.iter_mut())
            {
                if matches!(&a.source, AlertSource::External(_)) {
                    a.body = "收到新通知 · 內容已隱藏".into();
                }
            }
        }
        if self
            .notice
            .as_ref()
            .is_some_and(|a| self.t - a.at > a.duration)
        {
            self.notice = None;
        }
        if self.notice.is_none() {
            if let Some(mut a) = self.alerts.pop_front() {
                a.at = self.t;
                self.notice = Some(a);
            }
        }
    }
    pub(crate) fn draw_notice(&mut self, c:&Card,panel:Rect,a:&Alert,_cmds:&mut Vec<Cmd>){
        let nc=c.clipped(0.0,0.0,panel.width(),panel.height());
        #[cfg(windows)]
        if let AlertSource::Plugin { source, id } = &a.source {
            self.draw_plugin_notice_banner(&nc, a, source, id, _cmds);
            return;
        }
        if let AlertSource::MailCode(account,uid)=&a.source {
            if let Some(item)=self.mail.alert_code_item(account,*uid,self.t,self.settings.mail_code_hide_seconds) {
                let code=item.code.as_deref().unwrap_or_default();
                let width=nc.text_w(code,27.0,W::Bold);
                let size=27.0*(126.0/width).min(1.0);
                crate::paint::bell(&nc,22.0,30.0,theme::GRAY,0.0);
                nc.centered_text(135.0,16.0,&nc.fit(&item.from,11.0,W::Regular,126.0),11.0,W::Regular,theme::GRAY,H::Center);
                nc.centered_text(135.0,39.0,code,size,W::Bold,theme::GREEN,H::Center);
                let copy_id=format!("mail-code-copy:{account}:{uid}");
                if notice_button(&nc,204.0,20.0,52.0,"複製",&copy_id) {
                    let code=code.to_owned();
                    self.notice=None;
                    self.code_copied(&code);
                    _cmds.push(Cmd::Copy(code));
                }
            }
            return;
        }
        if let Some(source)=a.messenger(){
            messenger_icon(&nc,22.0,22.0,source);
            nc.circle(31.0,14.0,2.6,Color32::from_rgb(255,72,72));
            nc.centered_text(44.0,14.0,&a.title,11.0,W::Semibold,theme::WHITE,H::Left);
            nc.centered_text(44.0,30.0,&nc.fit(&a.body,10.5,W::Regular,154.0),10.5,W::Regular,theme::GRAY,H::Left);
            if notice_button(&nc,206.0,12.0,52.0,"查看","messenger-view"){_cmds.push(Cmd::ViewMessenger(a.unread.unwrap_or_else(||self.latest_unread(source))));}
            if notice_button(&nc,265.0,12.0,52.0,"關閉","messenger-dismiss"){
                if source.adapter().dismiss_unread{if let Some(unread)=a.unread{self.discard_unread(unread);}}
                self.notice=None;self.notification_bell_until=0.0;
            }
            return;
        }
        if a.source != AlertSource::Internal {
            crate::paint::bell(
                &nc,
                310.0,
                22.0,
                theme::GRAY,
                crate::paint::bell_swing(self.t - a.at),
            );
            nc.circle(314.0, 17.5, 2.0, Color32::from_rgb(255, 72, 72));
            nc.centered_text(
                16.0,
                14.0,
                &nc.fit(&a.title, 11.0, W::Semibold, 198.0),
                11.0,
                W::Semibold,
                theme::WHITE,
                H::Left,
            );
            nc.centered_text(
                16.0,
                30.0,
                &nc.fit(&a.body, 10.5, W::Regular, 198.0),
                10.5,
                W::Regular,
                theme::GRAY,
                H::Left,
            );
            if notice_button(&nc, 226.0, 12.0, 60.0, "關閉", "external-dismiss") {
                self.notice = None;
                self.notification_bell_until = 0.0;
            }
            return;
        }
        if let Some((meeting, _)) = &a.meeting {
            let secondary = Color32::from_gray(174);
            notice_icon(&nc.scaled(34.0, 46.0, 36.0 / 28.0), 34.0, 46.0, a);
            let now = chrono::Local::now();
            let start = chrono::DateTime::from_timestamp(meeting.start, 0)
                .unwrap()
                .with_timezone(&chrono::Local);
            let left = meeting.start - now.timestamp();
            let countdown = if left > 0 {
                format!("{} 分鐘後", (left + 59) / 60)
            } else {
                "正在開始".into()
            };
            let countdown_width = nc.text_w(&countdown, 10.5, W::Semibold) + 16.0;
            let countdown_x = 344.0 - countdown_width;
            nc.centered_text(
                64.0,
                33.0,
                &start.format("%H:%M").to_string(),
                30.0,
                W::Semibold,
                theme::WHITE,
                H::Left,
            );
            nc.fill(
                countdown_x,
                23.0,
                countdown_width,
                20.0,
                10.0,
                Color32::from_gray(28),
            );
            nc.centered_text(
                countdown_x + countdown_width / 2.0,
                33.0,
                &countdown,
                10.5,
                W::Semibold,
                theme::WHITE,
                H::Center,
            );
            let overflow = (nc.text_w(&meeting.title, size::TITLE, W::Semibold) - 280.0).max(0.0);
            let offset = (((self.t - a.at - 1.2).max(0.0) as f32) * 24.0).min(overflow);
            nc.clipped(64.0, 52.0, 280.0, 24.0).centered_text(
                64.0 - offset,
                64.0,
                &meeting.title,
                size::TITLE,
                W::Semibold,
                theme::WHITE,
                H::Left,
            );
            nc.seg((16.0, 84.0), (344.0, 84.0), 0.6, Color32::from_gray(36));
            let date = format!(
                "{}{}",
                if start.date_naive() == now.date_naive() {
                    "今天 · "
                } else {
                    ""
                },
                start.format("%-m月%-d日")
            );
            nc.centered_text(16.0, 110.0, &date, 11.0, W::Regular, secondary, H::Left);
            let (dismiss, dc) = crate::paint::capsule(
                &nc,
                188.0,
                98.0,
                72.0,
                24.0,
                "meeting-dismiss",
                Color32::from_gray(32),
                Color32::from_gray(48),
            );
            dc.centered_text(
                224.0,
                110.0,
                "關閉",
                10.5,
                W::Semibold,
                theme::WHITE,
                H::Center,
            );
            if dismiss.on_hover_text("關閉本次提醒，行程仍保留").clicked() {
                self.notice = None;
                return;
            }
            let (button, bc) = crate::paint::capsule(
                &nc,
                272.0,
                98.0,
                72.0,
                24.0,
                "meeting-action",
                Color32::from_gray(42),
                Color32::from_gray(56),
            );
            bc.centered_text(
                308.0,
                110.0,
                "檢視",
                10.5,
                W::Semibold,
                theme::WHITE,
                H::Center,
            );
            if button.clicked() {
                self.notice = None;
                self.open_calendar(Some(meeting.clone()));
            }
            return;
        }
        if a.is_hud(){
            if a.title=="藍牙裝置" {
                let (name,status,battery)=bluetooth_notice_content(&a.body);
                let badge=if status=="已連線"{Color32::from_rgb(10,132,255)}else if let Some(level)=battery.filter(|_|status!="已斷開"){crate::bluetooth_ui::level_color(level)}else{Color32::from_gray(95)};
                nc.circle(26.0,24.0,14.0,badge);
                let kind=a.bluetooth.map(|(kind,_)|kind).unwrap_or(crate::bt::DeviceKind::Other);
                device_icon(&nc,kind,17.0,15.0,18.0);
                let right=HUD_W-14.0;
                let name_end=if let Some(level)=battery {
                    let pct=format!("{level}%");
                    let pct_width=nc.text_w(&pct,10.5,W::Semibold);
                    let x=right-pct_width-22.0;
                    nc.stroke(x,20.0,15.0,8.0,2.0,1.0,Color32::from_gray(175));
                    nc.fill(x+15.5,22.5,2.0,3.0,0.8,Color32::from_gray(175));
                    let fill=11.0*level as f32/100.0;
                    if fill>0.0 {nc.fill(x+2.0,22.0,fill,4.0,1.0,crate::bluetooth_ui::level_color(level));}
                    nc.centered_text(right,24.0,&pct,10.5,W::Semibold,theme::WHITE,H::Right);
                    x-8.0
                } else {right};
                nc.centered_text(49.0,16.0,&nc.fit(name,11.0,W::Semibold,(name_end-49.0).max(20.0)),11.0,W::Semibold,theme::WHITE,H::Left);
                nc.centered_text(49.0,32.0,status,10.5,W::Regular,theme::GRAY,H::Left);
                return;
            }
            match a.title.as_str() {
                "系統音量" => {
                    nc.poly(
                        &[
                            (17.0, 16.0),
                            (21.0, 16.0),
                            (26.0, 12.0),
                            (26.0, 26.0),
                            (21.0, 22.0),
                            (17.0, 22.0),
                        ],
                        theme::WHITE,
                    );
                    if self.volume_ui.is_some_and(|(_, muted)| muted) {
                        nc.seg((29.0, 16.0), (34.0, 22.0), 1.4, theme::WHITE);
                        nc.seg((34.0, 16.0), (29.0, 22.0), 1.4, theme::WHITE);
                    } else {
                        nc.arc(26.0, 19.0, 5.0, -0.8, 0.8, 1.5, theme::WHITE);
                        nc.arc(26.0, 19.0, 8.0, -0.7, 0.7, 1.3, theme::WHITE);
                    }
                }
                _ => notice_icon(&nc, 24.0, 19.0, a),
            }
            if a.title == "系統音量" {
                self.draw_volume_hud(&nc);
            } else {
                let (title, body) = if a.title == "鍵盤輸入語言" {
                    ("輸入語言", a.body.as_str())
                } else if a.title == "已切換電池供電" {
                    ("電池供電", a.body.as_str())
                } else if a.title == "網路連線" {
                    a.body
                        .split_once(" · ")
                        .unwrap_or(("網路", a.body.as_str()))
                } else {
                    (a.title.as_str(), a.body.as_str())
                };
                nc.centered_text(
                    48.0,
                    19.0,
                    &nc.fit(title, 11.0, W::Semibold, 90.0),
                    11.0,
                    W::Semibold,
                    theme::WHITE,
                    H::Left,
                );
                nc.centered_text(
                    HUD_W - 16.0,
                    19.0,
                    &nc.fit(body, 10.5, W::Regular, 92.0),
                    10.5,
                    W::Regular,
                    if a.title == "大寫鎖定" && a.body == "已開啟"
                        || a.title == "數字鍵盤" && a.body == "已開啟"
                        || a.title == "網路連線" && a.body.contains("已連線")
                    {
                        theme::GREEN
                    } else {
                        theme::GRAY
                    },
                    H::Right,
                );
            }
        } else {
            let is_sit = a.health() == Some(HealthReminder::Sit);
            let is_water = a.health() == Some(HealthReminder::Water);
            let is_focus = a.page == 1;
            let is_power = a.title == "電量偏低";
            notice_icon(&nc, 22.0, 22.0, a);
            let max = if is_water || is_sit { 125.0 } else { 195.0 };
            nc.centered_text(
                43.0,
                14.0,
                &nc.fit(&a.title, 11.0, W::Semibold, max),
                11.0,
                W::Semibold,
                theme::WHITE,
                H::Left,
            );
            nc.centered_text(
                43.0,
                30.0,
                &nc.fit(&a.body, 10.5, W::Regular, max),
                10.5,
                W::Regular,
                theme::GRAY,
                H::Left,
            );
            if is_sit {
                if notice_button(&nc, 181.0, 12.0, 62.0, "知道了", "sit-confirm-read") {
                    self.confirm_health_reminder(HealthReminder::Sit);
                }
                if notice_button(&nc, 250.0, 12.0, 68.0, "起身了", "sit-confirm") {
                    self.notice = None;
                    self.stood_up();
                }
            } else if is_water {
                if notice_button(&nc, 181.0, 12.0, 62.0, "知道了", "water-confirm-read") {
                    self.confirm_health_reminder(HealthReminder::Water);
                }
                if notice_button(&nc, 250.0, 12.0, 68.0, "喝一口", "water-confirm") {
                    self.notice = None;
                    self.drink_water();
                }
            } else {
                let label = if is_focus {
                    "開始"
                } else if is_power {
                    "知道了"
                } else {
                    "檢視"
                };
                if notice_button(&nc, 257.0, 12.0, 60.0, label, "notice-action") {
                    self.notice = None;
                    if is_power {
                        return;
                    }
                    if is_focus {
                        self.pomo.running = true;
                    } else if a.page < 6 {
                        self.show_page(a.page);
                        if a.page == 4 {
                            let order = self.todos.display_order(&self.today);
                            if let Some(index) = order.iter().position(|&i| {
                                self.todos.items[i].text == a.body
                                    && !self.todos.items[i].done_for(&self.today)
                            }) {
                                self.todo_first = index.min(order.len().saturating_sub(3));
                            }
                        }
                    } else {
                        self.settings_tab = 6;
                        self.open_settings();
                    }
                }
            }
            if let Some(kind) = a.health() {
                if nc
                    .hit(0.0, 0.0, 175.0, 44.0, "health-notice-confirm")
                    .clicked()
                {
                    self.confirm_health_reminder(kind);
                }
            }
        }
    }
    /// v0.5.4：可交互音量條。樣式參考 oyenza-studio（MIT）的 HUD 音量條：
    /// 圓角膠囊、填充緩動、拖動時加粗並出現手柄；拖動/滾輪改音量，點喇叭靜音。
    pub(crate) fn draw_volume_hud(&mut self, nc: &Card) {
        let (v, muted) = self.volume_ui.unwrap_or((0, false));
        let v = v.min(100);
        // 喇叭：點擊靜音 / 恢復
        let icon = nc.hit(12.0, 9.0, 28.0, 22.0, "volume-mute");
        if icon.clicked() && nc.live {
            self.volume_ui = Some((v, !muted));
            self.volume_quiet_until = self.t + 0.9;
            self.pending.push(Cmd::ToggleMute);
        }
        // 滑條：拖動 / 點擊定位；滾輪微調
        let track = nc.hit_drag(HUD_TRACK_X, 8.0, HUD_TRACK_W, 24.0, "volume-track");
        let mut want: Option<u32> = None;
        if track.dragged() || track.clicked() {
            if let Some(p) = track.interact_pointer_pos() {
                let x = nc.to_local(p).x;
                want = Some(
                    (((x - HUD_TRACK_X) / HUD_TRACK_W) * 100.0)
                        .clamp(0.0, 100.0)
                        .round() as u32,
                );
            }
        } else if nc.live && track.hovered() {
            let dy = nc.ui.ctx().input(|i| i.raw_scroll_delta.y);
            if dy.abs() > 0.5 {
                want = Some(((v as f32 + (dy / 120.0) * 4.0).clamp(0.0, 100.0)).round() as u32);
            }
        }
        if let Some(nv) = want {
            if self.volume_ui.map(|(o, _)| o) != Some(nv) || muted {
                self.volume_ui = Some((nv, false));
                self.volume_quiet_until = self.t + 0.9;
                self.pending.push(Cmd::SetVolume(nv));
            }
        }
        let (v, muted) = self.volume_ui.unwrap_or((v, muted));
        let pressed = track.dragged()
            || track.is_pointer_button_down_on()
            || icon.is_pointer_button_down_on();
        let target = if muted { 0.0 } else { v as f32 / 100.0 };
        self.volume_a = crate::anim::approach(
            self.volume_a,
            target,
            if pressed { 34.0 } else { 13.0 },
            self.dt,
        );
        let h = if pressed { 7.0 } else { 5.0 };
        let y = 19.0 - h / 2.0;
        nc.fill(
            HUD_TRACK_X,
            y,
            HUD_TRACK_W,
            h,
            h / 2.0,
            Color32::from_white_alpha(if pressed { 46 } else { 34 }),
        );
        if self.volume_a > 0.0 {
            let w = (HUD_TRACK_W * self.volume_a).max(h);
            let col = if muted {
                Color32::from_gray(150)
            } else {
                theme::WHITE
            };
            nc.fill(HUD_TRACK_X, y, w, h, h / 2.0, col);
            nc.fill(
                HUD_TRACK_X,
                y + 0.8,
                (w - 1.2).max(0.0),
                h * 0.4,
                h * 0.2,
                Color32::from_white_alpha(64),
            );
            if pressed {
                nc.circle(HUD_TRACK_X + w - h / 2.0, y + h / 2.0, h * 0.95, col);
            }
        }
        let label = if muted {
            "靜音".to_string()
        } else {
            format!("{v}%")
        };
        nc.centered_text(
            HUD_W - 26.0,
            19.0,
            &label,
            10.5,
            if pressed { W::Semibold } else { W::Regular },
            if muted { theme::GRAY } else { theme::WHITE },
            H::Center,
        );
    }

    pub(crate) fn drink_water(&mut self) {
        let hour = self.current_hour;
        let amount = self.settings.serving_ml;
        let was = self.water.percent();
        let before = self.water.drunk_ml;
        self.water.drink(amount, hour);
        let amount = self.water.drunk_ml.saturating_sub(before);
        self.last_drink = Some((amount, hour));
        self.confirm_health_reminder(HealthReminder::Water);
        self.pet.react(PetState::Satisfied);
        self.say(format!("已記錄 {amount} ml"));
        if was < 100.0 && self.water.percent() >= 100.0 {
            self.alert("今日飲水目標達成", "做得不錯，繼續保持", 2, true);
        }
    }
}

/// 將本機提醒時間規範化，拒絕不存在的日期與超出範圍的時分。
pub(crate) fn parse_due(value: &str, today: &str) -> Option<String> {
    let value = value.trim();
    let full = if value.len() == 5 {
        format!("{today} {value}")
    } else {
        value.to_string()
    };
    if !full.is_ascii() || full.len() != 16 {
        return None;
    }
    let b = full.as_bytes();
    if b[4] != b'-' || b[7] != b'-' || b[10] != b' ' || b[13] != b':' {
        return None;
    }
    let year = full[0..4].parse::<u32>().ok()?;
    let month = full[5..7].parse::<usize>().ok()?;
    let day = full[8..10].parse::<u32>().ok()?;
    let hour = full[11..13].parse::<u32>().ok()?;
    let minute = full[14..16].parse::<u32>().ok()?;
    if !(2000..=9999).contains(&year) || !(1..=12).contains(&month) || hour > 23 || minute > 59 {
        return None;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if day == 0 || day > days[month - 1] {
        return None;
    }
    Some(full)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn island_size_drag_commits_during_drag() {
        let ctx = egui::Context::default();
        let mut value = 100;
        let pos = egui::pos2(30.0, 30.0);
        let end = egui::pos2(110.0, 30.0);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let frames = [
            vec![],
            vec![egui::Event::PointerMoved(pos)],
            vec![button(pos, true)],
            vec![egui::Event::PointerMoved(end)],
            vec![button(end, false)],
        ];
        for (i, events) in frames.into_iter().enumerate() {
            let _ = ctx.run(
                egui::RawInput {
                    events,
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(320.0, 80.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::new().inner_margin(20))
                        .show(ctx, |ui| {
                            ui.spacing_mut().slider_width = 180.0;
                            island_size_slider(ui, &mut value);
                        });
                },
            );
            if i < 2 {
                assert_eq!(value, 100);
            } else {
                assert_ne!(value, 100);
            }
        }
    }
    #[test]
    fn number_stepper_clicks_follow_steps_and_limits() {
        for (initial, step, x, expected) in [
            (5u32, 1, 127.0, 6),
            (5, 1, 13.0, 4),
            (0, 1, 13.0, 0),
            (10, 1, 127.0, 10),
            (9, 3, 127.0, 10),
            (1, 3, 13.0, 0),
        ] {
            let ctx = egui::Context::default();
            let mut value = initial;
            let pos = egui::pos2(x + 20.0, 34.0);
            let events = [
                vec![],
                vec![egui::Event::PointerMoved(pos)],
                vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                }],
                vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                }],
            ];
            for events in events {
                let raw = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(200.0, 80.0),
                    )),
                    events,
                    ..Default::default()
                };
                let _ = ctx.run(raw, |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::new().inner_margin(20))
                        .show(ctx, |ui| {
                            ui.horizontal(|ui| {
                                number_stepper(ui, &mut value, 0, 10, "", step, 76.0);
                            });
                        });
                });
            }
            assert_eq!(value, expected, "初值 {initial}，步長 {step}，點選位置 {x}");
        }
    }

    #[test]
    fn number_stepper_fits_with_settings_button_padding() {
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(340.0, 240.0),
            )),
            ..Default::default()
        };
        let _ = ctx.run(raw, |ctx| {
            egui::CentralPanel::default()
                .frame(egui::Frame::new())
                .show(ctx, |ui| {
                    ui.spacing_mut().button_padding = egui::vec2(10.0, 6.0);
                    for mut value in [0u32, 25, 180] {
                        let row = ui.horizontal(|ui| {
                            number_stepper(ui, &mut value, 0, 180, " 分鐘", 1, 76.0);
                        });
                        assert!(
                            row.response.rect.width() <= 140.1,
                            "數值控制元件超出預留寬度：{}",
                            row.response.rect.width()
                        );
                    }
                    for mut value in [25u32, 5, 15] {
                        setting_number(ui, "計時時長", &mut value, 1, 180, " 分鐘", 1.0);
                    }
                    assert!(ui.min_rect().right() <= 340.1, "連續數值行不應撐寬卡片");
                });
        });
    }

    #[test]
    fn settings_groups_keep_every_page_reachable() {
        let groups = ["通用", "音樂", "專注與健康", "專注與健康", "待辦與置物", "待辦與置物", "訊息提醒", "信箱", "行程", "關於", "頁面管理", "AI 額度", "天氣", "外觀", "位置", "系統性能", "無線裝置"];
        for (tab, expected) in groups.iter().enumerate() {
            let matches: Vec<_> = SETTINGS_GROUPS
                .iter()
                .filter(|(_, start, end, _)| (*start..=*end).contains(&tab))
                .collect();
            assert_eq!(matches.len(), 1, "頁面 {tab} 必須且只能屬於一個分組");
            assert_eq!(matches[0].0, *expected);
        }
    }

    #[test]
    fn bluetooth_notice_uses_real_connection_and_battery_data() {
        assert_eq!(
            bluetooth_notice_content("OpenMove by Shokz · 電量 80%"),
            ("OpenMove by Shokz", "已連線", Some(80))
        );
        assert_eq!(bluetooth_notice_content("耳機"), ("耳機", "已連線", None));
        assert_eq!(
            bluetooth_notice_content("已斷開連線"),
            ("藍牙裝置", "已斷開", None)
        );
        assert_eq!(
            bluetooth_notice_content("青龍鍵盤 · 已斷開"),
            ("青龍鍵盤", "已斷開", None)
        );
        assert_eq!(
            bluetooth_notice_content("耳機 · 電量 101%"),
            ("耳機 · 電量 101%", "已連線", None)
        );
    }

    #[test]
    fn device_icons_render_as_visible_svg() {
        for kind in ALL_DEVICE_KINDS {
            let egui::ImageSource::Bytes{bytes,..}=device_icon_source(kind) else {panic!("{kind:?}")};
            let image=egui_extras::image::load_svg_bytes(&bytes).unwrap();
            assert!(image.pixels.iter().any(|p|p.a()>0),"{kind:?}");
        }
    }

    /// 圖標要在圓環裡視覺居中：按 96px 渲染後，不透明部分的外框中心偏離不超過 0.25 個圖標單位。
    #[test]
    fn device_icons_are_visually_centered() {
        let mut off=vec![];
        for kind in ALL_DEVICE_KINDS {
            let egui::ImageSource::Bytes{uri,bytes}=device_icon_source(kind) else {panic!()};
            let image=egui_extras::image::load_svg_bytes_with_size(&bytes,Some(egui::load::SizeHint::Size(96,96))).unwrap();
            let [w,h]=image.size;let (mut x0,mut y0,mut x1,mut y1)=(w,h,0,0);
            for y in 0..h {for x in 0..w {if image.pixels[y*w+x].a()>40 {x0=x0.min(x);y0=y0.min(y);x1=x1.max(x);y1=y1.max(y);}}}
            let dx=(w as f32/2.0-(x0+x1+1) as f32/2.0)*24.0/w as f32;let dy=(h as f32/2.0-(y0+y1+1) as f32/2.0)*24.0/h as f32;
            println!("ICON-OFFSET {uri} {dx:.2} {dy:.2}");
            if dx.abs()>0.25||dy.abs()>0.25 {off.push(format!("{kind:?} 偏移 ({dx:.2},{dy:.2})"));}
        }
        assert!(off.is_empty(),"圖標沒有居中：{off:?}");
    }

    #[test]
    fn due_time_checks_calendar_and_format() {
        assert_eq!(
            parse_due("09:30", "2026-10-29").as_deref(),
            Some("2026-10-29 09:30")
        );
        assert!(parse_due("2026-02-29 12:00", "").is_none());
        assert!(parse_due("2028-02-29 12:00", "").is_some());
        for value in [
            "24:00",
            "12:60",
            "明天中午",
            "2026-13-01 10:00",
            "2026-00-01 10:00",
        ] {
            assert!(parse_due(value, "2026-10-29").is_none());
        }
    }
}

pub(crate) struct TaskDraft {
    index: usize,
    original: String,
    text: String,
    due: String,
    error: String,
    focused: bool,
    reminder_only: bool,
    pub(crate) repeat: u8,
    pub(crate) picker: crate::task_picker::ReminderPicker,
}
impl App {
    pub(crate) fn open_task_dialog(&mut self, index: usize) {
        self.todo_choosing = false;
        if let Some(task) = self.todos.items.get(index) {
            let draft = TaskDraft {
                index,
                original: task.text.clone(),
                text: task.text.clone(),
                due: task.due.clone().unwrap_or_default(),
                error: String::new(),
                focused: false,
                reminder_only: false,
                repeat: task.repeat,
                picker: crate::task_picker::ReminderPicker::new(task.due.as_deref()),
            };
            self.show_page(4);
            self.task_dialog = Some(draft);
        }
    }
    pub(crate) fn open_task_reminder(&mut self, index: usize) {
        self.open_task_dialog(index);
        if let Some(d) = self.task_dialog.as_mut() {
            d.reminder_only = true;
            d.picker.show();
        }
    }
    pub(crate) fn draw_task_editor(&mut self, c: &Card) {
        if self
            .task_dialog
            .as_ref()
            .is_some_and(|draft| draft.picker.open)
        {
            self.draw_task_reminder(c);
            return;
        }
        let reminder_only = self.task_dialog.as_ref().is_some_and(|d| d.reminder_only);
        c.text(
            15.0,
            25.0,
            if reminder_only {
                "設定提醒"
            } else {
                "編輯待辦"
            },
            size::TITLE,
            W::Bold,
            theme::WHITE,
            H::Left,
        );
        c.text(
            43.0,
            48.0,
            if reminder_only { "待辦" } else { "內容" },
            10.5,
            W::Regular,
            theme::GRAY,
            H::Left,
        );
        c.fill(91.0, 33.0, 275.0, 20.0, 6.0, theme::CARD);
        if let Some(d) = self.task_dialog.as_ref().filter(|d| d.reminder_only) {
            c.text(
                97.0,
                47.0,
                &c.fit(&d.text, 11.5, W::Regular, 260.0),
                11.5,
                W::Regular,
                theme::WHITE,
                H::Left,
            );
        }
        c.text(43.0, 72.0, "提醒", 10.5, W::Regular, theme::GRAY, H::Left);
        let time = self
            .task_dialog
            .as_ref()
            .map(|d| {
                if d.due.is_empty() {
                    "點選選擇時間".into()
                } else if d.repeat != 0 {
                    format!(
                        "{} {}",
                        crate::model::repeat_label(d.repeat),
                        d.due.get(11..16).unwrap_or(&d.due)
                    )
                } else {
                    d.due.clone()
                }
            })
            .unwrap_or_default();
        if setting_button(c, 91.0, 58.0, 275.0, &time, false, "task-select-reminder") {
            if let Some(d) = self.task_dialog.as_mut() {
                d.picker.show();
            }
        }
        // 每週重複：七個星期按鈕，全不選即一次性待辦。
        c.text(43.0, 95.0, "重複", 10.5, W::Regular, theme::GRAY, H::Left);
        self.task_repeat_chips(c, 91.0, 81.0, 25.0, 19.0, 28.0);
        let repeat = self.task_dialog.as_ref().map(|d| d.repeat).unwrap_or(0);
        let quick = if repeat == crate::model::REPEAT_ALL {
            "不重複"
        } else {
            "每天"
        };
        if setting_button(c, 289.0, 81.0, 77.0, quick, false, "task-repeat-all") {
            if let Some(d) = self.task_dialog.as_mut() {
                d.repeat = if d.repeat == crate::model::REPEAT_ALL {
                    0
                } else {
                    crate::model::REPEAT_ALL
                };
            }
        }
        if let Some(d) = &self.task_dialog {
            let summary = if d.repeat != 0 {
                crate::model::repeat_label(d.repeat)
            } else {
                "只做一次".into()
            };
            let hint = if d.error.is_empty() {
                summary.as_str()
            } else {
                &d.error
            };
            c.text(
                43.0,
                117.0,
                &c.fit(
                    hint,
                    10.5,
                    W::Regular,
                    if reminder_only { 140.0 } else { 154.0 },
                ),
                10.5,
                if d.error.is_empty() {
                    W::Regular
                } else {
                    W::Semibold
                },
                if d.error.is_empty() {
                    theme::GRAY
                } else {
                    theme::RED
                },
                H::Left,
            );
        }
        if reminder_only {
            let has_reminder = self
                .task_dialog
                .as_ref()
                .and_then(|d| self.todos.items.get(d.index))
                .is_some_and(|task| task.due.is_some());
            let skip = if has_reminder {
                "取消提醒"
            } else if repeat != 0 {
                "只設重複"
            } else {
                "暫不提醒"
            };
            if setting_button(c, 188.0, 104.0, 88.0, skip, false, "task-reminder-skip") {
                if let Some(d) = self.task_dialog.as_mut() {
                    d.due.clear();
                }
                self.save_task_editor();
            }
        } else if setting_button(c, 202.0, 104.0, 74.0, "取消", false, "task-cancel") {
            self.task_dialog = None;
        }
        if setting_button(
            c,
            286.0,
            104.0,
            80.0,
            if reminder_only {
                "選擇時間"
            } else {
                "儲存"
            },
            true,
            "task-save",
        ) {
            if reminder_only {
                if let Some(d) = self.task_dialog.as_mut() {
                    d.picker.show();
                }
            } else {
                self.save_task_editor();
            }
        }
    }
    pub(crate) fn task_editor_inputs(
        &mut self,
        ui: &mut egui::Ui,
        origin: egui::Pos2,
        cmds: &mut Vec<Cmd>,
    ) {
        let Some(d) = self.task_dialog.as_mut() else {
            return;
        };
        if d.reminder_only || d.picker.open {
            return;
        }
        let name = ui.put(
            Rect::from_min_size(origin + egui::vec2(97.0, 35.0), egui::vec2(260.0, 17.0)),
            egui::TextEdit::singleline(&mut d.text)
                .vertical_align(egui::Align::Center)
                .id(egui::Id::new("task-name-edit"))
                .frame(false)
                .font(theme::font(11.5, W::Regular)),
        );
        let first_focus = !d.focused;
        if first_focus {
            name.request_focus();
            d.focused = true;
            cmds.push(Cmd::Focus);
        }
        // 新建內容與下一步共用這一幀，避免同一次回車又提交了空提醒。
        if !first_focus && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            self.save_task_editor();
        }
    }
    /// 七個星期按鈕（一…日），點一下切換；全不選即一次性待辦。
    fn task_repeat_chips(&mut self, c: &Card, x: f32, y: f32, w: f32, h: f32, step: f32) {
        let repeat = self.task_dialog.as_ref().map(|d| d.repeat).unwrap_or(0);
        for (day, name) in crate::model::WEEKDAY_NAMES.iter().enumerate() {
            let on = repeat & (1 << day) != 0;
            let cx = x + day as f32 * step;
            let (r, b) = crate::paint::capsule(
                c,
                cx,
                y,
                w,
                h,
                &format!("task-repeat-{day}"),
                if on {
                    Color32::from_rgb(39, 94, 150)
                } else {
                    theme::BTN
                },
                theme::BTN_HOVER,
            );
            b.centered_text(
                cx + w / 2.0,
                y + h / 2.0,
                name,
                if h < 18.0 { 9.5 } else { 10.5 },
                W::Semibold,
                if on { theme::WHITE } else { theme::GRAY },
                H::Center,
            );
            if r.on_hover_text(format!("每週{name}")).clicked() {
                if let Some(d) = self.task_dialog.as_mut() {
                    d.repeat ^= 1 << day;
                    d.error.clear();
                }
            }
        }
    }
    fn draw_task_reminder(&mut self, c: &Card) {
        use crate::task_picker::PickerAction;
        let Some(d) = self.task_dialog.as_mut().filter(|d| d.picker.open) else {
            return;
        };
        let has_reminder = self
            .todos
            .items
            .get(d.index)
            .is_some_and(|task| task.due.is_some());
        c.text(
            15.0,
            25.0,
            "待辦",
            size::TITLE,
            W::Bold,
            theme::WHITE,
            H::Left,
        );
        c.text(
            44.0,
            25.0,
            "清單",
            size::TITLE,
            W::Bold,
            theme::GREEN,
            H::Left,
        );
        let (done, total) = self.todos.progress(&self.today);
        c.text(
            42.0,
            61.0,
            &format!("{done}/{total}"),
            29.0,
            W::Bold,
            theme::WHITE,
            H::Left,
        );
        // 左側：每週重複。選了星期幾後，提醒只按時間、在這些日子到點提醒。
        d.picker.repeat_note = Some(if d.repeat != 0 {
            format!("{} · 到點提醒", crate::model::repeat_label(d.repeat))
        } else {
            String::new()
        });
        let action = d.picker.draw(c, &d.text, has_reminder);
        let focus = std::mem::take(&mut d.picker.focus);
        self.task_repeat_chips(c, 41.0, 84.0, 17.0, 16.0, 19.5);
        let Some(d) = self.task_dialog.as_mut() else {
            return;
        };
        if focus {
            self.pending.push(Cmd::Focus);
        }
        match action {
            Some(PickerAction::Save) => {
                d.due = d.picker.due();
                d.picker.open = false;
                if d.reminder_only {
                    self.save_task_editor();
                }
            }
            Some(PickerAction::Skip) => {
                d.due.clear();
                d.picker.open = false;
                if d.reminder_only {
                    self.save_task_editor();
                }
            }
            Some(PickerAction::Cancel) => {
                d.picker.open = false;
                if d.reminder_only {
                    self.task_dialog = None;
                }
            }
            None => {}
        }
    }
    fn save_task_editor(&mut self) {
        let Some(mut d) = self.task_dialog.take() else {
            return;
        };
        let due = if d.due.trim().is_empty() {
            Some(None)
        } else {
            parse_due(&d.due, &self.today).map(Some)
        };
        if d.text.trim().is_empty() {
            d.error = "請填寫任務內容".into();
        } else if due.is_none() {
            d.error = "時間格式應為 18:30 或完整日期時間".into();
        } else if self
            .todos
            .items
            .get(d.index)
            .is_none_or(|t| t.text != d.original)
        {
            d.error = "任務已變化，請取消後重新開啟".into();
        } else {
            let item = &mut self.todos.items[d.index];
            if item.repeat != d.repeat {
                item.done_on = None;
            }
            item.text = d.text.trim().into();
            item.due = due.unwrap();
            item.reminded = false;
            item.reminded_on = None;
            item.repeat = d.repeat;
            self.todos.save();
            self.say("待辦已儲存");
            return;
        }
        self.task_dialog = Some(d);
    }
}

fn notice_button(c:&Card,x:f32,y:f32,w:f32,text:&str,id:&str)->bool {
    let Some(c)=c.edge_control(x,y,w,20.0) else {return false};
    let (r,b)=crate::paint::capsule(&c,x,y,w,20.0,id,Color32::from_gray(35),theme::BTN_HOVER);
    b.centered_text(x+w/2.0,y+10.0,text,10.5,W::Semibold,theme::WHITE,H::Center);
    r.clicked()
}
fn setting_button(c: &Card, x: f32, y: f32, w: f32, text: &str, on: bool, id: &str) -> bool {
    let (r, b) = crate::paint::capsule(
        c,
        x,
        y,
        w,
        19.0,
        id,
        if on {
            Color32::from_rgb(39, 94, 150)
        } else {
            theme::BTN
        },
        theme::BTN_HOVER,
    );
    b.text(
        x + w / 2.0,
        y + 13.3,
        text,
        10.5,
        W::Semibold,
        theme::WHITE,
        H::Center,
    );
    r.clicked()
}
fn hhmm(m: u32) -> String {
    format!("{:02}:{:02}", m / 60 % 24, m % 60)
}
/// 島內小開關（與健康頁右上開關同款）。
fn island_switch(c: &Card, x: f32, y: f32, on: bool, id: &str) -> bool {
    let r = c.hit(x - 2.0, y - 2.0, 38.0, 24.0, id);
    let (hv, _) = c.hover_t(&r);
    let track = if on {
        theme::SIT_GREEN
    } else {
        Color32::from_rgb(58, 58, 62)
    };
    c.fill(
        x,
        y,
        34.0,
        20.0,
        10.0,
        crate::paint::mix(track, theme::WHITE, hv * 0.08),
    );
    c.circle(
        if on { x + 24.0 } else { x + 10.0 },
        y + 10.0,
        8.0,
        theme::WHITE,
    );
    r.clicked()
}
impl App {
    fn health_time_summary(&self) -> String {
        let s = &self.settings;
        let hours = format!("{:02}:00–{:02}:00", s.water_start, s.water_end);
        if s.lunch_rest {
            format!(
                "{hours} · 午休 {}–{}",
                hhmm(s.lunch_start),
                hhmm(s.lunch_end)
            )
        } else {
            hours
        }
    }
    /// 「提醒時段」二級頁：提醒時段起止、午休開關、午休起止（15 分鐘一檔），喝水和久坐共用。
    fn health_time_page(&mut self, c: &Card) {
        let row_y = |i: usize| 38.0 + i as f32 * 22.0;
        for i in 1..4 {
            c.fill(
                44.0,
                row_y(i) - 2.0,
                326.0,
                1.0,
                0.5,
                Color32::from_rgb(28, 28, 31),
            );
        }
        let y = row_y(0);
        c.text(
            44.0,
            y + 13.0,
            "提醒時段",
            10.5,
            W::Regular,
            theme::GRAY,
            H::Left,
        );
        c.text(
            250.0,
            y + 13.0,
            "至",
            10.5,
            W::Regular,
            theme::GRAY,
            H::Center,
        );
        for i in 0..2 {
            let hour = if i == 0 {
                &mut self.settings.water_start
            } else {
                &mut self.settings.water_end
            };
            let value = egui::DragValue::new(hour)
                .range(0..=if i == 0 { 23 } else { 24 })
                .custom_formatter(|v, _| format!("{:02}:00", v as u32))
                .custom_parser(|t| {
                    let t = t.trim();
                    t.strip_suffix(":00")
                        .or_else(|| t.strip_suffix("：00"))
                        .unwrap_or(t)
                        .parse::<u32>()
                        .ok()
                        .map(|v| v as f64)
                });
            let (d, field) = stepper(
                c,
                if i == 0 { 238.0 } else { 370.0 },
                y,
                value,
                &format!("hour-{i}"),
            );
            if field.gained_focus() {
                self.pending.push(Cmd::Focus);
            }
            self.settings_editing |= field.has_focus();
            if d != 0 {
                let hour = if i == 0 {
                    &mut self.settings.water_start
                } else {
                    &mut self.settings.water_end
                };
                *hour = (*hour as i32 + d).rem_euclid(24) as u32;
            }
        }
        let y = row_y(1);
        c.text(
            44.0,
            y + 13.0,
            "午休不提醒",
            10.5,
            W::Regular,
            theme::GRAY,
            H::Left,
        );
        if island_switch(c, 336.0, y - 0.5, self.settings.lunch_rest, "lunch-rest") {
            self.settings.lunch_rest = !self.settings.lunch_rest;
        }
        let y = row_y(2);
        let on = self.settings.lunch_rest;
        let lc = c.faded(if on { 1.0 } else { 0.35 });
        let lc = if on { lc } else { lc.ghost() };
        lc.text(
            44.0,
            y + 13.0,
            "午休時間",
            10.5,
            W::Regular,
            theme::GRAY,
            H::Left,
        );
        lc.text(
            250.0,
            y + 13.0,
            "至",
            10.5,
            W::Regular,
            theme::GRAY,
            H::Center,
        );
        for i in 0..2 {
            let minute = if i == 0 {
                &mut self.settings.lunch_start
            } else {
                &mut self.settings.lunch_end
            };
            let value = egui::DragValue::new(minute)
                .range(0..=1439)
                .speed(1.0)
                .custom_formatter(|v, _| hhmm(v as u32))
                .custom_parser(|t| {
                    let (h, m) = t.trim().split_once([':', '：'])?;
                    Some(
                        (h.parse::<u32>().ok()?.min(23) * 60 + m.parse::<u32>().ok()?.min(59))
                            as f64,
                    )
                });
            let (d, field) = stepper(
                &lc,
                if i == 0 { 238.0 } else { 370.0 },
                y,
                value,
                &format!("lunch-{i}"),
            );
            if field.gained_focus() {
                self.pending.push(Cmd::Focus);
            }
            self.settings_editing |= field.has_focus();
            if d != 0 {
                let minute = if i == 0 {
                    &mut self.settings.lunch_start
                } else {
                    &mut self.settings.lunch_end
                };
                *minute = (*minute as i32 + d * 15).rem_euclid(1440) as u32;
            }
        }
        c.text(
            44.0,
            row_y(3) + 13.0,
            "喝水和久坐提醒都只在提醒時段內、午休以外彈出",
            10.5,
            W::Regular,
            theme::GRAY,
            H::Left,
        );
    }
}
fn island_number_field(
    c: &Card,
    rect: Rect,
    value: egui::DragValue<'_>,
    id: impl std::hash::Hash,
) -> egui::Response {
    let mut ui = egui::Ui::new(
        c.ui.ctx().clone(),
        egui::Id::new(id),
        egui::UiBuilder::new()
            .layer_id(c.ui.layer_id())
            .max_rect(rect)
            .style(c.ui.style().clone()),
    );
    ui.set_clip_rect(c.painter.clip_rect());
    ui.set_opacity(c.painter.opacity());
    if !c.live {
        ui.disable();
    }
    ui.spacing_mut().interact_size = rect.size();
    ui.spacing_mut().button_padding = egui::vec2(2.0, 1.0);
    ui.style_mut().drag_value_text_style = egui::TextStyle::Body;
    ui.style_mut()
        .text_styles
        .insert(egui::TextStyle::Body, c.font(11.0, W::Bold));
    ui.style_mut().explanation_tooltips = false;
    ui.visuals_mut().widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    crate::quota_ui::bar_tooltip(
        ui.put(rect, value.update_while_editing(false)),
        "點選輸入數值".into(),
    )
}
/// 加減器：[−] 可輸入數值 [+]，右緣對齊到 right。
fn stepper(
    c: &Card,
    right: f32,
    y: f32,
    value: egui::DragValue<'_>,
    id: &str,
) -> (i32, egui::Response) {
    let (w, vw) = (25.0, 58.0);
    let x = right - w * 2.0 - vw;
    let field = island_number_field(c, c.rect(x + w, y, vw, 19.0), value, ("health-number", id));
    let delta = if setting_button(c, x, y, w, "−", false, &format!("{id}-minus")) {
        -1
    } else if setting_button(c, right - w, y, w, "+", false, &format!("{id}-plus")) {
        1
    } else {
        0
    };
    (delta, field)
}
impl App {
    pub(crate) fn page_settings(&mut self, c: &Card) {
        let help = match self.settings_category {
            0 => "外觀與系統",
            1 => "調整喝水、久坐與提醒時段",
            2 => "點選分鐘數可直接輸入，也可用加減微調",
            _ => "通知",
        };
        if self.settings_category != 1 {
            c.text(44.0, 44.0, help, 10.5, W::Regular, theme::GRAY, H::Left);
        }
        let before = format!("{:?}", self.settings);
        self.settings_editing = false;
        match self.settings_category {
            0 => {
                let choices = [
                    ("吉祥物", self.settings.pet),
                    ("極簡劉海", self.settings.minimal),
                    ("系統提示", self.settings.system_hud),
                    ("全屏隱藏", self.settings.hide_fullscreen),
                ];
                for (i, (name, on)) in choices.iter().enumerate() {
                    let x = 42.0 + (i % 2) as f32 * 169.0;
                    let y = 57.0 + (i / 2) as f32 * 27.0;
                    let text = format!("{}  {}", name, if *on { "開" } else { "關" });
                    if setting_button(c, x, y, 158.0, &text, *on, &format!("general-{i}")) {
                        match i {
                            0 => self.set_pet(!self.settings.pet),
                            1 => self.set_minimal(!self.settings.minimal),
                            2 => self.settings.system_hud = !self.settings.system_hud,
                            _ => self.settings.hide_fullscreen = !self.settings.hide_fullscreen,
                        }
                    }
                }
                c.text(
                    42.0,
                    116.0,
                    "鬼島收起時常駐；極簡模式保留細線",
                    10.5,
                    W::Regular,
                    theme::GRAY,
                    H::Left,
                );
            }
            1 if self.health_time => self.health_time_page(c),
            1 => {
                // 與健康頁同一套切換（標題旁的水滴 / 椅子）；四行一列，標籤靠左、加減器靠右對齊。
                // 喝水三行、久坐一行加兩句說明；最後一行「提醒時段」兩邊共用，位置不變。
                let sv = self.sit_view_t.pos.clamp(0.0, 1.0);
                let row_y = |i: usize| 38.0 + i as f32 * 22.0;
                for i in 1..4 {
                    c.fill(
                        44.0,
                        row_y(i) - 2.0,
                        326.0,
                        1.0,
                        0.5,
                        Color32::from_rgb(28, 28, 31),
                    );
                }
                if sv < 0.99 {
                    let wc = c.faded(1.0 - sv);
                    let wc = if self.sit_mode { wc.ghost() } else { wc };
                    for (i, label) in ["每日目標", "每次記錄", "提醒間隔"].iter().enumerate()
                    {
                        wc.text(
                            44.0,
                            row_y(i) + 13.0,
                            label,
                            10.5,
                            W::Regular,
                            theme::GRAY,
                            H::Left,
                        );
                        let goal = self.water.goal_ml;
                        let mut liters = goal as f32 / 1000.0;
                        let value = match i {
                            0 => egui::DragValue::new(&mut liters)
                                .range(0.5..=6.0)
                                .speed(0.25)
                                .suffix(" L")
                                .min_decimals(1)
                                .max_decimals(2),
                            1 => egui::DragValue::new(&mut self.settings.serving_ml)
                                .range(50..=1000)
                                .speed(50.0)
                                .suffix(" ml"),
                            _ => egui::DragValue::new(&mut self.settings.water_minutes)
                                .range(5..=180)
                                .speed(5.0)
                                .suffix(" 分鐘"),
                        };
                        let (d, field) =
                            stepper(&wc, 370.0, row_y(i), value, &format!("water-{i}"));
                        if field.gained_focus() {
                            self.pending.push(Cmd::Focus);
                        }
                        self.settings_editing |= field.has_focus();
                        match i {
                            0 => {
                                self.water.goal_ml = ((liters * 1000.0).round() as i32 + d * 250)
                                    .clamp(500, 6000)
                                    as u32;
                                if self.water.goal_ml != goal {
                                    self.water.save();
                                }
                            }
                            1 => {
                                self.settings.serving_ml =
                                    (self.settings.serving_ml as i32 + d * 50).clamp(50, 1000)
                                        as u32
                            }
                            _ => {
                                self.settings.water_minutes =
                                    (self.settings.water_minutes as i32 + d * 5).clamp(5, 180)
                                        as u32
                            }
                        }
                    }
                }
                if sv > 0.01 {
                    let sc = c.faded(sv);
                    let sc = if self.sit_mode { sc } else { sc.ghost() };
                    sc.text(
                        44.0,
                        row_y(0) + 13.0,
                        "坐滿多久提醒",
                        10.5,
                        W::Regular,
                        theme::GRAY,
                        H::Left,
                    );
                    let value = egui::DragValue::new(&mut self.settings.sit_minutes)
                        .range(15..=180)
                        .speed(5.0)
                        .suffix(" 分鐘");
                    let (d, field) = stepper(&sc, 370.0, row_y(0), value, "sit");
                    if field.gained_focus() {
                        self.pending.push(Cmd::Focus);
                    }
                    self.settings_editing |= field.has_focus();
                    if d != 0 {
                        self.settings.sit_minutes =
                            (self.settings.sit_minutes as i32 + d * 5).clamp(15, 180) as u32;
                    }
                    sc.text(
                        44.0,
                        row_y(1) + 13.0,
                        "專注計時進行中不打擾，專注結束後再提醒",
                        10.5,
                        W::Regular,
                        theme::GRAY,
                        H::Left,
                    );
                    sc.text(
                        44.0,
                        row_y(2) + 13.0,
                        "提醒後常駐，點選確認或起身後重新計時",
                        10.5,
                        W::Regular,
                        theme::GRAY,
                        H::Left,
                    );
                }
                // 最後一行兩邊共用：提醒時段 + 午休的摘要，點進二級頁修改。
                let y = row_y(3);
                let r = c
                    .hit(40.0, y - 2.0, 334.0, 22.0, "health-time")
                    .on_hover_text("設定提醒時段和午休免打擾");
                let (hv, _) = c.hover_t(&r);
                if hv > 0.0 {
                    c.fill(
                        40.0,
                        y - 2.0,
                        334.0,
                        22.0,
                        8.0,
                        Color32::from_white_alpha((hv * 14.0) as u8),
                    );
                }
                c.text(
                    44.0,
                    y + 13.0,
                    "提醒時段",
                    10.5,
                    W::Regular,
                    theme::GRAY,
                    H::Left,
                );
                c.text(
                    356.0,
                    y + 13.0,
                    &self.health_time_summary(),
                    10.5,
                    W::Semibold,
                    theme::WHITE,
                    H::Right,
                );
                c.text(366.0, y + 13.5, "›", 14.0, W::Bold, theme::GRAY, H::Center);
                if r.clicked() {
                    self.health_time = true;
                }
            }
            2 => {
                for (i, name) in ["專注", "休息"].iter().enumerate() {
                    let y = 53.0 + i as f32 * 23.0;
                    c.text(48.0, y + 13.0, name, 10.5, W::Regular, theme::GRAY, H::Left);
                    let value = if i == 0 {
                        &mut self.settings.focus_minutes
                    } else {
                        &mut self.settings.rest_minutes
                    };
                    let max = if i == 0 { 180 } else { 60 };
                    let rect = c.rect(168.0, y, 76.0, 19.0);
                    let field = island_number_field(
                        c,
                        rect,
                        egui::DragValue::new(value).range(1..=max).suffix(" 分鐘"),
                        ("focus-duration", i),
                    );
                    if field.gained_focus() {
                        self.pending.push(Cmd::Focus);
                    }
                    self.settings_editing |= field.has_focus();
                    if setting_button(c, 131.0, y, 27.0, "−", false, &format!("focus-minus-{i}"))
                    {
                        *value = value.saturating_sub(1).max(1);
                    }
                    if setting_button(c, 255.0, y, 27.0, "+", false, &format!("focus-plus-{i}")) {
                        *value = (*value + 1).min(max);
                    }
                }
                c.text(
                    315.0,
                    93.0,
                    "下次生效",
                    10.5,
                    W::Regular,
                    theme::GRAY,
                    H::Center,
                );
            }
            _ => {
                if setting_button(
                    c,
                    43.0,
                    56.0,
                    152.0,
                    if self.settings.notifications {
                        "提醒 · 開"
                    } else {
                        "提醒 · 關"
                    },
                    self.settings.notifications,
                    "set-notice",
                ) {
                    self.settings.notifications = !self.settings.notifications;
                }
                if setting_button(
                    c,
                    212.0,
                    56.0,
                    153.0,
                    if self.settings.system_notify && self.settings.sound {
                        "聲音 · 開"
                    } else {
                        "聲音 · 關"
                    },
                    self.settings.system_notify && self.settings.sound,
                    "set-sound",
                ) {
                    self.settings.sound = !(self.settings.system_notify && self.settings.sound);
                    if self.settings.sound {
                        self.settings.system_notify = true;
                    }
                }
                if setting_button(
                    c,
                    43.0,
                    84.0,
                    152.0,
                    if self.settings.read_notifications {
                        "斷開系統通知"
                    } else {
                        "連線系統通知"
                    },
                    self.settings.read_notifications,
                    "set-listener",
                ) {
                    self.pending.push(if self.settings.read_notifications {
                        Cmd::DisconnectNotifications
                    } else {
                        Cmd::RequestNotifications
                    });
                }
                let status = c.fit(&self.listener_status, 10.5, W::Regular, 170.0);
                c.text(210.0, 97.0, &status, 10.5, W::Regular, theme::GRAY, H::Left);
            }
        }
        if before != format!("{:?}", self.settings) {
            self.settings.save();
        }
    }
}

/// 歌詞顏色：開關、色盤選取與可拖曳的漸層色帶（含一次性拖曳引導）。
pub(crate) fn lyric_gradient_editor(ui: &mut egui::Ui, settings: &mut crate::model::Settings) {
    setting_row(
        ui,
        "歌詞漸層",
        "以拖曳方式讓島上歌詞在色盤上漸變換色",
        &mut settings.lyric_gradient,
    );
    if settings.lyric_gradient {
        ui.separator();
        // 色盤色塊，點擊選取；選中項加白色外框。
        let swatch = 28.0;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
            for p in 0..crate::lyric_gradient::PALETTE_COUNT {
                let (rect, resp) =
                    ui.allocate_exact_size(egui::vec2(swatch, swatch), egui::Sense::click());
                ui.painter().add(egui::Shape::Mesh(std::sync::Arc::new(
                    crate::lyric_gradient::strip_mesh(rect, p, 6.0),
                )));
                if resp.clicked() {
                    settings.lyric_gradient_preset = p;
                    settings.lyric_gradient_hint = false;
                }
                if settings.lyric_gradient_preset == p {
                    ui.painter().rect_stroke(
                        rect.expand(2.0),
                        7.0,
                        egui::Stroke::new(1.8, Color32::WHITE),
                        egui::StrokeKind::Middle,
                    );
                }
            }
        });
        ui.add_space(4.0);
        // 可拖曳漸層色帶：拖曳控制漸層相位。
        let (rect, resp) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 18.0), egui::Sense::drag());
        ui.painter().add(egui::Shape::Mesh(std::sync::Arc::new(
            crate::lyric_gradient::strip_mesh(rect, settings.lyric_gradient_preset, 9.0),
        )));
        let hx = rect.left() + rect.width() * settings.lyric_gradient_shift;
        ui.painter().circle_filled(
            egui::pos2(hx, rect.center().y),
            8.0,
            Color32::from_black_alpha(60),
        );
        ui.painter()
            .circle_filled(egui::pos2(hx, rect.center().y), 7.0, Color32::WHITE);
        if resp.drag_started() || resp.dragged() {
            if let Some(pointer) = resp.interact_pointer_pos() {
                settings.lyric_gradient_shift =
                    ((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            }
            settings.lyric_gradient_hint = false;
        }
        // 引導：首次使用提示一次，開始拖曳或選色後自動關閉。
        if settings.lyric_gradient_hint {
            settings_hint(
                ui,
                "按住色帶左右拖動，即可捲動歌詞的漸層位置；點上方色塊可更換色盤。",
            );
        }
        settings_hint(ui, "漸層套用於島上目前歌詞；關閉開關後恢復單色。");
    }
}

/// 歌詞導唱外掛：開關與樣式（內建或 `%APPDATA%\Kuidao\lyric-karaoke` 的樣式包）。
pub(crate) fn lyric_karaoke_editor(ui: &mut egui::Ui, settings: &mut crate::model::Settings) {
    ui.separator();
    setting_row(
        ui,
        "歌詞導唱",
        "跟播放進度逐字亮起：已唱的字走漸層色，未唱的字變暗，前緣柔和過渡",
        &mut settings.lyric_karaoke,
    );
    if settings.lyric_karaoke {
        let names = crate::lyric_karaoke::style_names();
        if !names
            .iter()
            .any(|name| *name == settings.lyric_karaoke_style)
        {
            settings.lyric_karaoke_style = crate::lyric_karaoke::BUILTIN.into();
        }
        let ctx = &ui.ctx().clone();
        ui.horizontal(|ui| {
            ui.add(egui::Label::new(
                egui::RichText::new("導唱樣式")
                    .size(13.0)
                    .color(settings_color(
                        ctx,
                        Color32::WHITE,
                        Color32::from_rgb(30, 34, 41),
                    )),
            ));
            egui::ComboBox::from_id_salt("lyric-karaoke-style")
                .selected_text(settings.lyric_karaoke_style.clone())
                .width(180.0)
                .show_ui(ui, |ui| {
                    for name in &names {
                        ui.selectable_value(
                            &mut settings.lyric_karaoke_style,
                            name.clone(),
                            name.clone(),
                        );
                    }
                });
        });
        settings_hint(
            ui,
            "導唱與上面的整句漸層可各自開關；導唱只作用在目前這一句，不會顯示譯文。",
        );
        settings_hint(ui,"自訂樣式包：把 JSON 放進 %APPDATA%\\Kuidao\\lyric-karaoke\\ 後重新啟動，名稱就會出現在上面；格式錯誤會被忽略，不影響內建樣式。");
    }
}

fn switch(ui: &mut egui::Ui, value: &mut bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(34.0, 20.0), egui::Sense::click());
    if response.clicked() {
        *value = !*value;
    }
    let fill = if *value {
        Color32::from_rgb(48, 184, 92)
    } else {
        settings_color(
            &ui.ctx().clone(),
            Color32::from_rgb(82, 82, 87),
            Color32::from_rgb(203, 207, 214),
        )
    };
    ui.painter().rect_filled(rect, 10.0, fill);
    let x = if *value {
        rect.right() - 10.0
    } else {
        rect.left() + 10.0
    };
    ui.painter().circle_filled(
        egui::pos2(x, rect.center().y + 0.5),
        7.5,
        Color32::from_black_alpha(35),
    );
    ui.painter()
        .circle_filled(egui::pos2(x, rect.center().y), 7.0, Color32::WHITE);
    response.clicked()
}
/// 設定頁版式：標籤列寬、開關尺寸、控件與卡片右邊緣之間的固定留白。
pub(crate) const SETTINGS_LABEL_W: f32 = 80.0;
const SETTINGS_SWITCH_W: f32 = 34.0;
const SETTINGS_ROW_GAP: f32 = 16.0;

/// 開關行：左側標題 + 說明（寬度固定、自動換行），右側開關，長說明也不會把開關擠出卡片。
pub(crate) fn setting_row(ui: &mut egui::Ui, title: &str, description: &str, value: &mut bool) {
    let ctx = &ui.ctx().clone();
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        let text_w = (ui.available_width() - SETTINGS_SWITCH_W - SETTINGS_ROW_GAP).max(120.0);
        ui.vertical(|ui| {
            ui.set_width(text_w);
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.add(
                egui::Label::new(egui::RichText::new(title).size(13.0).color(settings_color(
                    ctx,
                    Color32::WHITE,
                    Color32::from_rgb(30, 34, 41),
                )))
                .wrap(),
            );
            if !description.is_empty() {
                ui.add(
                    egui::Label::new(egui::RichText::new(description).size(11.5).color(
                        settings_color(ctx, Color32::from_gray(185), Color32::from_gray(82)),
                    ))
                    .wrap(),
                );
            }
        });
        // add_space 之後還會再插入一次 item_spacing.x，這裡一併扣掉，開關才不會每行往外擠一點。
        ui.add_space(
            (ui.available_width() - SETTINGS_SWITCH_W - ui.spacing().item_spacing.x).max(0.0),
        );
        switch(ui, value);
    });
    ui.add_space(2.0);
}

/// 模塊關閉時禁用常駐開關，圓點仍反映儲存的偏好。
fn resident_setting_row(ui:&mut egui::Ui,title:&str,description:&str,module_on:bool,value:&mut bool){
    ui.add_enabled_ui(module_on,|ui|{
        setting_row(ui,title,if module_on {description}else{"功能頁已關閉"},value);
    });
}

/// 設定頁統一的分段選擇器（灰色底框 + 選中塊）。返回值為是否發生了切換。
pub(crate) fn settings_segmented<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    value: &mut T,
    options: &[(T, &str)],
    item_w: f32,
) -> bool {
    let ctx = &ui.ctx().clone();
    let mut changed = false;
    egui::Frame::new()
        .fill(settings_color(
            ctx,
            Color32::from_rgb(28, 28, 30),
            Color32::from_rgb(230, 233, 238),
        ))
        .corner_radius(8.0)
        .inner_margin(3)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (option, label) in options {
                    let selected = *value == *option;
                    let text = egui::RichText::new(*label).size(12.0).color(if selected {
                        settings_color(ctx, Color32::WHITE, Color32::from_rgb(30, 34, 41))
                    } else {
                        settings_color(ctx, Color32::from_gray(200), Color32::from_gray(70))
                    });
                    let button = egui::Button::new(text)
                        .fill(if selected {
                            settings_color(ctx, Color32::from_rgb(72, 72, 76), Color32::WHITE)
                        } else {
                            Color32::TRANSPARENT
                        })
                        .stroke(egui::Stroke::NONE)
                        .corner_radius(6.0);
                    let width = item_w.max(
                        ctx.fonts(|f| {
                            f.layout_no_wrap(
                                label.to_string(),
                                egui::FontId::proportional(12.0),
                                Color32::WHITE,
                            )
                            .size()
                            .x
                        }) + 20.0,
                    );
                    // 選中項浮起一點：淺色模式白底加淡陰影，深色模式更亮的底色加陰影。
                    let clicked = if selected {
                        egui::Frame::new()
                            .corner_radius(6.0)
                            .shadow(egui::Shadow {
                                offset: [0, 1],
                                blur: 4,
                                spread: 0,
                                color: Color32::from_black_alpha(if ui.visuals().dark_mode {
                                    90
                                } else {
                                    38
                                }),
                            })
                            .show(ui, |ui| ui.add_sized([width, 26.0], button))
                            .inner
                            .clicked()
                    } else {
                        ui.add_sized([width, 26.0], button).clicked()
                    };
                    if clicked && !selected {
                        *value = *option;
                        changed = true;
                    }
                }
            });
        });
    changed
}

/// 選項行：標籤列之後緊跟控件，同一卡片裡所有控件左緣對齊成一條豎線。
pub(crate) fn settings_control_row_right(
    ui: &mut egui::Ui,
    title: &str,
    controls: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    settings_control_row(ui, title, SETTINGS_LABEL_W, |ui| {
        ui.horizontal(|ui| controls(ui)).response
    })
}

/// 設定項下方的說明文字。
fn island_size_slider(ui: &mut egui::Ui, value: &mut u32) -> egui::Response {
    let id = ui.make_persistent_id("island-size-draft");
    let mut draft = ui.data(|d| d.get_temp::<u32>(id)).unwrap_or(*value);
    let response = ui.add(
        egui::Slider::new(
            &mut draft,
            crate::model::ISLAND_SIZE_MIN..=crate::model::ISLAND_SIZE_MAX,
        )
        .step_by(crate::model::ISLAND_SIZE_STEP as f64)
        .suffix("%"),
    );
    // 文字輸入與拖曳都在變更當下就套用，避免放開滑鼠前的舊草稿把數字跳回最大值。
    if response.changed() {
        *value = crate::model::island_size_value(draft);
        ui.data_mut(|d| d.remove::<u32>(id));
    } else if !ui.input(|i| i.pointer.primary_down()) {
        ui.data_mut(|d| d.remove::<u32>(id));
    }
    response
}

/// 目前跑的是哪一種版本：開發版主程式檔名帶「開發版」，正式版沒有。
pub(crate) fn channel_label() -> &'static str {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.file_name().map(|name| name.to_string_lossy().into_owned()))
        .filter(|name| name.contains("開發版"))
        .map_or("正式版", |_| "開發版")
}

pub(crate) fn settings_hint(ui: &mut egui::Ui, text: &str) {
    let ctx = &ui.ctx().clone();
    ui.add(
        egui::Label::new(egui::RichText::new(text).size(11.5).color(settings_color(
            ctx,
            Color32::from_gray(170),
            Color32::from_gray(92),
        )))
        .wrap(),
    );
}
pub(crate) fn settings_card(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    let ctx = &ui.ctx().clone();
    egui::Frame::new()
        .fill(settings_color(
            ctx,
            Color32::from_rgb(35, 35, 38),
            Color32::from_rgb(255, 255, 255),
        ))
        .corner_radius(10.0)
        .inner_margin(egui::Margin::symmetric(16, 12))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 8.0;
            add(ui);
        });
}

/// 卡片底部的「高級」摺疊區：只放內容多的頁面裡不常改的項，預設收起。整條都可以點，右側箭頭表示展開狀態。
pub(crate) fn settings_advanced(ui: &mut egui::Ui, id: &str, add: impl FnOnce(&mut egui::Ui)) {
    let ctx = &ui.ctx().clone();
    let state = egui::Id::new(("settings-advanced", id));
    let mut open = ctx.data(|d| d.get_temp::<bool>(state)).unwrap_or(false);
    ui.add_space(2.0);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 34.0), egui::Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(
            rect.expand2(egui::vec2(6.0, 0.0)),
            7.0,
            settings_color(
                ctx,
                Color32::from_rgb(44, 44, 48),
                Color32::from_rgb(242, 244, 247),
            ),
        );
    }
    let fg = settings_color(ctx, Color32::from_gray(205), Color32::from_gray(60));
    ui.painter().text(
        egui::pos2(rect.left(), rect.center().y),
        egui::Align2::LEFT_CENTER,
        "高階",
        egui::FontId::proportional(13.0),
        fg,
    );
    // 右側箭頭：收起朝右，展開朝下。
    let c = egui::pos2(rect.right() - 10.0, rect.center().y);
    let pts = if open {
        vec![
            c + egui::vec2(-4.5, -2.0),
            c + egui::vec2(0.0, 2.5),
            c + egui::vec2(4.5, -2.0),
        ]
    } else {
        vec![
            c + egui::vec2(-2.0, -4.5),
            c + egui::vec2(2.5, 0.0),
            c + egui::vec2(-2.0, 4.5),
        ]
    };
    ui.painter()
        .add(egui::Shape::line(pts, egui::Stroke::new(1.6, fg)));
    if response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
    {
        open = !open;
        ctx.data_mut(|d| d.insert_temp(state, open));
    }
    if open {
        ui.spacing_mut().item_spacing.y = 8.0;
        add(ui);
    }
}
/// 狀態小標籤：「已開啟」綠色，「已關閉」灰色；固定高度的膠囊，不隨行高拉伸。
fn settings_state_chip(ui: &mut egui::Ui, on: bool) {
    let ctx = &ui.ctx().clone();
    let (text, fg, bg) = if on {
        (
            "已開啟",
            settings_color(
                ctx,
                Color32::from_rgb(127, 214, 155),
                Color32::from_rgb(29, 122, 60),
            ),
            settings_color(
                ctx,
                Color32::from_rgb(28, 51, 36),
                Color32::from_rgb(227, 244, 232),
            ),
        )
    } else {
        (
            "已關閉",
            settings_color(ctx, Color32::from_gray(160), Color32::from_gray(105)),
            settings_color(
                ctx,
                Color32::from_rgb(44, 44, 48),
                Color32::from_rgb(232, 235, 240),
            ),
        )
    };
    let galley = ui
        .painter()
        .layout_no_wrap(text.into(), egui::FontId::proportional(11.5), fg);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(galley.size().x + 16.0, 20.0),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(rect, 10.0, bg);
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, fg);
}
/// 設定頁統一的控件樣式（按鈕、輸入框、選中色）；設定頁正文和彈窗共用。
pub(crate) fn settings_apply_style(ui: &mut egui::Ui) {
    let ctx = &ui.ctx().clone();
    // 分割線：淺色模式用很淡的灰，深色模式略亮於卡片，不再是系統預設的粗灰線。
    ui.visuals_mut().widgets.noninteractive.bg_stroke = egui::Stroke::new(
        1.0,
        settings_color(
            ctx,
            Color32::from_rgb(52, 52, 57),
            Color32::from_rgb(233, 235, 240),
        ),
    );
    ui.spacing_mut().item_spacing.y = ui.spacing().item_spacing.y.max(5.0);
    ui.spacing_mut().item_spacing = egui::vec2(8.0, 5.0);
    ui.spacing_mut().button_padding = egui::vec2(10.0, 6.0);
    ui.style_mut()
        .text_styles
        .insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
    ui.visuals_mut().override_text_color = Some(settings_color(
        ctx,
        Color32::WHITE,
        Color32::from_rgb(30, 34, 41),
    ));
    ui.visuals_mut().extreme_bg_color = settings_color(
        ctx,
        Color32::from_rgb(25, 25, 28),
        Color32::from_rgb(244, 246, 249),
    );
    ui.visuals_mut().widgets.inactive.bg_fill = settings_color(
        ctx,
        Color32::from_rgb(50, 50, 54),
        Color32::from_rgb(228, 231, 236),
    );
    ui.visuals_mut().widgets.hovered.bg_fill = settings_color(
        ctx,
        Color32::from_rgb(65, 65, 71),
        Color32::from_rgb(216, 222, 231),
    );
    // 統一按鈕：高度 28、圓角 7、內邊距 12×5，無描邊；預設按鈕也套用這套樣式。
    ui.spacing_mut().button_padding = egui::vec2(12.0, 5.0);
    ui.spacing_mut().interact_size.y = 28.0;
    {
        let fill = settings_color(
            ctx,
            Color32::from_rgb(50, 50, 54),
            Color32::from_rgb(228, 231, 236),
        );
        let hover = settings_color(
            ctx,
            Color32::from_rgb(65, 65, 71),
            Color32::from_rgb(216, 222, 231),
        );
        let widgets = &mut ui.visuals_mut().widgets;
        for (w, f) in [
            (&mut widgets.inactive, fill),
            (&mut widgets.hovered, hover),
            (&mut widgets.active, hover),
        ] {
            w.corner_radius = egui::CornerRadius::same(7);
            w.weak_bg_fill = f;
            w.bg_stroke = egui::Stroke::NONE;
        }
    }
    ui.visuals_mut().selection.bg_fill = settings_color(
        ctx,
        Color32::from_rgb(45, 105, 165),
        Color32::from_rgb(209, 226, 248),
    );
}
/// 設定頁彈窗的外框：卡片底色、大圓角、細描邊和陰影。
pub(crate) fn settings_modal_frame(ctx: &egui::Context) -> egui::Frame {
    egui::Frame::new()
        .fill(settings_color(
            ctx,
            Color32::from_rgb(36, 36, 40),
            Color32::WHITE,
        ))
        .corner_radius(14.0)
        .inner_margin(egui::Margin::same(20))
        .stroke(egui::Stroke::new(
            1.0,
            settings_color(
                ctx,
                Color32::from_rgb(58, 58, 64),
                Color32::from_rgb(222, 225, 231),
            ),
        ))
        .shadow(egui::Shadow {
            offset: [0, 8],
            blur: 28,
            spread: 0,
            color: Color32::from_black_alpha(90),
        })
}
/// 彈窗標題欄：標題在左、關閉在右，下面一條分隔線。返回是否點了關閉。
pub(crate) fn settings_modal_title(ui: &mut egui::Ui, title: &str) -> bool {
    let ctx = &ui.ctx().clone();
    let mut close = false;
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(title)
                .size(16.0)
                .strong()
                .color(settings_color(
                    ctx,
                    Color32::WHITE,
                    Color32::from_rgb(30, 34, 41),
                )),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            close = ui
                .add(egui::Button::new(egui::RichText::new("×").size(18.0)).frame(false))
                .on_hover_text("關閉")
                .clicked();
        });
    });
    ui.add_space(4.0);
    settings_separator(ui);
    ui.add_space(6.0);
    close
}
/// 設定頁分割線（樣式由 settings_apply_style 統一設定）。
fn settings_separator(ui: &mut egui::Ui) {
    ui.separator();
}
/// 設定頁的小型動作按鈕圖標；用矢量繪製，避免額外位圖在深淺主題下發灰。
#[derive(Clone, Copy)]
enum SettingsActionGlyph {
    Refresh,
    Trash,
}
/// 設定頁動作按鈕：34px 高膠囊、左側圖標、平滑 hover；主要動作用綠色，清除類用安靜的描邊紅。
fn settings_action_button(
    ui: &mut egui::Ui,
    text: &str,
    glyph: SettingsActionGlyph,
    primary: bool,
    enabled: bool,
    id: &str,
) -> egui::Response {
    let ctx = ui.ctx().clone();
    let active = enabled && ui.is_enabled();
    let font = egui::FontId::proportional(12.5);
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font, Color32::WHITE);
    let width = (galley.size().x + 48.0).max(180.0);
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(width, 34.0),
        if active {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    let hover = ctx.animate_bool_with_time(
        egui::Id::new(("settings-action", id)).with("hover"),
        active && response.hovered(),
        0.13,
    );
    let press = ctx.animate_bool_with_time(
        egui::Id::new(("settings-action", id)).with("press"),
        active && response.is_pointer_button_down_on(),
        0.08,
    );
    let body = rect.translate(egui::vec2(0.0, press * 1.2));
    let (fill, fill_hover, stroke, stroke_hover, fg, fg_hover) = if primary {
        (
            settings_color(
                &ctx,
                Color32::from_rgb(26, 47, 35),
                Color32::from_rgb(228, 245, 234),
            ),
            settings_color(
                &ctx,
                Color32::from_rgb(34, 62, 45),
                Color32::from_rgb(211, 239, 222),
            ),
            settings_color(
                &ctx,
                Color32::from_rgb(55, 98, 72),
                Color32::from_rgb(188, 222, 199),
            ),
            settings_color(
                &ctx,
                Color32::from_rgb(86, 150, 109),
                Color32::from_rgb(151, 205, 172),
            ),
            settings_color(
                &ctx,
                Color32::from_rgb(146, 229, 175),
                Color32::from_rgb(25, 111, 57),
            ),
            settings_color(
                &ctx,
                Color32::from_rgb(178, 244, 202),
                Color32::from_rgb(19, 92, 46),
            ),
        )
    } else {
        (
            settings_color(
                &ctx,
                Color32::from_rgb(42, 40, 42),
                Color32::from_rgb(247, 244, 245),
            ),
            settings_color(
                &ctx,
                Color32::from_rgb(57, 40, 42),
                Color32::from_rgb(252, 232, 234),
            ),
            settings_color(
                &ctx,
                Color32::from_rgb(72, 61, 63),
                Color32::from_rgb(230, 207, 210),
            ),
            settings_color(
                &ctx,
                Color32::from_rgb(127, 72, 77),
                Color32::from_rgb(218, 158, 164),
            ),
            settings_color(
                &ctx,
                Color32::from_rgb(218, 182, 185),
                Color32::from_rgb(145, 62, 70),
            ),
            settings_color(
                &ctx,
                Color32::from_rgb(249, 190, 194),
                Color32::from_rgb(178, 44, 56),
            ),
        )
    };
    let (fill, stroke, fg) = if !active {
        (
            settings_color(
                &ctx,
                Color32::from_rgb(38, 38, 41),
                Color32::from_rgb(240, 241, 244),
            ),
            settings_color(
                &ctx,
                Color32::from_rgb(54, 54, 58),
                Color32::from_rgb(225, 228, 233),
            ),
            settings_color(&ctx, Color32::from_gray(105), Color32::from_gray(155)),
        )
    } else {
        (
            crate::paint::mix(fill, fill_hover, hover),
            crate::paint::mix(stroke, stroke_hover, hover),
            crate::paint::mix(fg, fg_hover, hover),
        )
    };
    let rad = body.height() * 0.5;
    ui.painter().rect_filled(
        body.translate(egui::vec2(0.0, 2.0 - press)),
        rad,
        Color32::from_black_alpha(if active { 34 } else { 18 }),
    );
    ui.painter().rect_filled(body, rad, fill);
    ui.painter().rect_stroke(
        body,
        rad,
        egui::Stroke::new(1.0, stroke),
        egui::StrokeKind::Inside,
    );
    let content_w = 14.0 + 7.0 + galley.size().x;
    let left = body.center().x - content_w * 0.5;
    let icon_center = egui::pos2(left + 7.0, body.center().y);
    match glyph {
        SettingsActionGlyph::Refresh => paint_refresh_glyph(ui, icon_center, fg),
        SettingsActionGlyph::Trash => paint_trash_glyph(ui, icon_center, fg),
    }
    ui.painter().galley(
        egui::pos2(left + 21.0, body.center().y - galley.size().y * 0.5),
        galley,
        fg,
    );
    if active && response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}
fn paint_refresh_glyph(ui: &egui::Ui, center: egui::Pos2, color: Color32) {
    let radius = 5.2_f32;
    let mut points = Vec::with_capacity(25);
    for i in 0..=24 {
        let angle = 0.15 + 4.56 * (i as f32 / 24.0);
        points.push(center + egui::vec2(angle.cos() * radius, angle.sin() * radius));
    }
    let stroke = egui::Stroke::new(1.6, color);
    ui.painter().add(egui::Shape::line(points, stroke));
    let end_angle = 4.71_f32;
    let end = center + egui::vec2(end_angle.cos() * radius, end_angle.sin() * radius);
    let tangent = egui::vec2(-end_angle.sin(), end_angle.cos());
    let normal = egui::vec2(end_angle.cos(), end_angle.sin());
    // 箭頭放在正上方的缺口，24px 下仍能看出是「重新整理」而不是一個 C。
    let tip = end + tangent * 3.0;
    let base = end - tangent * 1.4;
    ui.painter().add(egui::Shape::convex_polygon(
        vec![tip, base + normal * 2.6, base - normal * 2.6],
        color,
        egui::Stroke::NONE,
    ));
}
fn paint_trash_glyph(ui: &egui::Ui, center: egui::Pos2, color: Color32) {
    let stroke = egui::Stroke::new(1.5, color);
    let body = egui::Rect::from_min_max(
        center + egui::vec2(-4.3, -1.6),
        center + egui::vec2(4.3, 6.3),
    );
    ui.painter()
        .rect_stroke(body, 1.0, stroke, egui::StrokeKind::Inside);
    ui.painter().line_segment(
        [
            center + egui::vec2(-6.1, -3.3),
            center + egui::vec2(6.1, -3.3),
        ],
        stroke,
    );
    ui.painter().line_segment(
        [
            center + egui::vec2(-2.5, -5.1),
            center + egui::vec2(2.5, -5.1),
        ],
        stroke,
    );
    for x in [-1.7, 1.7] {
        ui.painter().line_segment(
            [center + egui::vec2(x, 0.4), center + egui::vec2(x, 3.8)],
            stroke,
        );
    }
}
/// 可點選的標籤：開啟時綠色底，關閉時描邊灰字。
fn settings_toggle_chip(ui: &mut egui::Ui, text: &str, on: bool) -> egui::Response {
    let ctx = &ui.ctx().clone();
    let label = text.to_string();
    let fg = if on {
        settings_color(
            ctx,
            Color32::from_rgb(127, 214, 155),
            Color32::from_rgb(29, 122, 60),
        )
    } else {
        settings_color(ctx, Color32::from_gray(185), Color32::from_gray(80))
    };
    let fill = if on {
        settings_color(
            ctx,
            Color32::from_rgb(28, 51, 36),
            Color32::from_rgb(227, 244, 232),
        )
    } else {
        Color32::TRANSPARENT
    };
    let stroke = if on {
        egui::Stroke::NONE
    } else {
        egui::Stroke::new(
            1.0,
            settings_color(
                ctx,
                Color32::from_rgb(60, 60, 66),
                Color32::from_rgb(214, 218, 225),
            ),
        )
    };
    ui.add(
        egui::Button::new(egui::RichText::new(label).size(12.0).color(fg))
            .fill(fill)
            .stroke(stroke)
            .corner_radius(13.0)
            .min_size(egui::vec2(0.0, 26.0)),
    )
}
/// 功能頁面對應的圖標，與側欄同一套。
/// 紋理緩存只按地址區分、不分尺寸：側欄 23px 先載入後，36px 的卡片圖標會被放大發糊，所以這裡用獨立地址。
fn settings_page_icon(page: usize) -> egui::ImageSource<'static> {
    use crate::app::*;
    let (name,bytes):(&str,&'static [u8])=match page {
        MUSIC_PAGE=>("music",include_bytes!("../assets/settings-icons/system/music.svg")),
        FOCUS_PAGE=>("focus",include_bytes!("../assets/settings-icons/system/focus.svg")),
        WATER_PAGE=>("health",include_bytes!("../assets/settings-icons/system/health.svg")),
        SHELF_PAGE=>("shelf",include_bytes!("../assets/settings-icons/system/shelf.svg")),
        TODO_PAGE=>("tasks",include_bytes!("../assets/settings-icons/system/tasks.svg")),
        MAIL_PAGE=>("mail",include_bytes!("../assets/settings-icons/system/mail.svg")),
        QUOTA_PAGE=>("quota",include_bytes!("../assets/settings-icons/system/quota.svg")),
        CALENDAR_PAGE=>("calendar",include_bytes!("../assets/settings-icons/system/calendar.svg")),
        PERFORMANCE_PAGE=>("performance",include_bytes!("../assets/settings-icons/system/performance.svg")),
        BLUETOOTH_PAGE=>("wireless",include_bytes!("../assets/device-icons/bluetooth-linear.svg")),
        _=>("weather",include_bytes!("../assets/settings-icons/system/weather.svg")),
    };
    egui::ImageSource::Bytes {
        uri: format!("bytes://settings-header-{name}.svg").into(),
        bytes: bytes.into(),
    }
}

fn paint_wireless_settings_icon(ui:&egui::Ui,rect:egui::Rect,icon:egui::ImageSource<'static>){
    ui.painter().rect_filled(rect,rect.width()*0.25,Color32::from_rgb(57,118,211));
    egui::Image::new(icon).paint_at(ui,rect.shrink(rect.width()*0.15));
}

/// 設定分頁對應的功能模塊（頁面序號）；通用、外觀、位置、訊息提醒、關於、頁面管理不屬於任何模塊。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn settings_tab_module(tab: usize) -> Option<usize> {
    use crate::app::*;
    match tab {1=>Some(MUSIC_PAGE),2=>Some(FOCUS_PAGE),3=>Some(WATER_PAGE),4=>Some(TODO_PAGE),5=>Some(SHELF_PAGE),7=>Some(MAIL_PAGE),8=>Some(CALENDAR_PAGE),11=>Some(QUOTA_PAGE),12=>Some(WEATHER_PAGE),15=>Some(PERFORMANCE_PAGE),SETTINGS_TAB_WIRELESS=>Some(BLUETOOTH_PAGE),_=>None}
}

pub(crate) fn settings_label(ui: &mut egui::Ui, text: &str) {
    let ctx = &ui.ctx().clone();
    ui.add_space(9.0);
    ui.label(egui::RichText::new(text).size(11.5).color(settings_color(
        ctx,
        Color32::from_gray(185),
        Color32::from_gray(70),
    )));
    ui.add_space(2.0);
}
fn support_author(ui:&mut egui::Ui){
    let id=egui::Id::new("support-author-open");
    let open=ui.ctx().data(|data|data.get_temp::<bool>(id).unwrap_or(false));
    if !open {release_support_codes(ui.ctx());}
    let dark=ui.visuals().dark_mode;
    ui.scope(|ui|{
    if !open {
    egui::Frame::new().fill(if dark{Color32::from_rgb(42,37,53)}else{Color32::from_rgb(239,233,250)})
        .corner_radius(10.0).inner_margin(egui::Margin::symmetric(12,8)).show(ui,|ui|{
            let image_width=120.0;
            let text_width=(ui.available_width()-image_width-ui.spacing().item_spacing.x).max(0.0);
            ui.horizontal(|ui|{
                ui.allocate_ui_with_layout(egui::vec2(text_width,64.0),egui::Layout::top_down(egui::Align::Min).with_main_align(egui::Align::Center),|ui|{
                    ui.set_max_width(text_width);
                    ui.label(egui::RichText::new("請作者喝杯咖啡").size(19.0).strong());
                    ui.add(egui::Label::new("鬼島陪你認真生活，你的支援，讓它慢慢做好。").wrap());
                });
                ui.add(egui::Image::new(egui::include_image!("../assets/support/kuidao-coffee.png")).fit_to_exact_size(egui::vec2(image_width,62.5)));
            });
        });
    } else {
        ui.set_min_height(200.0);
        let ctx=ui.ctx().clone();
        // 只解碼一次並裁出二維碼區塊，按實際顯示像素縮小，收起時釋放。
        let pixels=((180.0*ctx.pixels_per_point()).ceil() as u32).clamp(180,600);
        ui.columns(2,|columns|{
            for (i,(name,bytes,crop,color)) in [
                ("街口支付",include_bytes!("../assets/support/support-jkopay-qr.png").as_slice(),[0.0,0.0,600.0],Color32::from_rgb(214,0,40)),
                ("支付寶",include_bytes!("../assets/support/support-alipay-qr.png").as_slice(),[0.0,0.0,600.0],Color32::from_rgb(22,120,255)),
            ].into_iter().enumerate(){
                let texture_id=egui::Id::new(("support-author-qr",i));
                let texture=ctx.data_mut(|data|data.get_temp::<egui::TextureHandle>(texture_id)).filter(|texture|texture.size()==[pixels as usize;2]).unwrap_or_else(||{
                    let image=support_code_image(bytes,crop,pixels);
                    let texture=ctx.load_texture(name,image,egui::TextureOptions::LINEAR);
                    ctx.data_mut(|data|data.insert_temp(texture_id,texture.clone()));texture
                });
                let uv=egui::Rect::from_min_max(egui::pos2(0.0,0.0),egui::pos2(1.0,1.0));
                columns[i].vertical_centered(|ui|{
                    ui.label(egui::RichText::new(name).color(color).strong());
                    let side=ui.available_width().min(156.0);
                    ui.add(egui::Image::new((texture.id(),egui::vec2(side,side))).uv(uv));
                    ui.label(egui::RichText::new(if i==0{"用街口支付掃一掃"}else{"打開支付寶掃一掃"}).size(11.0));
                });
            }
        });
    }
    });
    let button=egui::Button::new(egui::RichText::new(if open{"返回"}else{"請喝咖啡 · 支援作者"}).color(Color32::from_rgb(33,27,51)))
        .fill(Color32::from_rgb(182,163,245)).corner_radius(8.0);
    if ui.add_sized([ui.available_width(),32.0],button).clicked(){
        ui.ctx().data_mut(|data|data.insert_temp(id,!open));
    }
    ui.add_space(if open{4.0}else{7.0});
    ui.label(egui::RichText::new("完全自願 · 感謝每一份支援 · 不影響功能使用").size(11.0).color(ui.visuals().weak_text_color()));
}


pub(crate) fn settings_control_row(
    ui: &mut egui::Ui,
    title: &str,
    label_width: f32,
    controls: impl FnOnce(&mut egui::Ui) -> egui::Response,
) -> egui::Response {
    ui.horizontal(|ui| {
        // 同一張卡片裡的控件左緣對齊：標籤列至少 SETTINGS_LABEL_W。
        let (label_rect, _) = ui.allocate_exact_size(
            egui::vec2(label_width.max(SETTINGS_LABEL_W), 30.0),
            egui::Sense::hover(),
        );
        let content = controls(ui);
        ui.painter().text(
            egui::pos2(label_rect.left(), content.rect.center().y),
            egui::Align2::LEFT_CENTER,
            title,
            egui::TextStyle::Body.resolve(ui.style()),
            ui.visuals().text_color(),
        );
        content
    })
    .inner
}
fn mail_connection_choice(ui: &mut egui::Ui, id_salt: impl std::hash::Hash, use_windows_proxy: &mut bool) -> bool {
    let before = *use_windows_proxy;
    let direct = "直連（預設）";
    let proxy = "使用 Windows 手動代理";
    settings_control_row(ui, "連線方式", 56.0, |ui| {
        egui::ComboBox::from_id_salt(id_salt).width(170.0)
            .selected_text(if before { proxy } else { direct })
            .show_ui(ui, |ui| {
                ui.selectable_value(use_windows_proxy, false, direct);
                ui.selectable_value(use_windows_proxy, true, proxy);
            }).response
    });
    *use_windows_proxy != before
}
fn setting_number<N:egui::emath::Numeric>(ui:&mut egui::Ui,title:&str,value:&mut N,min:N,max:N,suffix:&str,speed:f64)->bool{
    let mut changed=false;
    settings_control_row(ui,title,80.0,|ui|{
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
            ui.allocate_ui_with_layout(egui::vec2(140.0,30.0),egui::Layout::left_to_right(egui::Align::Center),|ui|{
                changed=number_stepper(ui,value,min,max,suffix,N::from_f64(speed),76.0);
            });
        }).response
    });
    changed
}
fn number_stepper<N: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    value: &mut N,
    min: N,
    max: N,
    suffix: &str,
    step: N,
    field_width: f32,
) -> bool {
    let ctx = &ui.ctx().clone();
    let mut changed = false;
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.spacing_mut().button_padding = egui::vec2(4.0, 2.0);
        let button = |text| {
            egui::Button::new(egui::RichText::new(text).size(16.0).color(settings_color(
                ctx,
                Color32::WHITE,
                Color32::from_rgb(30, 34, 41),
            )))
            .fill(settings_color(
                ctx,
                Color32::from_rgb(48, 48, 51),
                Color32::from_rgb(229, 232, 238),
            ))
            .stroke(egui::Stroke::NONE)
            .corner_radius(13.0)
        };
        let decrease = ui
            .add_enabled(*value > min, button("−").min_size(egui::vec2(26.0, 26.0)))
            .on_hover_text("減少");
        if decrease.clicked() {
            *value = N::from_f64((value.to_f64() - step.to_f64()).max(min.to_f64()));
            changed = true;
        }
        let field = ui.add_sized(
            [field_width, 28.0],
            egui::DragValue::new(value)
                .range(min..=max)
                .speed(step.to_f64())
                .suffix(suffix)
                .fixed_decimals(if N::INTEGRAL { 0 } else { 2 }),
        );
        changed |= field.changed();
        if ui
            .add_enabled(*value < max, button("+").min_size(egui::vec2(26.0, 26.0)))
            .on_hover_text("增加")
            .clicked()
        {
            *value = N::from_f64((value.to_f64() + step.to_f64()).min(max.to_f64()));
            changed = true;
        }
    });
    changed
}
/// 時段行的寬度：兩個加減器（−、輸入框、+ 各間隔 6）加中間的「至」和兩側間距。
/// 區域剛好裝下內容，靠右放置時右邊緣就和單個加減器對齊。
const TIME_RANGE_HOURS_W: f32 = (26.0 + 6.0 + 48.0 + 6.0 + 26.0) * 2.0 + 8.0 + 13.0 + 8.0;
const TIME_RANGE_CLOCK_W: f32 = (26.0 + 6.0 + 56.0 + 6.0 + 26.0) * 2.0 + 8.0 + 13.0 + 8.0;
/// 時刻加減器（一天中的分鐘數，15 分鐘一檔，跨午夜循環），中間可直接輸入 12:30。
pub(crate) fn time_stepper(ui:&mut egui::Ui,value:&mut u32,id:&str)->bool{
    let ctx=&ui.ctx().clone();
    let mut changed=false;
    ui.push_id(id,|ui|{
        ui.spacing_mut().item_spacing.x=6.0;
        ui.spacing_mut().button_padding=egui::vec2(4.0,2.0);
        let button=|text|egui::Button::new(egui::RichText::new(text).size(16.0).color(settings_color(ctx,Color32::WHITE,Color32::from_rgb(30,34,41))))
            .fill(settings_color(ctx,Color32::from_rgb(48,48,51),Color32::from_rgb(229,232,238))).stroke(egui::Stroke::NONE).corner_radius(13.0);
        if ui.add(button("−").min_size(egui::vec2(26.0,26.0))).on_hover_text("提前 15 分鐘").clicked(){*value=(*value+1440-15)%1440;changed=true;}
        let field=ui.add_sized([56.0,28.0],egui::DragValue::new(value).range(0..=1439).speed(1.0)
            .custom_formatter(|v,_|{let m=v as u32;format!("{:02}:{:02}",m/60,m%60)})
            .custom_parser(|t|{let (h,m)=t.trim().split_once([':','：'])?;Some((h.parse::<u32>().ok()?.min(23)*60+m.parse::<u32>().ok()?.min(59)) as f64)}));
        changed|=field.changed();
        if ui.add(button("+").min_size(egui::vec2(26.0,26.0))).on_hover_text("推後 15 分鐘").clicked(){*value=(*value+15)%1440;changed=true;}
    });
    changed
}
fn setting_choices(ui: &mut egui::Ui, title: &str, value: &mut u32, values: [u32; 3], unit: &str) {
    let labels = values.map(|v| format!("{v} {unit}"));
    let options: Vec<(u32, &str)> = values
        .iter()
        .zip(labels.iter())
        .map(|(v, l)| (*v, l.as_str()))
        .collect();
    settings_control_row_right(ui, title, |ui| {
        settings_segmented(ui, value, &options, 62.0);
    });
}
/// 設定頁分頁序號。島上入口和測試按序號跳轉。
pub(crate) const SETTINGS_TAB_APPEARANCE: usize = 13;
pub(crate) const SETTINGS_TAB_POSITION: usize = 14;
pub(crate) const SETTINGS_TAB_PAGES: usize = 10;
pub(crate) const SETTINGS_TAB_WIRELESS: usize = 16;
#[cfg_attr(not(feature="sdk-js"),allow(dead_code))]
pub(crate) const SETTINGS_TAB_PLUGINS: usize = 19;
#[cfg(feature="sdk-js")]
pub(crate) const SETTINGS_TAB_PLUGIN_DETAIL: usize = 20;
/// 側欄：一項就是一頁，不再有頁內子標籤。專注與健康（2、3）、待辦與置物（4、5）各合成一頁；
/// 插件和關於固定在左下角，上方一條細線和功能頁分開。
/// 第四列是這一項之前的分組標題。
const SETTINGS_GROUPS: [(&str,usize,usize,&str);16 + cfg!(feature="sdk-js") as usize] = [
    ("通用",0,0,"島仔"), ("外觀",SETTINGS_TAB_APPEARANCE,SETTINGS_TAB_APPEARANCE,""), ("位置",SETTINGS_TAB_POSITION,SETTINGS_TAB_POSITION,""), ("頁面管理",SETTINGS_TAB_PAGES,SETTINGS_TAB_PAGES,""),
    ("音樂",1,1,"功能"), ("專注與健康",2,3,""), ("待辦與置物",4,5,""), ("訊息提醒",6,6,""), ("信箱",7,7,""), ("行程",8,8,""), ("AI 額度",11,11,""), ("天氣",12,12,""), ("系統性能",15,15,""), ("無線裝置",SETTINGS_TAB_WIRELESS,SETTINGS_TAB_WIRELESS,""),
    ("贊助",SETTINGS_TAB_SUPPORT,SETTINGS_TAB_SUPPORT,""),
    ("關於",9,9,""),
    #[cfg(feature="sdk-js")] ("插件",SETTINGS_TAB_PLUGINS,SETTINGS_TAB_PLUGINS,""),
];
const SETTINGS_BOTTOM_ROWS: usize = if cfg!(feature="sdk-js") { 3 } else { 2 };
/// 分頁標題；合併頁（3→2、5→4）顯示合併後的名字。
fn settings_tab_title(tab: usize) -> &'static str {
    SETTINGS_GROUPS
        .iter()
        .find(|(_, start, end, _)| (*start..=*end).contains(&tab))
        .map_or("通用", |g| g.0)
}
#[cfg(test)]
pub(crate) fn settings_tab_group_for_tests(tab: usize) -> &'static str {
    settings_tab_title(tab)
}
#[cfg(test)]
pub(crate) fn settings_group_names_for_tests() -> Vec<&'static str> {
    SETTINGS_GROUPS.iter().map(|g| g.0).collect()
}

fn settings_sidebar_row(
    ui: &mut egui::Ui, name: &str, icon: egui::ImageSource<'static>, row_h: f32,
    selected: bool, wireless: bool,
    #[cfg(feature="sdk-js")] navigation_icon: Option<&crate::sdk_spike::local::NavigationIcon>,
) -> egui::Response {
    let ctx = ui.ctx().clone();
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), row_h), egui::Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), name));
    if selected || response.hovered() {
        let fill = if selected {
            settings_color(&ctx, Color32::from_rgb(43,43,46), Color32::from_rgb(214,221,232))
        } else {
            settings_color(&ctx, Color32::from_rgb(30,30,33), Color32::from_rgb(225,230,238))
        };
        ui.painter().rect_filled(rect, 7.0, fill);
    }
    let icon_bg = egui::Rect::from_center_size(egui::pos2(rect.left()+21.0,rect.center().y),egui::vec2(23.0,23.0));
    #[cfg(feature="sdk-js")]
    let use_default_icon = if let Some(navigation_icon) = navigation_icon {
        let painter = ui.painter();
        let [r, g, b] = navigation_icon.background();
        painter.rect_filled(icon_bg, 6.0, Color32::from_rgb(r, g, b));
        crate::plugin_canvas_ui::draw_navigation_icon(
            painter, icon_bg, navigation_icon.commands(), Color32::WHITE,
        );
        false
    } else {
        true
    };
    #[cfg(not(feature="sdk-js"))]
    let use_default_icon = true;
    if use_default_icon {
        if wireless { paint_wireless_settings_icon(ui, icon_bg, icon); }
        else { egui::Image::new(icon).paint_at(ui, icon_bg); }
    }
    let color = if selected {
        settings_color(&ctx, Color32::WHITE, Color32::from_rgb(30,34,41))
    } else {
        settings_color(&ctx, Color32::from_gray(225), Color32::from_gray(55))
    };
    ui.painter().with_clip_rect(rect.shrink2(egui::vec2(5.0,0.0))).text(
        egui::pos2(rect.left()+40.0,rect.center().y), egui::Align2::LEFT_CENTER, name,
        egui::FontId::proportional(13.0), color,
    );
    response
}

impl App {
    fn settings_sidebar_builtin(
        &mut self, ui: &mut egui::Ui, (name, start, end): (&str, usize, usize),
        row_h: f32, row_gap: f32,
    ) {
        let icon = match start {
            SETTINGS_TAB_WIRELESS=>egui::include_image!("../assets/device-icons/bluetooth-linear.svg"),
            0=>egui::include_image!("../assets/settings-icons/system/general.svg"),
            11=>egui::include_image!("../assets/settings-icons/system/quota.svg"),
            10=>egui::include_image!("../assets/settings-icons/system/pages.svg"),
            1=>egui::include_image!("../assets/settings-icons/system/music.svg"),
            2=>egui::include_image!("../assets/settings-icons/system/focus.svg"),
            4=>egui::include_image!("../assets/settings-icons/system/tasks.svg"),
            6=>egui::include_image!("../assets/settings-icons/system/notifications.svg"),
            15=>egui::include_image!("../assets/settings-icons/system/performance.svg"),
            12=>egui::include_image!("../assets/settings-icons/system/weather.svg"),
            7=>egui::include_image!("../assets/settings-icons/system/mail.svg"),
            8=>egui::include_image!("../assets/settings-icons/system/calendar.svg"),
            SETTINGS_TAB_APPEARANCE=>egui::include_image!("../assets/settings-icons/system/appearance.svg"),
            SETTINGS_TAB_POSITION=>egui::include_image!("../assets/settings-icons/system/position.svg"),
            SETTINGS_TAB_SUPPORT=>egui::include_image!("../assets/settings-icons/system/support.svg"),
            #[cfg(feature="sdk-js")]
            SETTINGS_TAB_PLUGINS=>egui::include_image!("../assets/settings-icons/system/shelf.svg"),
            _=>egui::include_image!("../assets/settings-icons/system/about.svg"),
        };
        #[cfg(feature="sdk-js")]
        let icon = settings_tab_module(start).and_then(|page|self.plugins.appearance_icon(page,true)).unwrap_or(icon);
        let selected = (start..=end).contains(&self.settings_tab);
        if settings_sidebar_row(
            ui, name, icon, row_h, selected, start == SETTINGS_TAB_WIRELESS,
            #[cfg(feature="sdk-js")]
            None,
        ).clicked() && !selected {
            self.settings_tab = start;
            #[cfg(feature="sdk-js")]
            { self.settings_plugin = None; }
        }
        ui.add_space(row_gap);
    }

    #[cfg(feature="sdk-js")]
    fn apply_settings_navigation(&mut self, route: crate::sdk_spike::local::SettingsNavigation) {
        use crate::sdk_spike::local::SettingsNavigation;
        match route {
            SettingsNavigation::Tab(tab) => {
                self.settings_tab = tab;
                self.settings_plugin = None;
            }
            SettingsNavigation::Plugin(id) => self.open_plugin_settings(&id),
            SettingsNavigation::OpenStatus(id) => self.show_status_page(&id),
        }
    }

    /// 功能頁頂部卡片：名稱、說明和開啟狀態。開關統一在「頁面管理」；關閉時這裡給出「開啟」入口。
    /// 返回功能是否開啟，調用方據此把下面的設定置灰。
    fn settings_feature_header(&mut self,ui:&mut egui::Ui,page:usize,description:&str,extra:impl FnOnce(&mut Self,&mut egui::Ui))->bool{
        let ctx=&ui.ctx().clone();
        let on=self.module_on(page);
        let (mut enable,mut open)=(false,false);
        settings_card(ui,|ui|{
            ui.horizontal(|ui|{
                let icon=settings_page_icon(page);
                #[cfg(feature="sdk-js")]
                let icon=self.plugins.appearance_icon(page,false).unwrap_or(icon);
                if page==crate::app::BLUETOOTH_PAGE {
                    let (rect,_)=ui.allocate_exact_size(egui::vec2(36.0,36.0),egui::Sense::hover());
                    paint_wireless_settings_icon(ui,rect,icon);
                }else{
                    ui.add(egui::Image::new(icon).fit_to_exact_size(egui::vec2(36.0,36.0)));
                }
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(
                        egui::RichText::new(PAGE_NAMES[page])
                            .size(15.0)
                            .strong()
                            .color(settings_color(
                                ctx,
                                Color32::WHITE,
                                Color32::from_rgb(30, 34, 41),
                            )),
                    );
                    ui.label(
                        egui::RichText::new(description)
                            .size(11.5)
                            .color(settings_color(
                                ctx,
                                Color32::from_gray(175),
                                Color32::from_gray(90),
                            )),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // 每個功能卡片同一個位置：開著時「打開」跳到島上對應頁面，關著時「開啟」。
                    if on {
                        open = ui
                            .button("開啟")
                            .on_hover_text(format!("在島上開啟「{}」", PAGE_NAMES[page]))
                            .clicked();
                        extra(self, ui);
                    } else {
                        enable = ui.button("開啟").clicked();
                    }
                    settings_state_chip(ui, on);
                });
            });
            if !on {
                ui.label(
                    egui::RichText::new(format!(
                        "「{}」已關閉，不會在後台同步或提醒",
                        PAGE_NAMES[page]
                    ))
                    .size(12.0)
                    .color(settings_color(
                        ctx,
                        Color32::from_rgb(238, 181, 91),
                        Color32::from_rgb(139, 88, 12),
                    )),
                );
            }
        });
        if enable {
            self.set_page_visible(page, true);
        }
        if open {
            self.settings_open = false;
            if page == crate::app::CALENDAR_PAGE {
                self.open_calendar(None);
            } else {
                if page == crate::app::SHELF_PAGE {
                    self.shelf_tab = 0;
                }
                self.show_page(page);
            }
        }
        self.module_on(page)
    }
    pub fn settings_ui(&mut self,ctx:&egui::Context,autostart:bool){
        // 設定彈窗獨立於頁面的灰顯區域。模塊關掉後撤下彈窗，避免繼續操作。
        if !self.module_on(crate::app::MAIL_PAGE) {self.mail.form.open=false;}
        if !self.module_on(crate::app::CALENDAR_PAGE) {self.calendar.form.open=false;}
        if !self.module_on(crate::app::QUOTA_PAGE)&&self.quota.connect_open {self.quota_close_connection();}
        if !self.settings_open{
            self.window_layout().close();
            release_support_codes(ctx);
            self.weather_source.close();
        }
        self.updates.poll(ctx);
        self.updates.dialog_ui(ctx);
        if !self.settings_open {
            return;
        }
        let id = egui::ViewportId::from_hash_of("daozai-settings");
        let focus_requested = std::mem::take(&mut self.settings_focus_requested);
        let opening = self.window_layout().open();
        ctx.show_viewport_immediate(id,egui::ViewportBuilder::default().with_title("鬼島設定").with_icon(theme::app_icon()).with_inner_size([690.0,560.0]).with_min_inner_size([600.0,450.0]).with_visible(false).with_active(false).with_window_level(egui::WindowLevel::Normal),|ctx,_|{
            let restored=opening && self.window_layout().restore(ctx);
            if !restored{self.window_layout().observe(ctx);}
            let dark=self.settings.settings_theme.dark(crate::settings_theme::system_dark(ctx));
            let original_style=ctx.style();
            let mut style=(*original_style).clone();
            style.visuals=if dark{egui::Visuals::dark()}else{egui::Visuals::light()};
            // 側欄與正文之間的豎線、彈窗裡的分隔線都用這條淡線；淺色模式不再是一條深灰粗線。
            // 側欄邊框已去掉，這裡是唯一的一條：淺色模式一條淡線，深色模式不畫線，靠兩塊底色區分。
            style.visuals.widgets.noninteractive.bg_stroke=if dark{egui::Stroke::NONE}else{egui::Stroke::new(1.0,Color32::from_rgb(226,229,234))};
            ctx.set_style(style);
            ctx.send_viewport_cmd(egui::ViewportCommand::SetTheme(if dark{egui::SystemTheme::Dark}else{egui::SystemTheme::Light}));
            if self.settings.settings_theme==crate::settings_theme::SettingsTheme::System {
                ctx.request_repaint_after(std::time::Duration::from_secs(1));
            }
            if focus_requested {
                // Glow 在首幀交換緩衝後執行窗口命令，避免先顯示白色空窗口。
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            if ctx.input(|i|i.viewport().close_requested()){self.settings_open=false;self.quota_close_connection();}
            let before=format!("{:?}",self.settings);
            #[cfg(feature="sdk-js")]
            let plugin_pages: Vec<_> = self.plugins.standalone_settings_pages()
                .map(|(id, name)| (id.to_owned(), name.to_owned())).collect();
            #[cfg(not(feature="sdk-js"))]
            let plugin_pages: &[(&str, &str)] = &[];
            #[cfg(feature="sdk-js")]
            if self.settings_tab == SETTINGS_TAB_PLUGIN_DETAIL
                && !plugin_pages.iter().any(|(id, _)| self.settings_plugin.as_ref() == Some(id))
            {
                self.settings_tab = SETTINGS_TAB_PLUGINS;
                self.settings_plugin = None;
            }
            egui::SidePanel::left("settings-sidebar").resizable(false).exact_width(180.0).frame(egui::Frame::new().fill(settings_color(ctx,Color32::from_rgb(29,29,32),Color32::from_rgb(237,239,243))).stroke(egui::Stroke::NONE).inner_margin(egui::Margin::symmetric(10,12))).show(ctx,|ui|{
                ui.add_space(2.0);
                let per=(ui.available_height()-80.0)/(SETTINGS_GROUPS.len()+plugin_pages.len()) as f32;
                let row_h=(per-6.0).clamp(20.0,32.0);
                let row_gap=(per-3.0-row_h).clamp(0.0,3.0);
                let (main, bottom) = SETTINGS_GROUPS.split_at(SETTINGS_GROUPS.len()-SETTINGS_BOTTOM_ROWS);
                let footer_height = bottom.len() as f32*(row_h+row_gap+ui.spacing().item_spacing.y)+17.0;
                let main_height = (ui.available_height()-footer_height).max(0.0);
                egui::ScrollArea::vertical().id_salt("settings-sidebar-scroll").max_height(main_height)
                    .auto_shrink([false,false]).show(ui, |ui| {
                        for &(name, start, end, section) in main {
                            if start == SETTINGS_TAB_SUPPORT && !support_page_enabled() { continue; }
                            if !section.is_empty() {
                                ui.add_space(if start == 0 { 2.0 } else { 8.0 });
                                let color = settings_color(ui.ctx(), Color32::from_gray(150), Color32::from_gray(105));
                                ui.label(egui::RichText::new(section).size(11.0).color(color));
                                ui.add_space(3.0);
                            }
                            #[cfg(feature="sdk-js")]
                            if section == "功能" {
                                for (id, name) in &plugin_pages {
                                    let selected = self.settings_tab == SETTINGS_TAB_PLUGIN_DETAIL
                                        && self.settings_plugin.as_ref() == Some(id);
                                    let icon = egui::include_image!("../assets/settings-icons/system/pages.svg");
                                    let response = ui.push_id(("plugin-settings-sidebar", id), |ui| {
                                        settings_sidebar_row(
                                            ui, name, icon, row_h, selected, false,
                                            self.plugins.navigation_icon(id),
                                        ).on_hover_text(name)
                                    }).inner;
                                    if selected && focus_requested && !ui.clip_rect().contains_rect(response.rect) {
                                        response.scroll_to_me(Some(egui::Align::Center));
                                    }
                                    if response.clicked() && !selected { self.open_plugin_settings(id); }
                                    ui.add_space(row_gap);
                                }
                            }
                            self.settings_sidebar_builtin(ui, (name, start, end), row_h, row_gap);
                        }
                    });
                ui.add_space(4.0);
                ui.painter().hline(ui.max_rect().x_range().shrink(4.0), ui.cursor().top(), egui::Stroke::new(1.0,
                    settings_color(ctx, Color32::from_rgb(46,46,51), Color32::from_rgb(218,222,229))));
                ui.add_space(9.0);
                for &(name, start, end, _) in bottom {
                    if start == SETTINGS_TAB_SUPPORT && !support_page_enabled() { continue; }
                    self.settings_sidebar_builtin(ui, (name, start, end), row_h, row_gap);
                }
            });
            egui::CentralPanel::default().frame(egui::Frame::new().fill(settings_color(ctx,Color32::from_rgb(25,25,27),Color32::from_rgb(246,247,249))).inner_margin(egui::Margin::symmetric(22,16))).show(ctx,|ui|{
                settings_apply_style(ui);
                #[cfg(feature="sdk-js")]
                let plugin = plugin_pages.iter().find(|(id, _)| {
                    self.settings_tab == SETTINGS_TAB_PLUGIN_DETAIL && self.settings_plugin.as_ref() == Some(id)
                });
                let title = settings_tab_title(self.settings_tab);
                #[cfg(feature="sdk-js")]
                let title = plugin.map_or(title, |(_, name)| name.as_str());
                ui.horizontal(|ui|{
                    let label = egui::RichText::new(title).size(18.0).strong()
                        .color(settings_color(ctx,Color32::WHITE,Color32::from_rgb(30,34,41)));
                    #[cfg(feature="sdk-js")]
                    if let Some((id, _)) = plugin {
                        let can_open = self.plugins.status_pages().any(|(page, _)| page == id);
                        let title_width = (ui.available_width() - if can_open { 106.0 } else { 0.0 }).max(90.0);
                        ui.allocate_ui_with_layout(egui::vec2(title_width, 24.0),
                            egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                ui.scope(|ui| {
                                    ui.set_max_width((title_width - 58.0).max(20.0));
                                    ui.add(egui::Label::new(label).truncate()).on_hover_text(title);
                                });
                                crate::sdk_spike::local::plugin_tag(ui);
                            });
                        if can_open {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button("打開功能頁").clicked() {
                                    self.settings_open = false;
                                    self.show_status_page(id);
                                }
                            });
                        }
                        return;
                    }
                    ui.label(label);
                    // 插件頁：「集市 / 已安裝」切換放在標題同一行的右側。
                    #[cfg(feature="sdk-js")]
                    if self.settings_tab==SETTINGS_TAB_PLUGINS {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|self.plugins.header_tabs(ui));
                    }
                });
                ui.add_space(6.0);
                // 合併頁：健康（3）併入專注（2），置物（4→5）併入待辦，舊序號仍能打開對應頁面。
                let tab=match self.settings_tab {3=>2,5=>4,t=>t};
                let scroll = egui::ScrollArea::vertical().auto_shrink([false,false]);
                #[cfg(feature="sdk-js")]
                let scroll = if let Some((id, _)) = plugin { scroll.id_salt(("settings-plugin-scroll", id)) }
                    else { scroll.id_salt(("settings-scroll",tab)) };
                #[cfg(not(feature="sdk-js"))]
                let scroll = scroll.id_salt(("settings-scroll",tab));
                scroll.show(ui,|ui|{
                    // 右側給滾動條留出空隙，卡片和開關不會被它壓住。
                    ui.set_width((ui.available_width()-12.0).max(240.0));
                    #[cfg(feature="sdk-js")]
                    if let Some((id, _)) = plugin {
                        self.plugins.standalone_settings(ui, id);
                        return;
                    }
                    #[cfg(any(feature="sdk-js",feature="sdk-wasm"))]
                    if tab==4 {crate::sdk_spike::live_settings(ui);}
                    match tab {
                    15=>{
                        let on=self.settings_feature_header(ui,crate::app::PERFORMANCE_PAGE,"查看硬件負載和實時吞吐量",|_,_|{});
                        ui.add_enabled_ui(on,|ui|self.performance_settings(ui));
                    }
                    SETTINGS_TAB_WIRELESS=>{
                        let on=self.settings_feature_header(ui,crate::app::BLUETOOTH_PAGE,"查看藍牙電量和 2.4G 接收器狀態",|_,_|{});
                        #[cfg(all(windows,feature="sdk-js"))]
                        self.plugins.wireless_settings(ui);
                        ui.add_enabled_ui(on,|ui|{
                            settings_label(ui,"島上常駐");
                            settings_card(ui,|ui|{
                                resident_setting_row(ui,"無線裝置電量","收起時顯示，最多兩台",on,&mut self.settings.show_bluetooth);
                                if self.settings.show_bluetooth {
                                    settings_separator(ui);
                                    self.wireless_resident_settings(ui);
                                }
                            });
                            settings_label(ui,"電量提醒");
                            settings_card(ui,|ui|{
                                setting_row(ui,"島上提示","接上電源、充滿或電量偏低時提醒",&mut self.settings.bluetooth_battery_notifications);
                            });
                        });
                    }
                    #[cfg(feature="sdk-js")]
                    SETTINGS_TAB_PLUGINS=>{
                        let on=self.module_on(crate::app::TODO_PAGE);
                        let calendar_on=self.module_on(crate::app::CALENDAR_PAGE);
                        let quota_on=self.quota_active();
                        if let Some(route)=self.plugins.manage(ui,on,calendar_on,quota_on,
                            #[cfg(all(windows,feature="sdk-material"))] self.material_active_id.as_deref().filter(|_| self.material_capture_forced),
                        ){self.apply_settings_navigation(route);}
                    }
                    12=>{
                        let on=self.settings_feature_header(ui,crate::app::WEATHER_PAGE,"當地天氣與七日預報",|_,_|{});
                        ui.add_space(4.0);
                        ui.add_enabled_ui(on,|ui|self.weather_settings(ui));
                    }
                    11=>{
                        let on=self.settings_feature_header(ui,crate::app::QUOTA_PAGE,"每 3 分鐘自動更新剩餘額度",|_,_|{});
                        ui.add_space(4.0);
                        ui.add_enabled_ui(on,|ui|{
                            self.quota_settings(ui);
                            #[cfg(feature="sdk-js")]
                            self.plugins.settings(ui,"quota",on);
                        });
                    }
                    SETTINGS_TAB_PAGES=>{
                        ui.label(egui::RichText::new("選擇需要的頁面和狀態組件。").size(12.0).color(settings_color(ctx,Color32::from_gray(180),Color32::from_gray(75))));
                        // 島上顯示：收起時的常駐內容在這裡統一排序和開關；島上歌詞屬於音樂的顯示方式，在音樂頁設定。
                        settings_label(ui,"島上顯示");
                        settings_card(ui,|ui|{
                            self.resident_order_settings(ui);
                            ui.separator();
                            setting_row(ui,"時間","關閉後仍可在展開頂欄檢視",&mut self.settings.show_clock);
                            ui.separator();
                            setting_row(ui,"電量","系統電池優先；桌機改顯示已連線手機的電量，手機沒回報才用藍牙耳機、音箱，都沒有才自動隱藏",&mut self.settings.show_battery);
                            ui.separator();
                            resident_setting_row(ui,"天氣","天氣圖標和溫度，點擊打開天氣頁",
                                self.module_on(crate::app::WEATHER_PAGE),&mut self.settings.show_weather);
                            ui.separator();
                            resident_setting_row(ui,"無線裝置電量","收起時顯示，最多兩台",
                                self.module_on(crate::app::BLUETOOTH_PAGE),&mut self.settings.show_bluetooth);
                            ui.separator();
                            ui.add_enabled_ui(self.module_on(crate::app::PERFORMANCE_PAGE),|ui|setting_row(ui,"系統性能","點擊常駐指標打開性能頁",&mut self.settings.show_performance));
                            ui.separator();
                            let before=self.quota.config.compact;
                            resident_setting_row(ui,"AI 額度","常駐帳號，WorkBuddy 積分在前，DeepSeek 餘額在最後",
                                self.module_on(crate::app::QUOTA_PAGE),&mut self.quota.config.compact);
                            if before!=self.quota.config.compact {self.quota.config.save();}
                        });
                        settings_label(ui,"功能頁面");
                        settings_card(ui,|ui|{
                            #[cfg(feature = "sdk-js")]
                            if self.plugins.page_shortcut_settings(ui,&mut self.settings.show_page_shortcuts) {
                                ui.separator();
                            }
                            let descriptions=["播放控制與同步歌詞","番茄鐘與休息計時","喝水與久坐提醒","臨時收納檔案位置","任務清單與到期提醒","信箱摘要、驗證碼與同一組 Google 帳號的行程","Agent 剩餘額度和帳號切換","行事曆、郵件邀請與工作行程","當地天氣與七日預報","處理器、顯卡、記憶體、網路與磁碟","藍牙裝置連線狀態與多來源電量","小遊戲與休閒入口","電池與無線裝置電量總覽"];
                            ui.label(egui::RichText::new("拖動左側手柄調整順序；關閉的功能不在後台運行，不同步也不提醒，已有數據保留。").size(11.5).color(settings_color(ctx,Color32::from_gray(180),Color32::from_gray(75))));
                            #[derive(Clone)]
                            struct PageDrag(usize);
                            let mut movement=None;
                            let mut jump=None;
                            let row_w=ui.available_width();
                            for (index,page) in self.settings.page_order.into_iter().enumerate() {
                                // 行程已併進信箱與行程頁，不再單獨列出。
                                if page==crate::app::CALENDAR_PAGE {continue;}
                                if index>0 {ui.separator();}
                                let mut visible=self.settings.visible_pages[page];
                                let can_change=!visible || self.settings.visible_pages.iter().filter(|enabled|**enabled).count()>1;
                                let row=ui.push_id(("page-order",page),|ui|ui.horizontal(|ui|{
                                    ui.set_max_width(row_w);
                                    let (rect,handle)=ui.allocate_exact_size(egui::vec2(20.0,40.0),egui::Sense::drag());
                                    for x in [-3.0,3.0] {for y in [-6.0,0.0,6.0] {
                                        ui.painter().circle_filled(rect.center()+egui::vec2(x,y),1.4,settings_color(ctx,Color32::from_gray(155),Color32::from_gray(100)));
                                    }}
                                    handle.on_hover_cursor(egui::CursorIcon::Grab).dnd_set_drag_payload(PageDrag(page));
                                    // 每行右側「設定 ›」直接跳到這個功能的設定頁，開關仍在最右。
                                    let jump_w=58.0;
                                    ui.allocate_ui(egui::vec2((ui.available_width()-jump_w-ui.spacing().item_spacing.x).max(160.0),40.0),|ui|{
                                        ui.add_enabled_ui(can_change,|ui|{setting_row(ui,PAGE_NAMES[page],descriptions[page],&mut visible);});
                                    });
                                    if ui.add_sized([jump_w,26.0],egui::Button::new(egui::RichText::new("設定 ›").size(12.0)).frame(false)).on_hover_text(format!("開啟「{}」設定",PAGE_NAMES[page])).clicked(){jump=Some(crate::app::PAGE_SETTINGS_TABS[page]);}
                                })).inner.response;
                                if let Some(payload)=row.dnd_hover_payload::<PageDrag>() {
                                    if payload.0!=page {
                                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                                        let y=if self.settings.page_order.iter().position(|p|*p==payload.0).unwrap()<index {row.rect.bottom()}else{row.rect.top()};
                                        ui.painter().line_segment([egui::pos2(row.rect.left(),y),egui::pos2(row.rect.right(),y)],egui::Stroke::new(2.0,Color32::from_rgb(70,150,255)));
                                    }
                                }
                                if let Some(payload)=row.dnd_release_payload::<PageDrag>() {movement=Some((payload.0,index));}
                                if visible!=self.settings.visible_pages[page] {self.set_page_visible(page,visible);}
                            }
                            if let Some(tab)=jump {self.settings_tab=tab;}
                            if let Some((page,to))=movement {
                                let from=self.settings.page_order.iter().position(|p|*p==page).unwrap();
                                if from<to {self.settings.page_order[from..=to].rotate_left(1);}
                                else {self.settings.page_order[to..=from].rotate_right(1);}
                            }
                            if self.settings.visible_pages.iter().filter(|enabled|**enabled).count()==1 {
                                ui.add_space(5.0);
                                ui.label(egui::RichText::new("至少保留一個頁面。").size(11.0).color(settings_color(ctx,Color32::from_gray(180),Color32::from_gray(75))));
                            }
                        });
                        #[cfg(feature="sdk-js")]
                        if let Some(route)=self.plugins.status_page_settings(ui) {self.apply_settings_navigation(route);}
                    }
                    0=>{
                        settings_label(ui,"語言");
                        settings_card(ui,|ui|{
                            settings_control_row_right(ui,"介面語言",|ui|{
                                // 目前只有繁體中文；英文名稱接在後面，之後新增語言直接加進同一份清單。
                                let mut language="zh-TW";
                                egui::ComboBox::from_id_salt("tw-language").selected_text("繁體中文  Traditional Chinese").width(220.0).show_ui(ui,|ui|{
                                    ui.selectable_value(&mut language,"zh-TW","繁體中文  Traditional Chinese");
                                });
                            });
                            settings_hint(ui,"目前提供繁體中文，後方保留英文名稱供辨識。");
                        });
                        settings_label(ui,"設定頁外觀");
                        settings_card(ui,|ui|{
                            use crate::settings_theme::SettingsTheme;
                            settings_control_row_right(ui,"主題",|ui|{
                                if settings_segmented(ui,&mut self.settings.settings_theme,&[(SettingsTheme::System,"跟隨系統"),(SettingsTheme::Light,"淺色"),(SettingsTheme::Dark,"深色")],52.0) {ctx.request_repaint();}
                            });
                            settings_hint(ui,"僅影響設定頁，島的外觀保持不變。");
                        });
                        settings_label(ui,"時間日期格式");
                        settings_card(ui,|ui|{
                            settings_control_row_right(ui,"時間制式",|ui|{settings_segmented(ui,&mut self.settings.clock_12h,&[(false,"24 小時"),(true,"12 小時")],72.0);});
                            settings_control_row_right(ui,"時段文字",|ui|{settings_segmented(ui,&mut self.settings.clock_period,&[(0,"不顯示"),(1,"上午 / 下午"),(2,"詳細時段")],72.0);});
                            if self.settings.clock_period==2 {settings_hint(ui,"詳細時段：凌晨、上午、中午、下午、傍晚、晚上");}
                            ui.separator();
                            setting_row(ui,"顯示秒數","時間顯示為 14:32:08",&mut self.settings.clock_seconds);
                            ui.separator();
                            let mut date_modes=|ui:&mut egui::Ui|{
                                use crate::model::ResidentDate;
                                let modes=[
                                    (ResidentDate::Off,"關閉"),
                                    (ResidentDate::LunarOnly,"僅農曆"),
                                    (ResidentDate::Gregorian,"僅公曆"),
                                    (ResidentDate::Lunar,"公曆＋農曆"),
                                ];
                                if settings_segmented(ui,&mut self.settings.resident_date,&modes,52.0) {
                                    ctx.request_repaint_of(egui::ViewportId::ROOT);
                                }
                            };
                            if ui.available_width()<360.0 {
                                ui.label("日期常駐");
                                date_modes(ui);
                            } else {
                                settings_control_row_right(ui,"日期常駐",date_modes);
                            }
                        });
                        settings_label(ui,"啟動");
                        settings_card(ui,|ui|{
                            let mut a=autostart;
                            setting_row(ui,"開機啟動","",&mut a);
                            if a!=autostart{self.pending.push(Cmd::ToggleAutostart);}
                            ui.separator();
                            setting_row(ui,"全屏時隱藏","全屏隱藏島仔，健康提醒靜默保留",&mut self.settings.hide_fullscreen);
                            ui.separator();
                            #[cfg(windows)]
                            let supported = crate::native::Window::capture_exclusion_supported();
                            #[cfg(not(windows))]
                            let supported = false;
                            #[cfg(all(windows,feature="sdk-material"))]
                            let material_forced = self.material_capture_forced;
                            #[cfg(not(all(windows,feature="sdk-material")))]
                            let material_forced = false;
                            let mut enabled = self.settings.exclude_from_capture || material_forced;
                            ui.add_enabled_ui(supported && !material_forced, |ui| {
                                setting_row(ui, "在截圖和錄屏中隱藏島仔", "桌面上照常顯示，支援的截圖和錄屏工具會略過島仔。", &mut enabled);
                            });
                            if material_forced {
                                settings_hint(ui,"材質生效時需隱藏島仔自身，才能讀取它背後的畫面。停用材質後恢復原設定。");
                            } else if enabled != self.settings.exclude_from_capture {
                                self.capture_exclusion_request = Some(enabled);
                                self.capture_exclusion_error = None;
                            }
                            let error = self.capture_exclusion_error.as_deref()
                                .or((!supported).then_some("當前系統不支援在截圖和錄屏中隱藏島仔。"));
                            if let Some(error) = error {
                                ui.add(egui::Label::new(egui::RichText::new(error).size(11.5)
                                    .color(settings_color(ctx, Color32::from_rgb(238,181,91), Color32::from_rgb(139,88,12)))).wrap());
                            }
                        });
                        self.avoidance.settings(ui,&mut self.settings);
                        ui.add_space(8.0);
                        settings_card(ui,|ui|{
                            settings_advanced(ui,"settings-general-advanced",|ui|{
                                let before=self.settings.render_compat;
                                setting_row(ui,"相容渲染（Direct3D 12）","出現黑框、閃爍或開啟設定後崩潰時開啟；切換後重啟鬼島生效",&mut self.settings.render_compat);
                                // 手動關閉後不再顯示自動切換的原因；再次開啟視為手動開啟。
                                if before!=self.settings.render_compat {self.settings.render_compat_reason.clear();}
                                if !self.settings.render_compat_reason.is_empty() {settings_hint(ui,&self.settings.render_compat_reason);}
                            });
                            // 待重啟的提示不折疊，收起「高級」也能看到。
                            if self.settings.render_compat!=self.render_compat_running {
                                ui.horizontal(|ui|{
                                    ui.label(egui::RichText::new("相容渲染重啟鬼島後生效").size(12.0).color(Color32::from_rgb(230,150,40)));
                                    if ui.button("立即重啟").clicked(){self.pending.push(Cmd::RestartApp);}
                                });
                            }
                        });
                    }
                    SETTINGS_TAB_APPEARANCE=>{
                        settings_label(ui,"島體外觀");
                        settings_card(ui,|ui|{
                            use crate::model::{self as model,Skin};
                            #[cfg(feature="sdk-js")]
                            let plugin_picker=self.plugins.appearance_picker(ui,crate::sdk_spike::local::appearance_assets::Kind::Skin,Some(&mut self.settings.skin));
                            #[cfg(not(feature="sdk-js"))]
                            let plugin_picker=false;
                            if !plugin_picker {
                                let skins:Vec<(Skin,&str)>=Skin::ALL.iter().map(|s|(*s,s.label())).collect();
                                settings_control_row_right(ui,"皮膚",|ui|{settings_segmented(ui,&mut self.settings.skin,&skins,64.0);});
                            }
                            settings_hint(ui,self.effective_skin().description());
                            if self.effective_skin().is_gradient() {
                                let mut on=self.settings.gradient_brightness_on;
                                setting_row(ui,"調整漸層亮度","關閉時漸層保持原始亮度（100%），打開後可往下調暗",&mut on);
                                self.settings.gradient_brightness_on=on;
                                ui.add_enabled_ui(on,|ui|{
                                    settings_control_row(ui,"亮度",56.0,|ui|{
                                        ui.horizontal_wrapped(|ui|{
                                            ui.spacing_mut().slider_width=(ui.available_width()-96.0).clamp(64.0,180.0);
                                            ui.add(egui::Slider::new(&mut self.settings.gradient_brightness,60..=100).suffix("%").show_value(false));
                                            ui.label(format!("{}%",self.settings.gradient_brightness));
                                            if ui.add_enabled(self.settings.gradient_brightness!=80,egui::Button::new("恢復預設")).clicked(){self.settings.gradient_brightness=80;}
                                        }).response
                                    });
                                });
                                settings_hint(ui,"只調漸層底色深淺，文字與圖示亮度不變；60% 最深、100% 最亮。");
                            }
                            if self.effective_skin().is_glass() {
                                let mut blur=self.settings.glass_blur;
                                setting_row(ui,"背景模糊","需開啟系統“透明效果”；關閉後改為煙燻半透明",&mut blur);
                                self.settings.glass_blur=blur;
                            }
                            #[cfg(all(windows,feature="sdk-js",feature="sdk-material"))]
                            {
                                let active=self.material_active_id.as_deref().filter(|_| self.material_capture_forced);
                                if self.plugins.material_settings(ui,active,self.settings.render_compat) {
                                    self.settings.render_compat=true;
                                    self.settings.render_compat_reason.clear();
                                    self.pending.push(Cmd::RestartApp);
                                    ctx.request_repaint_of(egui::ViewportId::ROOT);
                                }
                            }
                            #[cfg(feature="sdk-js")]
                            self.plugins.appearance_picker(ui,crate::sdk_spike::local::appearance_assets::Kind::Icon,None);
                            ui.separator();
                            let shapes:Vec<(model::IslandShape,&str)>=model::IslandShape::ALL.iter().map(|s|(*s,s.label())).collect();
                            settings_control_row_right(ui,"形狀",|ui|{settings_segmented(ui,&mut self.settings.island_shape,&shapes,64.0);});
                            settings_hint(ui,self.settings.island_shape.description());
                            ui.separator();
                            settings_control_row(ui,"大小",56.0,|ui|{
                                ui.horizontal_wrapped(|ui|{
                                    // 給百分比和恢復按鈕留出空間，寬窗口仍使用原來的軌道長度。
                                    ui.spacing_mut().slider_width=(ui.available_width()-150.0).clamp(64.0,180.0);
                                    island_size_slider(ui,&mut self.settings.island_size);
                                    if ui.add_enabled(self.settings.island_size!=crate::model::ISLAND_SIZE_DEFAULT,egui::Button::new("恢復預設")).clicked(){self.settings.island_size=crate::model::ISLAND_SIZE_DEFAULT;}
                                }).response
                            });
                            settings_hint(ui,"隻影響島體，放大後文字和圖示依舊清晰。建議 80%，最大 200%。");
                        });
                        settings_label(ui,"島體顯示");
                        settings_card(ui,|ui|{
                            let mut minimal=self.settings.minimal;
                            setting_row(ui,"極簡劉海","收起時顯示一條細線",&mut minimal);
                            if minimal!=self.settings.minimal {self.set_minimal(minimal);}
                            ui.separator();
                            let mut pet=self.settings.pet;
                            setting_row(ui,"鬼島吉祥物","收起時常駐，播放音樂時一起唱歌",&mut pet);
                            if pet!=self.settings.pet {self.set_pet(pet);}
                            #[cfg(feature="sdk-js")]
                            self.plugins.appearance_picker(ui,crate::sdk_spike::local::appearance_assets::Kind::Pet,None);
                            ui.separator();
                            ui.horizontal(|ui|{
                                ui.label("吉祥物形象");
                                ui.label(egui::RichText::new("島虎（石虎）").strong());
                            });
                            settings_hint(ui,"台灣版只保留島虎。經典、奶油、毛玻璃、懸浮雲與懸浮漸層都使用同一組台灣石虎影格；漸層只換島體底色，吉祥物不會變形狀。");
                            ui.horizontal(|ui|{
                                ui.label("吉祥物動畫包");
                                if ui.button("開啟資料夾").clicked(){crate::mascot_pack::open_folder(self.settings.mascot.key());}
                                if ui.button("匯入動畫包…").clicked(){
                                    match crate::system::pick_folder_for("選擇吉祥物動畫包資料夾（內含 pack.json）") {
                                        Ok(Some(dir))=>match crate::mascot_pack::import_folder(self.settings.mascot.key(),&dir){
                                            Ok(())=>{self.pet.reload_mascot_pack();self.say("吉祥物動畫包已匯入，重新載入完成");}
                                            Err(error)=>self.say(format!("匯入失敗：{error}")),
                                        },
                                        Ok(None)=>{},
                                        Err(error)=>self.say(format!("無法開啟資料夾選擇視窗：{error}")),
                                    }
                                }
                                if ui.button("重新載入").clicked(){self.pet.reload_mascot_pack();}
                            });
                            settings_hint(ui,"動畫包不放也可以，會用內建動畫；自行繪製的影格建議 144×90、透明背景。");
                            settings_advanced(ui,"settings-appearance-advanced",|ui|{
                                setting_row(ui,"OLED 畫素偏移","防止 OLED 螢幕燒屏：收起靜止時讓島體緩慢移動幾個畫素，不改動你儲存的位置",&mut self.settings.oled_shift);
                            });
                        });
                    }
                    SETTINGS_TAB_POSITION=>{
                        settings_card(ui,|ui|{
                            let selected=self.displays.iter().find(|d|d.id==self.settings.display_id);
                            let label=if self.settings.display_id.is_empty(){"跟隨主屏"}else{selected.map_or("所選螢幕未連線",|d|d.label.as_str())};
                            settings_control_row(ui,"顯示屏",56.0,|ui|{
                                egui::ComboBox::from_id_salt("island-display").selected_text(label).width(ui.available_width().min(300.0)).truncate().show_ui(ui,|ui|{
                                    ui.selectable_value(&mut self.settings.display_id,String::new(),"跟隨主屏");
                                    for display in &self.displays {ui.selectable_value(&mut self.settings.display_id,display.id.clone(),&display.label);}
                                }).response
                            });
                            // 只在所選螢幕斷開時說明，平時不佔位置。
                            if !self.settings.display_id.is_empty()&&selected.is_none(){settings_hint(ui,"暫時顯示在主屏，重新連線所選螢幕後自動恢復。");}
                            ui.separator();
                            // 停靠邊緣：上下為橫向；左右為豎向排列，展開面板從邊緣滑出。
                            let docks:Vec<(crate::model::Dock,&str)>=crate::model::Dock::ALL.iter().map(|d|(*d,d.label())).collect();
                            settings_control_row_right(ui,"停靠",|ui|{
                                // 換邊後偏移的含義變了（水平 ↔ 上下），回到居中。
                                if settings_segmented(ui,&mut self.settings.dock,&docks,64.0) {self.settings.island_offset=0;}
                            });
                            if self.settings.dock.is_side() {settings_hint(ui,"左右停靠時豎向排列；吉祥物和島上歌詞在頂部或底部停靠時顯示。");}
                            let side=self.settings.dock.is_side();
                            settings_control_row(ui,if side{"上下位置"}else{"水平位置"},56.0,|ui|{
                                ui.horizontal_wrapped(|ui|{
                                    let limit=crate::model::ISLAND_OFFSET_MAX;
                                    let offset=&mut self.settings.island_offset;
                                    let (less,more,less_tip,more_tip)=if side{("↑","↓","向上微調","向下微調")}else{("←","→","向左微調","向右微調")};
                                    if ui.add_enabled(*offset>-limit,egui::Button::new(less).min_size(egui::vec2(26.0,22.0))).on_hover_text(less_tip).clicked(){*offset=(*offset-5).max(-limit);}
                                    // 左側微調按鈕已佔位，剩餘寬度還要容納右側微調和恢復居中。
                                    ui.spacing_mut().slider_width=(ui.available_size_before_wrap().x-134.0).clamp(64.0,150.0);
                                    ui.add(egui::Slider::new(&mut *offset,-limit..=limit).show_value(false));
                                    if ui.add_enabled(*offset<limit,egui::Button::new(more).min_size(egui::vec2(26.0,22.0))).on_hover_text(more_tip).clicked(){*offset=(*offset+5).min(limit);}
                                    if ui.add_enabled(*offset!=0,egui::Button::new("恢復居中")).clicked(){*offset=0;}
                                }).response
                            });
                            let offset=self.settings.island_offset;
                            let (less,more)=if side{("偏上","偏下")}else{("偏左","偏右")};
                            let position=match offset {0=>"居中".to_string(),o if o<0=>format!("{less} {}%",-o),o=>format!("{more} {o}%")};
                            settings_hint(ui,&format!("當前：{position}。也可以展開後按住日期時間直接拖動。"));
                            ui.separator();
                            setting_row(ui,"頂欄適配","頂部有選單欄或工作列時使用",&mut self.settings.avoid_top_bar);
                            if self.settings.avoid_top_bar {
                                settings_control_row_right(ui,"顯示方式",|ui|{
                                    settings_segmented(ui,&mut self.settings.top_bar_on_top,&[(false,"避開頂欄"),(true,"鬼島置頂")],90.0);
                                });
                                if self.settings.top_bar_on_top {
                                    settings_hint(ui,"島體顯示在頂欄上方。");
                                } else {
                                    let reserved=crate::display_layout::selected(&self.displays,&self.settings.display_id).map_or(0,|d|{
                                        let b=crate::display_layout::placement_bounds(d,true);(b.top-d.bounds.top).max(0)
                                    });
                                    settings_hint(ui,&if reserved>0 {format!("已檢測到頂欄，島體下移 {reserved} 畫素。")}else{"未檢測到頂欄；沒有向系統登記的頂欄可嘗試“鬼島置頂”。".to_string()});
                                }
                            }
                        });
                    }
                    1=>{
                        let on=self.settings_feature_header(ui,crate::app::MUSIC_PAGE,"播放控制與同步歌詞",|_,_|{});
                        ui.add_enabled_ui(on,|ui|{
                            settings_label(ui,"歌詞");
                            settings_card(ui,|ui|{
                                setting_row(ui,"島上歌詞","播放時收起的島顯示當前句，其他常駐圖示暫時隱藏；無同步歌詞時顯示封面和歌名",&mut self.settings.island_lyrics);
                                ui.separator();
                                setting_row(ui,"展開時顯示歌詞","在播放器右側同步滾動",&mut self.settings.lyrics_enabled);
                                self.show_lyrics=self.settings.lyrics_enabled;
                                ui.separator();
                                let mut advance=-self.settings.lyric_delay_ms as f64/1000.0;
                                if setting_number(ui,"時間偏移",&mut advance,-5.0,5.0," 秒",0.1){self.settings.lyric_delay_ms=(-advance*1000.0).round()as i32;}
                                settings_hint(ui,"歌詞和歌聲對不上時調整：正數提前，負數延後；預設提前 0.35 秒。");
                                settings_hint(ui,"查詞為自動進行：播放器本機歌詞 → KKBOX → 網易雲 → QQ 音樂 → LRCLIB；找不到時按播放器上的「詞」可重試。");
                                settings_hint(ui,"Chrome、Edge、Firefox 播 YouTube／YouTube Music 時，會從 Windows 右下角播放卡自動讀取歌名；瀏覽器縮小也持續顯示，不需要安裝擴充功能。");
                                settings_hint(ui,"YouTube 標題常混著頻道名、「Official Video」與「- YouTube」；抓詞前會先拆成歌名與歌手，再逐組比對同步歌詞。");
                                ui.separator();
                                setting_row(ui,"聽歌辨識","瀏覽器播放時直接聽電腦正在輸出的聲音認歌；影片標題不準也能抓到正確歌名，並依辨識到的段落對上歌詞", &mut self.settings.listen_id);
                                settings_hint(ui,"先聽系統輸出的聲音；系統迴路不可用時才改用內建麥克風。只用於辨識歌名、片段不會存檔，需要連網。關掉後瀏覽器改回用分頁標題猜歌。");
                                if self.settings.listen_id {
                                    match crate::audio_id::status() {
                                        Some(status) if status.needs_permission => {
                                            settings_hint(ui,"Windows 目前封鎖桌面應用程式使用麥克風，聽歌辨識收不到聲音。請開啟「設定 → 隱私權與安全性 → 麥克風 → 允許桌面應用程式存取您的麥克風」。");
                                            if settings_action_button(ui,"開啟 Windows 麥克風權限設定",SettingsActionGlyph::Refresh,true,true,"listen-permission").clicked() {
                                                crate::audio_id::open_permission_settings();
                                            }
                                            ui.add_space(2.0);
                                        }
                                        Some(status) => settings_hint(ui,&format!("聽歌辨識狀態：{}",status.text)),
                                        None => settings_hint(ui,"聽歌辨識狀態：待瀏覽器開始播放"),
                                    }
                                }
                                // 本曲自動對齊：聽歌辨識學會的位移，歌詞用它換算成曲內時間（進度條不動）。
                                let song_offset=crate::lyric_sync::offset_seconds(&self.media.title,&self.media.artist,&self.media.source);
                                if self.media.title.trim().is_empty() {
                                    settings_hint(ui,"本曲自動對齊：播放並認出歌之後，會自動算出這首歌的位移。");
                                } else if song_offset.abs()>=0.05 {
                                    settings_hint(ui,&format!("本曲自動對齊：已校準，歌詞比播放器進度{} {:.1} 秒（例：影片前奏不算進歌詞）。",if song_offset>0.0{"慢"}else{"早"},song_offset.abs()));
                                } else {
                                    settings_hint(ui,"本曲自動對齊：這首歌還沒校準，播放幾秒後會自動學習。");
                                }
                                if !self.media.title.trim().is_empty() && settings_action_button(ui,"歸零本曲對齊",SettingsActionGlyph::Refresh,true,true,"lyric-sync-reset").clicked() {
                                    crate::lyric_sync::forget(&self.media.title,&self.media.artist,&self.media.source);
                                }
                                ui.separator();
                                // 本機歌詞庫：抓過一次的歌詞就留在這台電腦，下次辨識完成立刻出詞。
                                let library=crate::lyrics_library::stats();
                                if library.songs==0 {
                                    settings_hint(ui,"本機歌詞庫：還沒有存過歌詞，第一次播放時會邊抓邊存。");
                                } else {
                                    settings_hint(ui,&format!("本機歌詞庫：已存 {} 首（{} KB）· 抓到就存本機，下次同一首歌直接讀取、不再連網重抓。",library.songs,library.bytes.saturating_div(1024)));
                                }
                                ui.separator();
                                let translate=self.settings.lyrics_translate;
                                setting_row(ui,"自動翻譯歌詞","非中文歌詞保留原文，譯文顯示在原文下方；中文歌本身就是繁體，不做翻譯",&mut self.settings.lyrics_translate);
                                settings_hint(ui,"日文、韓文原文也會翻譯；譯文快取在本機，同一首歌不重複送翻。翻譯需連網，歌詞文字會送到 Google 翻譯，失敗時照常顯示原文。");
                                if translate!=self.settings.lyrics_translate {self.refresh_translation();}
                            });
                            ui.add_space(10.0);
                            settings_label(ui,"歌詞顏色");
                            settings_card(ui,|ui|{
                                lyric_gradient_editor(ui,&mut self.settings);
                                lyric_karaoke_editor(ui,&mut self.settings);
                            });
                        });
                    }
                    2=>{
                        let on=self.settings_feature_header(ui,crate::app::FOCUS_PAGE,"番茄鍾與休息計時",|_,_|{});
                        ui.add_enabled_ui(on,|ui|{
                            settings_card(ui,|ui|{
                                setting_number(ui,"專注",&mut self.settings.focus_minutes,1,180," 分鐘",1.0);
                                ui.separator();
                                setting_number(ui,"休息",&mut self.settings.rest_minutes,1,60," 分鐘",1.0);
                                settings_hint(ui,"調整後從下一階段開始生效。");
                            });
                        });
                        ui.add_space(12.0);
                        let on=self.settings_feature_header(ui,crate::app::WATER_PAGE,"喝水與久坐提醒",|_,_|{});
                        ui.add_enabled_ui(on,|ui|{
                            settings_card(ui,|ui|{
                                // 喝水：開關一行，數量與間隔緊隨其後。
                                let old_remind=self.water.remind;
                                setting_row(ui,"喝水提醒","確認或喝水後重新計時；達標後停止提醒",&mut self.water.remind);
                                if self.water.remind!=old_remind{self.water.save();self.confirm_health_reminder(HealthReminder::Water);}
                                ui.add_enabled_ui(self.water.remind,|ui|{
                                    setting_number(ui,"提醒間隔",&mut self.settings.water_minutes,5,180," 分鐘",1.0);
                                    setting_number(ui,"每次記錄",&mut self.settings.serving_ml,50,1000," ml",25.0);
                                    if setting_number(ui,"每日目標",&mut self.water.goal_ml,500,6000," ml",50.0){self.water.save();}
                                });
                                ui.separator();
                                let old_sit=self.settings.sit_remind;
                                setting_row(ui,"久坐提醒","確認提醒或點「起身了」後重新計時",&mut self.settings.sit_remind);
                                if self.settings.sit_remind!=old_sit {self.confirm_health_reminder(HealthReminder::Sit);}
                                ui.add_enabled_ui(self.settings.sit_remind,|ui|{
                                    setting_number(ui,"久坐時長",&mut self.settings.sit_minutes,15,180," 分鐘",1.0);
                                });
                                ui.separator();
                                // 提醒時段與午休對喝水和久坐都生效。
                                // 喝水和久坐提醒都關掉時，時段和午休不起作用，一起置灰。
                                let any_reminder=self.water.remind||self.settings.sit_remind;
                                ui.add_enabled_ui(any_reminder,|ui|{
                                    settings_control_row(ui,"提醒時段",80.0,|ui|{
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                                            // 區域寬度正好等於兩個加減器加「至」，靠右放置後與上面單個加減器的右邊緣對齊。
                                            ui.allocate_ui_with_layout(egui::vec2(TIME_RANGE_HOURS_W,30.0),egui::Layout::left_to_right(egui::Align::Center),|ui|{
                                                number_stepper(ui,&mut self.settings.water_start,0,23," 時",1,48.0);
                                                ui.label("至");
                                                number_stepper(ui,&mut self.settings.water_end,0,24," 時",1,48.0);
                                            });
                                        }).response
                                    });
                                    setting_row(ui,"午休不提醒","午休時間內喝水和久坐都不彈提醒",&mut self.settings.lunch_rest);
                                    if self.settings.lunch_rest {
                                        settings_control_row(ui,"午休時間",80.0,|ui|{
                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                                                ui.allocate_ui_with_layout(egui::vec2(TIME_RANGE_CLOCK_W,30.0),egui::Layout::left_to_right(egui::Align::Center),|ui|{
                                                    time_stepper(ui,&mut self.settings.lunch_start,"lunch-start");
                                                    ui.label("至");
                                                    time_stepper(ui,&mut self.settings.lunch_end,"lunch-end");
                                                });
                                            }).response
                                        });
                                    }
                                    settings_hint(ui,"提醒時段和午休對喝水、久坐都生效；專注計時進行中不打擾，結束後再提醒。");
                                });
                            });
                        });
                    }
                    4=>{
                        let on=self.settings_feature_header(ui,crate::app::TODO_PAGE,"任務清單與到期提醒",|_,_|{});
                        let _=on;
                        #[cfg(feature="sdk-js")] self.plugins.settings(ui,"tasks",on);
                        ui.add_space(12.0);
                        let on=self.settings_feature_header(ui,crate::app::SHELF_PAGE,"臨時收納檔案與剪貼簿",|_,_|{});
                        ui.add_enabled_ui(on,|ui|{
                            settings_card(ui,|ui|{
                                ui.label(egui::RichText::new(format!("當前 {} 項 · 移除條目不會刪除原檔案",self.shelf.items.len())).size(12.0).color(settings_color(ctx,Color32::from_gray(180),Color32::from_gray(75))));
                                ui.separator();
                                setting_row(ui,"記錄剪貼簿","在置物架的「剪貼簿」裡檢視最近複製的文字，點一下再次複製",&mut self.settings.clipboard_history);
                                settings_hint(ui,&format!("本機儲存最近 {} 條；密碼管理器標記為敏感的內容不會記錄。",crate::clipboard::MAX_RECENT));
                                if ui.button("開啟剪貼簿").clicked(){self.settings_open=false;self.shelf_tab=1;self.show_page(3);}
                            });
                        });
                    }
                    6=>{
                        settings_label(ui,"提醒");
                        settings_card(ui,|ui|{
                            setting_row(ui,"提醒總開關","關閉後飲水、專注、待辦及聊天軟體提醒都不顯示",&mut self.settings.notifications);
                            // 提醒方式由系統通知和提示音兩個設定共同決定。
                            ui.separator();
                            ui.add_enabled_ui(self.settings.notifications,|ui|{
                                let mut mode:u8=if !self.settings.system_notify{0}else if self.settings.sound{2}else{1};
                                settings_control_row_right(ui,"提醒方式",|ui|{settings_segmented(ui,&mut mode,&[(0,"僅島上"),(1,"加系統通知"),(2,"系統通知 + 聲音")],96.0);});
                                settings_hint(ui,["只在島上提醒，不傳送 Windows 系統通知","同時在 Windows 通知中心留下記錄，不響鈴","同時傳送 Windows 系統通知並播放提示音"][mode as usize]);
                                self.settings.system_notify=mode>0;
                                self.settings.sound=mode==2;
                            });
                        });
                        settings_label(ui,"系統提示");
                        settings_card(ui,|ui|{
                            setting_row(ui,"系統狀態","音量、充電、網路和鍵盤狀態",&mut self.settings.system_hud);
                            ui.separator();
                            setting_row(ui,"藍牙裝置連線","連線與斷開時提醒",&mut self.settings.bluetooth_notifications);
                        });
                        // 聊天軟體：一個軟體一行、一個開關；附屬選項縮進跟在所屬軟體下面。
                        let master=self.settings.notifications;
                        settings_label(ui,"社群與聊天軟體");
                        if !master {
                            ui.label(egui::RichText::new("提醒總開關已關閉，開啟後下面的提醒才會生效").size(12.0).color(Color32::from_rgb(230,150,40)));
                        }
                        let sub=|ui:&mut egui::Ui,title:&str,description:&str,value:&mut bool|{
                            ui.horizontal(|ui|{ui.add_space(22.0);ui.vertical(|ui|setting_row(ui,title,description,value));});
                        };
                        let mut open_connect=false;
                        settings_card(ui,|ui|{
                            ui.add_enabled_ui(master,|ui|{
                                ui.horizontal(|ui|{
                                    ui.vertical(|ui|{
                                        ui.spacing_mut().item_spacing.y=2.0;
                                        ui.label(egui::RichText::new("LINE、Instagram、QQ 與其他應用程式").size(13.0).color(settings_color(ctx,Color32::WHITE,Color32::from_rgb(30,34,41))));
                                        ui.label(egui::RichText::new(&self.listener_status).size(11.5).color(settings_color(ctx,Color32::from_gray(175),Color32::from_gray(90))));
                                    });
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                                        if self.settings.read_notifications {
                                            if ui.button("斷開通知").clicked(){self.pending.push(Cmd::DisconnectNotifications);}
                                            if ui.button("重新連線").on_hover_text("收不到通知時按這裡，讓 Windows 重新授權").clicked(){self.pending.push(Cmd::RequestNotifications);}
                                        } else if ui.button("連線…").clicked(){open_connect=true;}
                                    });
                                });
                                sub(ui,"顯示通知正文","適用於 QQ、Windows 通知和本機接入；螢幕共享時建議關閉",&mut self.settings.notification_preview);
                                // 收到過通知的應用可能有幾十個：預設收起，展開後用可點選的標籤排成幾行，不再一應用一行。
                                let before_apps=self.notification_apps.items.clone();
                                let total=self.notification_apps.items.len();
                                if total>0 {
                                    let on_count=self.notification_apps.items.iter().filter(|(_,_,enabled)|*enabled).count();
                                    ui.horizontal(|ui|{ui.add_space(22.0);ui.vertical(|ui|{
                                        egui::CollapsingHeader::new(egui::RichText::new(format!("按應用程式接收（已開 {on_count} / 共 {total}）")).size(12.5)).id_salt("settings-notification-apps").default_open(false).show(ui,|ui|{
                                            ui.horizontal_wrapped(|ui|{
                                                ui.spacing_mut().item_spacing=egui::vec2(6.0,6.0);
                                                for (_,name,enabled) in &mut self.notification_apps.items {
                                                    if settings_toggle_chip(ui,name,*enabled).clicked(){*enabled=!*enabled;}
                                                }
                                            });
                                            settings_hint(ui,"點選切換；綠色為接收，灰框為不接收。應用程式會在收到通知後出現在這裡。");
                                        });
                                    });});
                                }
                                if before_apps!=self.notification_apps.items{self.notification_apps.save();}
                            });
                        });
                        let connect_id=egui::Id::new("settings-notify-connect");
                        if open_connect {ctx.data_mut(|d|d.insert_temp(connect_id,true));}
                        if ctx.data(|d|d.get_temp::<bool>(connect_id)).unwrap_or(false) {
                            let mut close=false;
                            let modal=egui::Modal::new(connect_id.with("modal")).frame(settings_modal_frame(ctx)).backdrop_color(Color32::from_black_alpha(110)).show(ctx,|ui|{
                                ui.set_width(360.0);
                                settings_apply_style(ui);if settings_modal_title(ui,"連線系統通知"){close=true;}
                                if ui.add_sized([ui.available_width(),30.0],egui::Button::new("連線系統通知")).clicked(){self.pending.push(Cmd::RequestNotifications);close=true;}
                                settings_hint(ui,"只接收 Windows 通知中心可見的內容；應用程式會在收到通知後出現在列表裡。");
                                ui.add_space(8.0);
                                if ui.button("取消").clicked(){close=true;}
                            });
                            if close || modal.should_close() {ctx.data_mut(|d|d.remove::<bool>(connect_id));}
                        }
                    }
                    7=>{
                        let mut add_mail=false;
                        let on=self.settings_feature_header(ui,crate::app::MAIL_PAGE,"新郵件提醒與驗證碼；島上歌詞模式顯示右側圖示",|_,_|{});
                        ui.add_enabled_ui(on,|ui|{
                        settings_label(ui,"設定");
                        settings_card(ui,|ui|{
                            setting_row(ui,"僅提醒驗證碼郵件","所有信箱共用。其他郵件靜默收進列表，可隨時查看。",&mut self.settings.mail_code_only_notifications);
                            ui.separator();
                            setting_row(ui,"驗證碼高亮","識別到驗證碼時在島上放大顯示，點擊即複製",&mut self.settings.mail_code);
                            ui.separator();
                            ui.add_enabled_ui(self.settings.mail_code,|ui|{
                                setting_row(ui,"避讓時彈出驗證碼","自動避讓時短暫顯示新收到的驗證碼，點擊即可複製",&mut self.settings.mail_code_popup);
                            });
                            ui.separator();
                            let sync_before=self.settings.mail_sync_operations;
                            ui.add_enabled_ui(self.mail.manage.pending.is_empty(),|ui|{
                                setting_row(ui,"同步郵件操作到信箱","已讀、刪除和恢復同步到信箱；關閉時只整理鬼島裡的記錄，切換不會補交之前的操作",&mut self.settings.mail_sync_operations);
                            });
                            if sync_before!=self.settings.mail_sync_operations{self.mail_manage_mode_changed();}
                            ui.separator();
                            ui.horizontal(|ui|{
                                ui.vertical(|ui|{
                                    ui.spacing_mut().item_spacing.y=2.0;
                                    ui.label(egui::RichText::new("會議提前").size(13.0).color(settings_color(ctx,Color32::WHITE,Color32::from_rgb(30,34,41))));
                                    ui.label(egui::RichText::new(format!("{} 分鐘 · 與行程共用",self.settings.mail_ics)).size(11.5).color(settings_color(ctx,Color32::from_gray(175),Color32::from_gray(90))));
                                });
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{if ui.button("在行程中設定 ›").clicked(){self.settings_tab=8;}});
                            });
                            settings_advanced(ui,"settings-mail-advanced",|ui|{
                                setting_choices(ui,"驗證碼隱藏",&mut self.settings.mail_code_hide_seconds,[30,60,300],"秒");
                                settings_hint(ui,"明確有效時長更短時提前隱藏；隱藏不代表失效，主動檢視後 15 秒再次隱藏。");
                                setting_choices(ui,"檢查間隔",&mut self.settings.mail_interval,[60,120,300],"秒");
                                settings_hint(ui,"支援推送的信箱實時收信；間隔只用於不支援推送的信箱。");
                            });
                        });
                        ui.horizontal(|ui|{
                            settings_label(ui,"信箱帳號");
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::BOTTOM),|ui|{if ui.button("＋ 新增信箱").clicked(){add_mail=true;}});
                        });
                        settings_card(ui,|ui|{
                            if self.mail.accounts.is_empty(){ui.label(egui::RichText::new("還沒有信箱。點「＋ 新增信箱」，儲存後自動連線。").size(12.0).color(settings_color(ctx,Color32::from_gray(195),Color32::from_gray(60))));}
                            let total=self.mail.accounts.len();
                            let(mut changed,mut remove,mut test)=(false,None,None);
                            for i in 0..total{
                                let(email,provider)=(self.mail.accounts[i].email.clone(),self.mail.accounts[i].provider);
                                let status=self.mail.status_of(&email).label();
                                let mut on=self.mail.accounts[i].enabled;
                                if i>0{ui.separator();}
                                ui.label(egui::RichText::new(&email).size(13.0).color(settings_color(ctx,Color32::WHITE,Color32::from_rgb(30,34,41))));
                                ui.label(egui::RichText::new(format!("{} · {}",provider.label(),status)).size(12.0).color(settings_color(ctx,Color32::from_gray(180),Color32::from_gray(75))));
                                if provider == crate::mail::Provider::Gmail
                                    && mail_connection_choice(ui, ("mail-account-connection", email.as_str()), &mut self.mail.accounts[i].use_windows_proxy)
                                {
                                    changed = true;
                                    self.mail.note = format!("{email}：連線方式已更新");
                                }
                                ui.horizontal(|ui|{
                                    ui.add(crate::mail_icon::image(provider).fit_to_exact_size(egui::vec2(22.0,22.0)));
                                    ui.vertical(|ui|{
                                        ui.spacing_mut().item_spacing.y=2.0;
                                        ui.label(egui::RichText::new(&email).size(13.0).color(settings_color(ctx,Color32::WHITE,Color32::from_rgb(30,34,41))));
                                        ui.label(egui::RichText::new(format!("{} · {}",provider.label(),status)).size(12.0).color(settings_color(ctx,Color32::from_gray(180),Color32::from_gray(75))));
                                    });
                                });
                                ui.horizontal(|ui|{
                                    if ui.add_sized([66.0,30.0],egui::Button::new("測試")).clicked(){test=Some(i);}
                                    if ui.add_sized([66.0,30.0],egui::Button::new("刪除")).clicked(){remove=Some(i);}
                                    ui.add_space(8.0);
                                    ui.label(egui::RichText::new("啟用").size(12.0).color(settings_color(ctx,Color32::from_gray(190),Color32::from_gray(65))));
                                    if switch(ui,&mut on){self.mail.accounts[i].enabled=on;changed=true;}
                                });
                            }
                            if changed{self.mail_accounts_changed();}
                            if let Some(i)=remove{
                                let email=self.mail.accounts[i].email.clone();
                                let credential=self.mail.accounts[i].credential_key();
                                self.mail.accounts.remove(i);
                                crate::mail::secrets::delete(&credential);
                                self.mail.status.remove(&email.to_ascii_lowercase());
                                self.mail.note=format!("{email}：已移除，憑據已從系統憑據管理器刪除");
                                self.mail_accounts_changed();
                            }
                            if let Some(i)=test{
                                let account=self.mail.accounts[i].clone();
                                if let Err(error)=account.endpoint(){
                                    self.mail.note=error;
                                }else{
                                match crate::mail::secrets::load(&account.credential_key()){
                                    Ok(pass)=>{self.mail.testing=Some(account.email.clone());self.mail.note=format!("{}：正在測試連線…",account.email);self.pending.push(Cmd::MailTest(account,pass));}
                                    Err(e)=>self.mail.note=format!("{}：{e}",account.email),
                                }
                                }
                            }
                                                    if !self.mail.note.is_empty(){ui.add_space(3.0);ui.add(egui::Label::new(egui::RichText::new(&self.mail.note).size(12.0).color(settings_color(ctx,Color32::from_gray(180),Color32::from_gray(75)))).wrap());}
                        });
                        });
                        // 新增信箱：一次性的長表單放進彈窗，頁面上只保留帳號列表。
                        let form_id=egui::Id::new("settings-mail-add");
                        if add_mail {self.mail.form.open=true;}
                        if self.mail.form.open {
                            let mut close=false;
                            let modal=egui::Modal::new(form_id).frame(settings_modal_frame(ctx)).backdrop_color(Color32::from_black_alpha(110)).show(ctx,|ui|{
                            ui.set_width(440.0);
                            settings_apply_style(ui);if settings_modal_title(ui,"新增信箱"){close=true;}
                            ui.spacing_mut().item_spacing.y=8.0;
                            settings_control_row(ui,"信箱服務",56.0,|ui|{
                                egui::ComboBox::from_id_salt("mail-provider").width(160.0).selected_text(self.mail.form.provider.label()).show_ui(ui,|ui|{
                                    ui.spacing_mut().item_spacing.y=2.0;
                                    for p in crate::mail::Provider::ALL{
                                        let chosen=ui.horizontal(|ui|{
                                            ui.add(crate::mail_icon::image(p).fit_to_exact_size(egui::vec2(16.0,16.0)));
                                            ui.selectable_label(self.mail.form.provider==p,p.label())
                                        }).inner;
                                        if chosen.clicked(){self.mail.form.provider=p;}
                                    }
                                }).response
                            });
                            ui.add_space(5.0);
                            let microsoft=self.mail.form.provider==crate::mail::Provider::Microsoft;
                            ui.label(egui::RichText::new(if microsoft{"連線微軟信箱"}else{"取得授權碼或專用密碼"}).size(12.5).strong());
                            ui.add(egui::Label::new(egui::RichText::new(self.mail.form.provider.hint()).size(12.0).color(settings_color(ctx,Color32::from_gray(200),Color32::from_gray(55)))).wrap());
                            let (label,url)=self.mail.form.provider.setup_link();
                            if !url.is_empty() && ui.link(label).clicked(){self.pending.push(Cmd::OpenUrl(url.into()));}
                            ui.add_space(8.0);
                            if self.mail.form.provider==crate::mail::Provider::Custom {
                                settings_control_row(ui,"IMAP 主機",80.0,|ui|{ui.text_edit_singleline(&mut self.mail.form.custom.host)});
                                settings_control_row(ui,"TLS 埠",80.0,|ui|{ui.add(egui::DragValue::new(&mut self.mail.form.custom.port).range(1..=65535))});
                                settings_control_row(ui,"登入使用者名稱",80.0,|ui|{ui.add(egui::TextEdit::singleline(&mut self.mail.form.custom.username).vertical_align(egui::Align::Center).hint_text("留空使用信箱地址"))});
                            }
                            if !microsoft{
                            let input_width=(ui.available_width()-56.0-48.0-2.0*ui.spacing().item_spacing.x).max(100.0);
                            settings_control_row(ui,"信箱",56.0,|ui|{
                                ui.add_sized([input_width,30.0],egui::TextEdit::singleline(&mut self.mail.form.email).vertical_align(egui::Align::Center).hint_text("輸入信箱地址"))
                            });
                            let gmail = self.mail.form.provider == crate::mail::Provider::Gmail;
                            if gmail {
                                mail_connection_choice(ui, "mail-form-connection", &mut self.mail.form.use_windows_proxy);
                            }
                            let target = crate::mail::Account {
                                custom: (self.mail.form.provider == crate::mail::Provider::Custom).then(|| self.mail.form.custom.clone()),
                                email: self.mail.form.email.trim().to_owned(),
                                provider: self.mail.form.provider,
                                enabled: true,
                                use_windows_proxy: gmail && self.mail.form.use_windows_proxy,
                            };
                            let unsupported=target.endpoint().err().as_deref()==Some(crate::mail::MICROSOFT_UNSUPPORTED);
                            if unsupported{ui.add(egui::Label::new(crate::mail::MICROSOFT_UNSUPPORTED).wrap());}
                            ui.add_enabled_ui(!unsupported,|ui|{
                            settings_control_row(ui,"授權碼",56.0,|ui|{
                                let field=ui.add_sized([input_width,30.0],egui::TextEdit::singleline(&mut self.mail.form.password).vertical_align(egui::Align::Center).password(!self.mail.form.show).hint_text("輸入信箱授權碼"));
                                if ui.add_sized([48.0,30.0],egui::Button::new(if self.mail.form.show{"隱藏"}else{"顯示"})).clicked(){self.mail.form.show=!self.mail.form.show;}
                                field
                            });
                            ui.horizontal(|ui|{
                                let test = ui.button("測試連線").clicked();
                                let save = ui.button("儲存並開始監控").clicked();
                                if test || save {
                                    let email = target.email.clone();
                                    if !email.contains('@'){self.mail.note="請先填信箱地址".into();}
                                    else if let Err(error)=target.endpoint(){self.mail.note=error;}
                                    else if self.mail.form.password.is_empty(){self.mail.note="請先填授權碼（不是信箱登入密碼）".into();}
                                    else if test {
                                        self.mail.testing=Some(email.clone());
                                        self.mail.note=format!("{email}：正在測試連線…");
                                        self.pending.push(Cmd::MailTest(target, self.mail.form.password.clone()));
                                    } else {
                                        match crate::mail::secrets::save(&target.credential_key(),&self.mail.form.password){
                                            Ok(())=>{
                                                if let Some(slot)=self.mail.accounts.iter_mut().find(|a|a.email.eq_ignore_ascii_case(&email)){*slot=target;}else{self.mail.accounts.push(target);}
                                                self.mail.form.password.clear();
                                                self.mail.note=format!("{email}：已儲存，正在連線…");
                                                self.mail_accounts_changed();
                                            }
                                            Err(e)=>self.mail.note=e,
                                        }
                                    }
                                }
                            });
                            ui.label(egui::RichText::new("授權碼儲存在 Windows 憑據管理器中。").size(12.0).color(settings_color(ctx,Color32::from_gray(180),Color32::from_gray(75))));
                            });
                            }
                                                    if !self.mail.note.is_empty(){ui.add_space(3.0);ui.add(egui::Label::new(egui::RichText::new(&self.mail.note).size(12.0).color(settings_color(ctx,Color32::from_gray(180),Color32::from_gray(75)))).wrap());}
                            });
                            // 儲存後彈窗保持打開，結果和「測試連線」都在這裡；使用者自己關閉。
                            if close || modal.should_close() {
                                self.mail.form.open = false;
                                self.mail.form.use_windows_proxy = false;
                            }
                        }
                    }
                    8=>{
                        ui.spacing_mut().item_spacing.y=4.0;
                        let mut open_add=false;
                        let on=self.settings_feature_header(ui,crate::app::CALENDAR_PAGE,"月曆、郵件邀請與工作行程",|_,_|{});
                        ui.add_enabled_ui(on,|ui|{
                        settings_label(ui,"設定");
                        settings_card(ui,|ui|{
                            setting_row(ui,"自動同步","每 5 分鐘更新行程；關閉後可點「立即同步」",&mut self.settings.calendar_auto_sync);
                            ui.separator();
                            setting_choices(ui,"會議提前",&mut self.settings.mail_ics,[5,15,30],"分鐘");
                            settings_hint(ui,"與郵件中的會議邀請共用提醒，同一場會議只提醒一次。");
                        });
                        #[cfg(feature="sdk-js")] self.plugins.settings(ui,"calendar",on);
                        settings_label(ui,"行事曆帳號");
                        let mut import=false;
                        settings_card(ui,|ui|{
                        if let Some(cfg)=self.calendar.config.clone(){
                                let n=self.calendar.events.len();
                                // 一行：平台圖標、名稱與狀態，右側立即同步和斷開。
                                ui.horizontal(|ui|{
                                    let (rect,_)=ui.allocate_exact_size(egui::vec2(28.0,28.0),egui::Sense::hover());
                                    ui.painter().rect_filled(rect,7.0,Color32::from_rgb(51,112,255));
                                    ui.painter().text(rect.center(),egui::Align2::CENTER_CENTER,cfg.provider.label().chars().next().unwrap_or('日'),egui::FontId::proportional(13.0),Color32::WHITE);
                                    ui.vertical(|ui|{
                                        ui.spacing_mut().item_spacing.y=2.0;
                                        ui.label(egui::RichText::new(format!("{} · 最後同步 {}",cfg.provider.label(),if self.calendar.last_sync.is_empty(){"—".to_string()}else{self.calendar.last_sync.clone()})).size(13.0).color(settings_color(ctx,Color32::WHITE,Color32::from_rgb(30,34,41))));
                                        let status = if self.calendar.connected {
                                            if n == 0 && !self.calendar.warning.is_empty() { "● 已連線 · 行程同步不完整".into() }
                                            else { format!("● 已連線 · 未來 24 小時 {n} 個行程") }
                                        } else if self.calendar.failures == 0 { "● 正在連線…".into() }
                                        else { format!("● 上次連線失敗 · 已知 {n} 個行程") };
                                        ui.label(egui::RichText::new(status).size(11.5).color(if self.calendar.connected{settings_color(ctx,Color32::from_rgb(120,220,160),Color32::from_rgb(24,121,64))}else{settings_color(ctx,Color32::from_rgb(240,180,110),Color32::from_rgb(145,89,12))}));
                                    });
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                                        if ui.add_enabled(!self.calendar.testing,egui::Button::new("斷開")).clicked(){self.pending.push(Cmd::CalendarForget);}
                                        if ui.button("立即同步").clicked(){self.calendar.note="正在同步…".into();self.calendar.note_error=false;self.pending.push(Cmd::CalendarSync);}
                                    });
                                });
                                if let Some(feed)=self.calendar.feeds.iter().find(|feed|feed.config==cfg) {
                                    let status=if feed.month.loading {"正在同步本月行程…".to_owned()}
                                        else if !feed.month.error.is_empty(){format!("本月同步失敗：{}",feed.month.error)}
                                        else if feed.month.loaded==Some(self.calendar.view.month){format!("{} {} {} 個行程",self.calendar.view.month.format("%Y-%m"),if feed.month.warning.is_empty(){"已同步"}else{"已讀取"},feed.month.events.len())}
                                        else {String::new()};
                                    if !status.is_empty(){ui.add(egui::Label::new(egui::RichText::new(status).size(12.0)).wrap());}
                                    if !feed.month.warning.is_empty(){ui.add(egui::Label::new(egui::RichText::new(format!("本月部分行程未同步：{}",feed.month.warning)).size(12.0).color(settings_color(ctx,Color32::from_rgb(238,181,91),Color32::from_rgb(139,88,12)))).wrap());}
                                }
                                if !self.calendar.warning.is_empty(){ui.add(egui::Label::new(egui::RichText::new(format!("同步提示：{}",self.calendar.warning)).size(12.0).color(settings_color(ctx,Color32::from_rgb(238,181,91),Color32::from_rgb(139,88,12)))).wrap());}
                                if !self.calendar.error.is_empty(){
                                    ui.add(egui::Label::new(egui::RichText::new(&self.calendar.error).size(12.0).color(settings_color(ctx,Color32::from_rgb(240,96,96),Color32::from_rgb(184,34,34)))).wrap());
                                }
                                ui.separator();
                        }else{
                            ui.label(egui::RichText::new("還沒有連線行事曆。新增行事曆帳號，或匯入 .ics 檔案。").size(12.0).color(settings_color(ctx,Color32::from_gray(195),Color32::from_gray(60))));
                        }
                        ui.horizontal(|ui|{
                            if ui.button(if self.calendar.config.is_some(){"更換行事曆"}else{"＋ 新增行事曆"}).clicked(){open_add=true;}
                            if ui.button("匯入 .ics 檔案").clicked(){import=true;}
                        });
                        if !self.calendar.import_note.is_empty(){settings_hint(ui,&self.calendar.import_note);}
                        });
                        if import {self.pending.push(Cmd::CalendarImport);}
                        });
                        // 新增行事曆：選平台、填帳號、取得方法、匯入 .ics 都在彈窗裡。
                        let add_id=egui::Id::new("settings-calendar-add");
                        if open_add {self.calendar.form.open=true;self.calendar.form.guide_open=true;}
                        if self.calendar.form.open {
                            let mut close=false;
                            let connected_before=self.calendar.config.clone();
                            let modal=egui::Modal::new(add_id.with("modal")).frame(settings_modal_frame(ctx)).backdrop_color(Color32::from_black_alpha(110)).show(ctx,|ui|{
                            ui.set_width(460.0);
                            ui.spacing_mut().item_spacing.y=6.0;
                            settings_apply_style(ui);if settings_modal_title(ui,"新增行事曆"){close=true;}
                            settings_label(ui,"選擇行事曆平台");
                            egui::Frame::new().fill(settings_color(ctx,Color32::from_rgb(29,29,31),Color32::from_rgb(236,239,244))).corner_radius(8.0).inner_margin(3).show(ui,|ui|{
                                ui.set_min_width(ui.available_width());
                                ui.spacing_mut().item_spacing.x=2.0;
                                // 四個平台：Google / iCloud / CalDAV，第二行 Exchange。
                                let width=((ui.available_width()-4.0)/3.0).max(80.0);
                                for row in crate::calendar::Provider::ALL.chunks(3){
                                    ui.horizontal(|ui|{
                                        for &p in row{
                                            let selected=self.calendar.form.provider==p;
                                            let button=egui::Button::new(egui::RichText::new(p.label()).size(13.0).color(if selected{settings_color(ctx,Color32::WHITE,Color32::from_rgb(30,34,41))}else{settings_color(ctx,Color32::from_gray(190),Color32::from_gray(65))}))
                                                .fill(if selected{settings_color(ctx,Color32::from_rgb(81,81,85),Color32::from_rgb(208,217,231))}else{Color32::TRANSPARENT})
                                                .stroke(egui::Stroke::NONE).corner_radius(6.0);
                                            let button=ui.add_enabled_ui(!self.calendar.testing,|ui|ui.add_sized([width,30.0],button)).inner;
                                            let button=match p{
                                                crate::calendar::Provider::Google=>button.on_hover_text("Google CalDAV 已停止密碼登入，請改用 iCal 訂閱"),
                                                crate::calendar::Provider::ICloud=>button.on_hover_text("蘋果行事曆：Apple ID + App 專用密碼"),
                                                crate::calendar::Provider::Custom=>button.on_hover_text("任意 CalDAV 行事曆：Nextcloud、群暉、Fastmail 等"),
                                                crate::calendar::Provider::Exchange=>button.on_hover_text("公司自建或第三方 Exchange 行事曆（ActiveSync），只讀同步"),
                                                crate::calendar::Provider::Ical=>button.on_hover_text("iCal 訂閱：貼上 .ics 網址即可，只讀同步、不需要密碼"),
                                            };
                                            if button.clicked() && !selected{
                                                self.select_calendar_provider(p);
                                            }
                                        }
                                    });
                                }
                            });
                            ui.add_space(7.0);
                            ui.horizontal_top(|ui|{
                                ui.allocate_ui_with_layout(egui::vec2(50.0,18.0),egui::Layout::left_to_right(egui::Align::Center),|ui|{ui.set_min_size(egui::vec2(50.0,18.0));ui.label(egui::RichText::new(if self.calendar.form.provider==crate::calendar::Provider::Ical{"iCal 網址"}else{"伺服器"}).size(12.0).color(settings_color(ctx,Color32::from_gray(180),Color32::from_gray(75))));});
                                if self.calendar.form.provider.custom_server(){
                                    ui.add_sized([ui.available_width(),22.0],egui::TextEdit::singleline(&mut self.calendar.form.server).vertical_align(egui::Align::Center)
                                        .hint_text(egui::RichText::new(self.calendar.form.provider.server_hint()).color(settings_color(ctx,Color32::from_gray(155),Color32::from_gray(100)))));
                                }else{
                                    ui.add(egui::Label::new(egui::RichText::new(self.calendar.form.provider.server()).size(12.0).color(settings_color(ctx,Color32::from_gray(205),Color32::from_gray(55)))).wrap());
                                }
                            });
                            if self.calendar.form.provider!=crate::calendar::Provider::Ical {
                            settings_label(ui,"連線帳號");
                            ui.scope(|ui|{
                                ui.visuals_mut().extreme_bg_color=settings_color(ctx,Color32::from_rgb(25,25,28),Color32::from_rgb(244,246,249));
                            let input_width=(ui.available_width()-50.0-48.0-2.0*ui.spacing().item_spacing.x).max(80.0);
                                ui.horizontal(|ui|{
                                    ui.allocate_ui_with_layout(egui::vec2(50.0,30.0),egui::Layout::left_to_right(egui::Align::Center),|ui|{ui.set_min_size(egui::vec2(50.0,30.0));ui.label(egui::RichText::new("使用者名稱").size(13.0).color(settings_color(ctx,Color32::from_gray(210),Color32::from_gray(55))));});
                                    let user=ui.add_sized([input_width,30.0],egui::TextEdit::singleline(&mut self.calendar.form.username).vertical_align(egui::Align::Center)
                                        .hint_text(egui::RichText::new("輸入使用者名稱或貼上帳號資訊").color(settings_color(ctx,Color32::from_gray(155),Color32::from_gray(100)))));
                                    if user.changed(){
                                        let pasted=self.calendar.form.username.clone();
                                        if let Some((u,p))=crate::calendar::parse_paste(&pasted){
                                            self.calendar.form.username=u;
                                            self.calendar.form.password=p;
                                            self.calendar.note="已從貼上內容中填入使用者名稱和密碼".into();
                                            self.calendar.note_error=false;
                                        }
                                    }
                                });
                                ui.add_space(5.0);
                                ui.horizontal(|ui|{
                                    ui.allocate_ui_with_layout(egui::vec2(50.0,30.0),egui::Layout::left_to_right(egui::Align::Center),|ui|{ui.set_min_size(egui::vec2(50.0,30.0));ui.label(egui::RichText::new("密碼").size(13.0).color(settings_color(ctx,Color32::from_gray(210),Color32::from_gray(55))));});
                                    ui.add_sized([input_width,30.0],egui::TextEdit::singleline(&mut self.calendar.form.password).vertical_align(egui::Align::Center)
                                        .password(!self.calendar.form.show).hint_text(egui::RichText::new("輸入平台生成的密碼").color(settings_color(ctx,Color32::from_gray(155),Color32::from_gray(100)))));
                                    if ui.add_sized([48.0,30.0],egui::Button::new(if self.calendar.form.show{"隱藏"}else{"顯示"})).clicked(){self.calendar.form.show=!self.calendar.form.show;}
                                });
                            });
                            }
                            ui.add_space(7.0);
                            let guide=if self.calendar.form.provider==crate::calendar::Provider::Ical {
                                if self.calendar.form.guide_open{"如何取得 iCal 網址？（點選收起）"}else{"如何取得 iCal 網址？"}
                            }else if self.calendar.form.guide_open{"如何取得使用者名稱和密碼？（點選收起）"}else{"如何取得使用者名稱和密碼？"};
                            if ui.link(egui::RichText::new(guide).color(settings_color(ctx,Color32::from_rgb(120,185,245),Color32::from_rgb(26,100,177)))).clicked(){self.calendar.form.guide_open=!self.calendar.form.guide_open;}
                            if self.calendar.form.guide_open{
                                ui.add_space(4.0);
                                for(i,step)in self.calendar.form.provider.guide().iter().enumerate(){
                                    guide_line(ui,i+1,step);
                                }
                            }
                            ui.add_space(9.0);
                            let busy=self.calendar.testing;
                            let label=if busy{"正在測試連線…"}else{"測試並儲存"};
                            let btn=egui::Button::new(egui::RichText::new(label).size(13.0).strong().color(Color32::WHITE))
                                .fill(if busy{settings_color(ctx,Color32::from_rgb(70,70,76),Color32::from_rgb(213,219,227))}else{Color32::from_rgb(45,105,165)})
                                .corner_radius(7.0);
                            if ui.add_enabled_ui(!busy,|ui|ui.add_sized([130.0,32.0],btn)).inner.clicked(){
                                // Google 的 CalDAV 只認完整信箱：只填名稱時自動補上 @gmail.com，並回填欄位讓使用者看到。
                                let user=crate::calendar::normalize_username(self.calendar.form.provider,&self.calendar.form.username);
                                self.calendar.form.username=user.clone();
                                let custom=self.calendar.form.provider.custom_server();
                                let server=crate::calendar::normalize_server(&self.calendar.form.server);
                                if self.calendar.form.provider==crate::calendar::Provider::Ical {
                                    if let Some(server)=server {
                                        let cfg=crate::calendar::Config{provider:crate::calendar::Provider::Ical,username:String::new(),href:None,server:Some(server)};
                                        self.calendar.testing=true;
                                        self.calendar.test_password.clear();
                                        self.calendar.note="正在讀取 iCal…".into();
                                        self.calendar.note_error=false;
                                        self.pending.push(Cmd::CalendarTest(cfg,String::new()));
                                    }else{self.calendar.note="請貼上以 https:// 開頭、結尾為 .ics 的 iCal 網址".into();self.calendar.note_error=true;}
                                }else if self.calendar.form.provider==crate::calendar::Provider::Google {
                                    self.calendar.note="Google CalDAV 已改為只接受 OAuth，應用程式密碼必定回 401。請改選 iCal，貼上 Google 行事曆的「秘密 iCal 網址」".into();
                                    self.calendar.note_error=true;
                                }else if custom && server.is_none(){self.calendar.note="請填寫以 https:// 開頭的 CalDAV 伺服器地址".into();self.calendar.note_error=true;}
                                else if user.is_empty(){self.calendar.note="請先填使用者名稱".into();self.calendar.note_error=true;}
                                else if self.calendar.form.password.is_empty(){self.calendar.note="請先填密碼".into();self.calendar.note_error=true;}
                                else{
                                    // Google／iCloud 的 16 碼應用程式密碼常被複製成四組、夾著空白，清掉才驗得過。
                                    let password=crate::calendar::normalize_password(self.calendar.form.provider,&self.calendar.form.password);
                                    self.calendar.form.password=password.clone();
                                    // 長度不對時先說清楚，不要送出去換一個沒有線索的 401。
                                    if let Some(problem)=crate::calendar::app_password_problem(self.calendar.form.provider,&password){
                                        self.calendar.note=problem;self.calendar.note_error=true;
                                    }else{
                                    let cfg=crate::calendar::Config{provider:self.calendar.form.provider,username:user,href:None,server:if custom{server}else{None}};
                                    self.calendar.testing=true;
                                    self.calendar.test_password=password.clone();
                                    self.calendar.note="正在連線行事曆…".into();
                                    self.calendar.note_error=false;
                                    self.pending.push(Cmd::CalendarTest(cfg,password));
                                    }
                                }
                            }
                            ui.add_space(5.0);
                            ui.label(egui::RichText::new("連線成功後儲存，密碼儲存在 Windows 憑據管理器中。").size(11.0).color(settings_color(ctx,Color32::from_gray(180),Color32::from_gray(75))));
                            if !self.calendar.note.is_empty(){
                                ui.add_space(5.0);
                                let color=if self.calendar.note_error{settings_color(ctx,Color32::from_rgb(255,125,125),Color32::from_rgb(184,34,34))}else{settings_color(ctx,Color32::from_gray(210),Color32::from_gray(55))};
                                ui.add(egui::Label::new(egui::RichText::new(&self.calendar.note).size(12.0).color(color)).wrap());
                            }
                            });
                            // 連線成功（行事曆配置變化）後自動關閉彈窗。
                            if self.calendar.config.is_some() && self.calendar.config!=connected_before && !self.calendar.testing {close=true;}
                            if close || modal.should_close() {self.calendar.form.open=false;}
                        }
                    }
                    SETTINGS_TAB_SUPPORT=>{
                        settings_label(ui,"贊助鬼島");
                        settings_card(ui,|ui|{
                            ui.label(egui::RichText::new("鬼島沒有訂閱制，也沒有廣告").size(13.0).strong());
                            ui.add_space(2.0);
                            ui.add(egui::Label::new(egui::RichText::new("下載、更新與全部功能都免費。如果它真的幫到你，可以用下面的收款碼請作者喝杯咖啡；完全自願，不影響任何功能。").size(11.0).color(settings_color(ctx,Color32::from_gray(175),Color32::from_gray(85)))).wrap());
                        });
                        settings_card(ui,|ui|support_author(ui));
                    }
                    _=>{
                        settings_label(ui,"鬼島 Kuidao 桌面靈動島");
                        settings_card(ui,|ui|{
                            ui.horizontal(|ui|{
                                ui.label(egui::RichText::new(format!("版本 {}",env!("CARGO_PKG_VERSION"))).size(13.0));
                                ui.add_space(8.0);
                                ui.vertical(|ui|self.updates.ui(ui));
                            });
                            ui.add_space(3.0);
                            ui.add(egui::Label::new(egui::RichText::new("官方更新說明：本程式由官方 Release 管道發布更新，維護流程每日凌晨 0 至 3 時自動檢查與整理。檢查更新只讀取版本與說明，不會上傳任何本機資料；版本與功能仍以官方公告為準。").size(10.5).color(settings_color(ctx,Color32::from_gray(145),Color32::from_rgb(105,112,124)))).wrap());
                            ui.label(egui::RichText::new("桌面頂部的音樂、專注、提醒與行程小助手。").size(12.0).color(settings_color(ctx,Color32::from_gray(180),Color32::from_gray(75))));
                            ui.horizontal_wrapped(|ui|{
                                if ui.button("開啟本機資料目錄").clicked(){self.pending.push(Cmd::Open(crate::model::config_dir()));}
                                if ui.button("收集診斷").clicked(){
                                    // 另起 `daozai.exe --collect-diagnostics`：在新窗口裡收集，診斷包儲存到桌面並自動選中。
                                    if crate::diagnostics_tool::launch().is_err(){self.say("無法啟動診斷收集，請從開始選單開啟「收集鬼島診斷」");}
                                }
                                ui.label(egui::RichText::new("裝置圖示：Solar Icons · 480 Design").size(11.0).color(settings_color(ctx,Color32::from_gray(150),Color32::from_gray(105))));
                                ui.hyperlink_to("CC BY 4.0", "https://creativecommons.org/licenses/by/4.0/");
                            });
                        });
                        // 更新內容資料庫只保留最新一筆，每次檢查成功都整份覆寫；開發版與正式版共用同一個檔案。
                        settings_label(ui,"更新內容");
                        settings_card(ui,|ui|{
                            let heading=settings_color(ctx,Color32::from_gray(238),Color32::from_rgb(30,34,41));
                            let body_color=settings_color(ctx,Color32::from_gray(214),Color32::from_rgb(55,62,72));
                            let sub_color=settings_color(ctx,Color32::from_gray(145),Color32::from_rgb(105,112,124));
                            let accent=settings_color(ctx,Color32::from_rgb(126,224,166),Color32::from_rgb(22,122,68));
                            match self.updates.latest_notes(){
                                Some(record)=>{
                                    ui.horizontal(|ui|{
                                        egui::Frame::new().fill(settings_color(ctx,Color32::from_rgb(32,53,41),Color32::from_rgb(230,245,236))).corner_radius(999.0).inner_margin(egui::Margin::symmetric(8,2)).show(ui,|ui|{
                                            ui.label(egui::RichText::new("最新一筆").size(10.0).color(accent));
                                        });
                                        ui.label(egui::RichText::new(&record.version).size(12.0).strong().color(accent));
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                                            if let Some(time)=chrono::DateTime::from_timestamp(record.checked_unix,0){
                                                ui.label(egui::RichText::new(format!("更新於 {}",time.with_timezone(&chrono::Local).format("%m-%d %H:%M"))).size(10.0).color(sub_color));
                                            }
                                        });
                                    });
                                    ui.add_space(5.0);
                                    egui::ScrollArea::vertical().id_salt("settings-update-notes").max_height(230.0).auto_shrink([false,true]).show(ui,|ui|{
                                        for raw in record.notes.lines(){
                                            let line=raw.trim();
                                            if line.is_empty(){ui.add_space(2.0);continue;}
                                            let is_heading=line.starts_with('#');
                                            let is_bullet=line.starts_with("- ")||line.starts_with("* ");
                                            let text=if is_heading{line.trim_start_matches('#').trim()}
                                                else if is_bullet{line[2..].trim()}
                                                else{line};
                                            let text=text.replace('`',"");
                                            let color=if is_heading{heading}else{body_color};
                                            if is_bullet{
                                                ui.horizontal(|ui|{
                                                    let (dot,_)=ui.allocate_exact_size(egui::vec2(9.0,15.0),egui::Sense::hover());
                                                    ui.painter().circle_filled(egui::pos2(dot.left()+2.0,dot.center().y),2.2,accent);
                                                    ui.add(egui::Label::new(egui::RichText::new(text).size(11.5).color(color)).wrap());
                                                });
                                            }else{
                                                let mut rich=egui::RichText::new(text).size(if is_heading{16.0}else{11.5}).color(color);
                                                if is_heading{rich=rich.strong();}
                                                ui.add(egui::Label::new(rich).wrap());
                                            }
                                        }
                                    });
                                    ui.add_space(2.0);
                                }
                                None=>{
                                    ui.label(egui::RichText::new("還沒有更新內容紀錄").size(11.5).color(body_color));
                                    ui.label(egui::RichText::new("按一次「檢查更新」就會把最新一版的說明存進來，只保留一筆。").size(10.5).color(sub_color));
                                }
                            }
                        });
                    }
                    }
                });
            });
            self.avoidance.dialog(ctx,&mut self.settings);
            if before!=format!("{:?}",self.settings){self.settings.save();}
            ctx.set_style(original_style);
        });
        if !self.settings_open {self.window_layout().close();}
        if self.settings_open&&self.module_on(crate::app::WEATHER_PAGE) {self.weather_tutorial_ui(ctx);}
        else {self.weather_source.close();}
    }
}

/// 收款碼只在「支援作者」卡片展開時佔用貼圖；收起卡片或關閉設定窗口即釋放。
fn release_support_codes(ctx: &egui::Context) {
    ctx.data_mut(|data| {
        for i in 0..2usize {
            data.remove::<egui::TextureHandle>(egui::Id::new(("support-author-qr", i)));
        }
    });
}

/// 從原圖裁出二維碼區域 `[x, y, 邊長]` 並縮放到 `pixels` 見方。
fn support_code_image(bytes: &[u8], crop: [f32; 3], pixels: u32) -> egui::ColorImage {
    let image = image::load_from_memory(bytes).expect("內建收款碼圖片有效");
    let side = (crop[2] as u32)
        .min(image.width().saturating_sub(crop[0] as u32))
        .min(image.height().saturating_sub(crop[1] as u32));
    let code = image
        .crop_imm(crop[0] as u32, crop[1] as u32, side, side)
        .resize_exact(pixels, pixels, image::imageops::FilterType::Lanczos3)
        .to_rgba8();
    egui::ColorImage::from_rgba_unmultiplied([pixels as usize, pixels as usize], code.as_raw())
}

#[test]
fn support_codes_are_cropped_small_and_released_when_hidden() {
    for (bytes, crop) in [
        (
            include_bytes!("../assets/support/support-jkopay-qr.png").as_slice(),
            [0.0, 0.0, 600.0],
        ),
        (
            include_bytes!("../assets/support/support-alipay-qr.png").as_slice(),
            [0.0, 0.0, 600.0],
        ),
    ] {
        let image = support_code_image(bytes, crop, 360);
        assert_eq!(image.size, [360, 360]);
        assert!(
            image.pixels.len() * 4 <= 600 * 1024,
            "每張收款碼貼圖應小於 0.6MB"
        );
        // 裁剪結果應是二維碼本身：同時含有大量深色與淺色像素。
        let dark =
            image.pixels.iter().filter(|p| p.r() < 80).count() as f32 / image.pixels.len() as f32;
        assert!(
            (0.2..0.8).contains(&dark),
            "裁剪區域不像QR Code：深色佔比 {dark}"
        );
    }
    let ctx = egui::Context::default();
    let _ = ctx.run(Default::default(), |ctx| {
        let texture = ctx.load_texture(
            "qr",
            egui::ColorImage::new([4, 4], Color32::WHITE),
            Default::default(),
        );
        ctx.data_mut(|data| {
            data.insert_temp(egui::Id::new(("support-author-qr", 0usize)), texture)
        });
    });
    assert_eq!(
        ctx.tex_manager()
            .read()
            .allocated()
            .filter(|(_, meta)| meta.name == "qr")
            .count(),
        1
    );
    release_support_codes(&ctx);
    let _ = ctx.run(Default::default(), |_| {});
    assert_eq!(
        ctx.tex_manager()
            .read()
            .allocated()
            .filter(|(_, meta)| meta.name == "qr")
            .count(),
        0,
        "收起後應釋放收款碼貼圖"
    );
}

/// 引導步驟中的網址自動變成可點連結，點了用系統瀏覽器開啟。
fn guide_line(ui:&mut egui::Ui,index:usize,text:&str){
    let link=regex::Regex::new(r"https?://[^\s，。；、）)]+").unwrap();
    ui.horizontal_wrapped(|ui|{
        ui.spacing_mut().item_spacing.x=0.0;
        let color=settings_color(ui.ctx(),Color32::from_gray(200),Color32::from_gray(55));
        ui.label(egui::RichText::new(format!("{index}. ")).size(12.0).color(color));
        let mut cursor=0;
        for found in link.find_iter(text){
            if found.start()>cursor {let before=&text[cursor..found.start()];if !before.is_empty(){ui.label(egui::RichText::new(before).size(12.0).color(color));}}
            ui.hyperlink_to(egui::RichText::new(found.as_str()).size(12.0),found.as_str());
            cursor=found.end();
        }
        if cursor<text.len(){ui.label(egui::RichText::new(&text[cursor..]).size(12.0).color(color));}
    });
}
//! 電量頁：手機、耳機、滑鼠、鍵盤等會回報電量的裝置，用一列環形進度顯示。
//!
//! 樣式參考使用者指定的圖：環內放裝置線稿，環下是百分比；綠弧長度跟著電量增減，
//! 低於 20% 整圈轉紅，充電時弧變成充電綠並在頂端加一顆閃電。

use crate::app::{App, Env, POWER_H, PW};
use crate::bt::{Device, DeviceKind};
use crate::paint::{Card, H};
use crate::theme::{self, W};
use eframe::egui::Color32;

/// 一列最多放幾顆環；超過就只畫前六個，其餘在頁尾補一行數量，避免整排擠成一團。
const MAX_RINGS: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RingKind {
    System,
    Phone,
    Audio,
    Gamepad,
    Mouse,
    Keyboard,
    Speaker,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PowerRing {
    pub kind: RingKind,
    pub name: String,
    pub level: u8,
    pub charging: bool,
}

impl PowerRing {
    pub(crate) fn tooltip(&self) -> String {
        let state = if self.charging {
            "充電中"
        } else if self.level < 20 {
            "電量偏低"
        } else {
            "使用中"
        };
        format!("{} · {}% · {}", self.name, self.level, state)
    }
}

fn ring_kind(kind: DeviceKind) -> RingKind {
    match kind {
        DeviceKind::Bone | DeviceKind::Earbuds | DeviceKind::Headphones => RingKind::Audio,
        DeviceKind::Speaker => RingKind::Speaker,
        DeviceKind::Keyboard => RingKind::Keyboard,
        DeviceKind::Mouse => RingKind::Mouse,
        DeviceKind::Gamepad => RingKind::Gamepad,
        DeviceKind::Phone => RingKind::Phone,
        DeviceKind::Other => RingKind::Other,
    }
}

fn icon_kind(kind: RingKind) -> DeviceKind {
    match kind {
        RingKind::Phone => DeviceKind::Phone,
        RingKind::Audio => DeviceKind::Headphones,
        RingKind::Gamepad => DeviceKind::Gamepad,
        RingKind::Mouse => DeviceKind::Mouse,
        RingKind::Keyboard => DeviceKind::Keyboard,
        RingKind::Speaker => DeviceKind::Speaker,
        RingKind::System | RingKind::Other => DeviceKind::Other,
    }
}

/// 排序：系統電池（桌機就是主機自己）→ 手機 → 音訊 → 手把 → 滑鼠 → 鍵盤 → 音箱 → 其他。
fn rank(kind: RingKind) -> u8 {
    match kind {
        RingKind::System => 0,
        RingKind::Phone => 1,
        RingKind::Audio => 2,
        RingKind::Gamepad => 3,
        RingKind::Mouse => 4,
        RingKind::Keyboard => 5,
        RingKind::Speaker => 6,
        RingKind::Other => 7,
    }
}

/// 弧的顏色：充電用充電綠；低於 20% 轉紅；其餘用系統綠。
pub(crate) fn ring_color(level: u8, charging: bool) -> Color32 {
    if charging {
        theme::CHARGE
    } else if level < 20 {
        theme::RED
    } else {
        theme::GREEN
    }
}

/// 組出要顯示的環。`battery` 是系統電池（桌機多為 None），裝置取自藍牙輪詢結果。
pub(crate) fn rings(battery: Option<(u8, bool)>, devices: &[Device]) -> Vec<PowerRing> {
    let mut out = Vec::new();
    if let Some((level, charging)) = battery.filter(|(level, _)| *level <= 100) {
        out.push(PowerRing { kind: RingKind::System, name: "系統電池".into(), level, charging });
    }
    // 同一顆裝置常同時出現在傳統藍牙與 BLE 兩份清單，用「類型 + 名稱」去重。
    let mut seen: Vec<(RingKind, String)> = Vec::new();
    for device in devices {
        let Some(level) = device.battery.filter(|level| *level <= 100) else { continue };
        let kind = ring_kind(device.kind);
        let name = device.name.trim().to_string();
        if name.is_empty() {
            continue;
        }
        let key = (kind, name.clone());
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        out.push(PowerRing { kind, name, level, charging: false });
    }
    out.sort_by_key(|ring| rank(ring.kind));
    out
}

/// 弧用一串重疊的實心圓點拼成。描邊弧線在缺口兩端會冒出尖角，三角帶又沒有反鋸齒；
/// 圓點本身有反鋸齒、彼此重疊成連續色帶，兩端自然收成圓頭，跟參考圖的環形進度一致。
fn arc_band(c: &Card, cx: f32, cy: f32, r: f32, a0: f32, a1: f32, width: f32, color: Color32) {
    let dot = width * 0.5;
    let span = (a1 - a0).abs() * r;
    let steps = ((span / (width * 0.36)).ceil() as usize).max(2);
    for i in 0..=steps {
        let t = a0 + (a1 - a0) * i as f32 / steps as f32;
        c.circle(cx + r * t.cos(), cy + r * t.sin(), dot, color);
    }
}

impl App {
    pub(crate) fn page_power(&self, c: &Card, env: &Env) {
        let all = rings(env.battery.map(|level| (level, env.charging)), &env.bt);
        let total = all.len();
        let list = &all[..total.min(MAX_RINGS)];
        let summary = if list.is_empty() {
            "沒有偵測到可回報電量的裝置".to_string()
        } else {
            let mut parts = vec![format!("{total} 個裝置")];
            if let Some(lowest) = list.iter().min_by_key(|ring| ring.level) {
                parts.push(format!("最低 {}% {}", lowest.level, lowest.name));
            }
            if let Some(charging) = list.iter().find(|ring| ring.charging) {
                parts.push(format!("{} 充電中", charging.name));
            }
            parts.join(" · ")
        };
        c.text(
            15.0,
            41.0,
            &c.fit(&summary, 9.5, W::Regular, 300.0),
            9.5,
            W::Regular,
            if list.is_empty() { theme::GRAY } else { theme::DIM },
            H::Left,
        );

        if list.is_empty() {
            self.paint_power_empty(c);
        } else {
            self.paint_power_rings(c, list);
        }

        // 底部說明：藍牙裝置要系統有回報電量才會出現，避免使用者以為壞了。
        let footer = POWER_H - 8.0;
        let note = if total > MAX_RINGS { format!("另有 {} 個裝置未列出", total - MAX_RINGS) } else { "每分鐘更新".to_string() };
        c.text(
            15.0,
            footer,
            &c.fit(&note, 8.5, W::Regular, 150.0),
            8.5,
            W::Regular,
            theme::DIM,
            H::Left,
        );
        c.hit(12.0, footer - 12.0, 160.0, 16.0, "power-footer")
            .on_hover_text("系統電池優先；桌機顯示已連線手機與藍牙耳機、滑鼠、鍵盤的電量\n藍牙裝置要等系統回報電量才會出現");
    }

    fn paint_power_empty(&self, c: &Card) {
        // 空狀態：畫一個虛線圈示意「這裡會出現裝置電量」。
        let cx = PW * 0.5;
        let cy = 84.0;
        let r = 24.0;
        c.ring(cx, cy, r, 2.6, Color32::from_white_alpha(26));
        self.paint_power_icon(c, RingKind::Phone, cx, cy - 1.0, 18.0, theme::DIM);
        c.text(cx, cy + r + 20.0, "尚無裝置電量", 11.0, W::Semibold, theme::WHITE, H::Center);
        c.text(
            cx,
            cy + r + 36.0,
            "連接手機、藍牙耳機、滑鼠或鍵盤後自動顯示",
            9.0,
            W::Regular,
            theme::DIM,
            H::Center,
        );
    }

    fn paint_power_rings(&self, c: &Card, list: &[PowerRing]) {
        let count = list.len() as f32;
        let spacing = if count <= 4.0 { 74.0 } else { 66.0 };
        let cx0 = PW * 0.5;
        let cy = 80.0;
        let r = if count <= 3.0 { 24.0 } else { 21.0 };
        let width = 4.6;
        // 底弧（未充電部分）保持很淡，避免整排看起來像一整條灰線。
        let track = Color32::from_white_alpha(30);
        for (i, ring) in list.iter().enumerate() {
            let cx = cx0 + (i as f32 - (count - 1.0) * 0.5) * spacing;
            let color = ring_color(ring.level, ring.charging);
            c.ring(cx, cy, r, width, track);
            let fraction = ring.level as f32 / 100.0;
            if fraction >= 1.0 {
                // 滿電畫整圈：弧線首尾會留一道接縫，直接畫圓環比較乾淨。
                c.ring(cx, cy, r, width, color);
            } else if fraction > 0.0 {
                // 從正上方順時針畫：0 = 12 點鐘方向。
                let a0 = -std::f32::consts::FRAC_PI_2;
                arc_band(c, cx, cy, r, a0, a0 + std::f32::consts::TAU * fraction, width, color);
            }
            self.paint_power_icon(c, ring.kind, cx, cy, r * 0.94, theme::WHITE);
            if ring.charging {
                self.paint_power_bolt(c, cx, cy - r - 7.0, color);
            }
            c.text(
                cx,
                cy + r + 15.0,
                &format!("{}%", ring.level),
                12.5,
                W::Semibold,
                if ring.level < 20 && !ring.charging { ring_color(ring.level, false) } else { theme::WHITE },
                H::Center,
            );
            c.text(cx, cy + r + 30.0, &c.fit(&ring.name, 8.0, W::Regular, spacing - 8.0), 8.0, W::Regular, theme::DIM, H::Center);
            c.hit(cx - spacing * 0.46, cy - r - 8.0, spacing * 0.92, r * 2.0 + 52.0, ("power-ring", i))
                .on_hover_text(ring.tooltip());
        }
    }

    /// `cx/cy` 是圖示中心；`device_icon` 吃的是左上角，這裡換算後再畫。
    fn paint_power_icon(&self, c: &Card, kind: RingKind, cx: f32, cy: f32, size: f32, color: Color32) {
        match kind {
            RingKind::System => {
                // 系統電池：細線電池身 + 右側正極凸點，和頂欄電量圈語意一致。
                let w = size * 0.86;
                let h = size * 0.42;
                c.stroke(cx - w * 0.5 - 1.0, cy - h * 0.5, w, h, h * 0.32, 1.3, color);
                c.fill(cx + w * 0.5 + 1.0, cy - h * 0.22, 1.8, h * 0.44, 0.9, color);
                c.fill(cx - w * 0.5, cy - h * 0.5 + 1.6, (w - 3.2) * 0.45, h - 3.2, h * 0.2, color);
            }
            _ => crate::extras::device_icon(c, icon_kind(kind), cx - size * 0.5, cy - size * 0.5, size),
        }
    }

    /// 充電閃電：畫在環的正上方，跟圖四一樣貼在外面。
    fn paint_power_bolt(&self, c: &Card, x: f32, y: f32, color: Color32) {
        let s = 6.2;
        c.poly(
            &[
                (x - s * 0.18, y - s),
                (x + s * 0.62, y - s * 0.24),
                (x + s * 0.12, y - s * 0.24),
                (x + s * 0.34, y + s),
                (x - s * 0.62, y + s * 0.22),
                (x - s * 0.06, y + s * 0.22),
            ],
            color,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(name: &str, kind: DeviceKind, battery: Option<u8>) -> Device {
        Device { id: name.into(), name: name.into(), battery, kind }
    }

    #[test]
    fn rings_put_system_battery_first_and_drop_devices_without_levels() {
        let devices = vec![
            device("MX Keys", DeviceKind::Keyboard, Some(58)),
            device("iPhone", DeviceKind::Phone, Some(95)),
            device("沒有電量的音箱", DeviceKind::Speaker, None),
        ];
        let list = rings(Some((70, false)), &devices);
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].kind, RingKind::System);
        assert_eq!(list[0].level, 70);
        assert_eq!(list[1].kind, RingKind::Phone);
        assert_eq!(list[1].level, 95);
        assert_eq!(list[2].kind, RingKind::Keyboard);
        assert!(list.iter().all(|ring| ring.name != "沒有電量的音箱"));
    }

    #[test]
    fn rings_dedupe_the_same_device_reported_twice() {
        let devices = vec![
            device("AirPods Pro", DeviceKind::Earbuds, Some(64)),
            device("AirPods Pro", DeviceKind::Headphones, Some(64)),
        ];
        let list = rings(None, &devices);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].kind, RingKind::Audio);
        assert_eq!(list[0].level, 64);
    }

    #[test]
    fn rings_keep_every_device_so_the_page_can_count_them() {
        let devices = (0..9)
            .map(|i| device(&format!("裝置 {i}"), DeviceKind::Mouse, Some(40)))
            .collect::<Vec<_>>();
        assert_eq!(rings(None, &devices).len(), 9);
        assert!(MAX_RINGS < 9);
    }

    #[test]
    fn low_battery_turns_the_arc_red_and_charging_wins() {
        assert_eq!(ring_color(19, false), theme::RED);
        assert_eq!(ring_color(20, false), theme::GREEN);
        assert_eq!(ring_color(19, true), theme::CHARGE);
        assert_eq!(ring_color(100, false), theme::GREEN);
    }

    #[test]
    fn tooltip_states_read_like_the_ring() {
        let low = PowerRing { kind: RingKind::Mouse, name: "MX Master".into(), level: 12, charging: false };
        assert!(low.tooltip().contains("電量偏低"));
        let charging = PowerRing { kind: RingKind::Phone, name: "iPhone".into(), level: 55, charging: true };
        assert!(charging.tooltip().contains("充電中"));
    }
}

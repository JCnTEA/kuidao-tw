//! 藍牙設備連接與多來源電量監控。
mod state;
pub use state::{Device,DeviceKind,BatterySource,BatteryReading,FRESH_FOR};

mod bas;
pub mod compat;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
mod receivers;

/// 識別順序：具體型號詞 → 通用名稱詞 → 藍牙類別碼 → 兜底名稱詞。
/// 短詞（pen、mic、band…）按整詞匹配，避免 OpenRun 裡的 pen 被認成觸控筆。
fn device_kind(name: &str, classic: bool, class: Option<u32>) -> DeviceKind {
    use DeviceKind::*;
    let name = name.to_lowercase();
    let words: Vec<&str> = name.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let word = |w: &str| words.contains(&w);
    let any = |list: &[&str]| list.iter().any(|s| name.contains(s));
    if name.contains("airpods") { return if word("max") { Headphones } else if word("pro") { EarbudsStemTips } else { EarbudsStem }; }
    if name.contains("freebuds") && word("pro") { return EarbudsStemTips; }
    if any(&["joy-con", "joycon"]) { return JoyCon; }
    // Shokz 的開放式耳機不是骨傳導，要先於骨傳導規則。
    if any(&["openfit", "opendots", "freeclip", "ultra open", "耳夾"]) { return OpenEar; }
    if any(&["shokz", "aftershokz", "openmove", "openrun", "openswim", "骨傳導"]) { return Bone; }
    if word("wf") || any(&["earbuds", "buds", "earphones", "入耳"]) { return Earbuds; }
    if word("wh") { return Headphones; }
    if any(&["hearing", "助聽"]) { return HearingAid; }
    if any(&["trackpad", "touchpad", "觸控板"]) { return Trackpad; }
    if any(&["keyboard", "鍵盤"]) { return Keyboard; }
    if any(&["mouse", "鼠標"]) { return Mouse; }
    if any(&["stylus", "手寫筆", "觸控筆"]) || word("pen") || word("pencil") { return Pen; }
    if any(&["presenter", "spotlight", "翻頁"]) { return Presenter; }
    if any(&["gamepad", "controller", "dualsense", "dualshock", "手柄"]) { return Gamepad; }
    if any(&["speaker", "soundlink", "音箱", "音響"]) { return Speaker; }
    if any(&["microphone", "麥克風"]) || word("mic") { return Mic; }
    if any(&["遙控"]) || word("remote") { return Remote; }
    if any(&["ipad", "tablet", "matepad", "平板"]) { return Tablet; }
    if any(&["手錶"]) || words.iter().any(|w| w.starts_with("watch")) { return Watch; }
    if any(&["手環"]) || word("band") { return Band; }
    if any(&["心率"]) || word("hrm") || word("heart") { return Health; }
    if let Some(class) = class {
        if classic {
            let minor = (class >> 2) & 0x3f;
            match (class >> 8) & 0x1f {
                1 if minor == 7 => return Tablet,
                2 => return Phone,
                4 => return match minor { 4 => Mic, 5 | 7 | 10 => Speaker, _ => Headphones },
                5 => return match ((class >> 6) & 3, (class >> 2) & 0xf) {
                    (1 | 3, _) => Keyboard, (2, _) => Mouse,
                    (_, 1 | 2) => Gamepad, (_, 3) => Remote, (_, 5 | 7) => Pen, _ => Other,
                },
                7 if minor == 1 => return Watch,
                9 => return Health,
                _ => {}
            }
        } else {
            // 低功耗藍牙外觀值：高 10 位是類別，低 6 位是子類別。
            match (class >> 6, class & 0x3f) {
                (15, 1) => return Keyboard,
                (15, 2) => return Mouse,
                (15, 3 | 4) => return Gamepad,
                (15, 5 | 7) => return Pen,
                (15, 9) => return Trackpad,
                (15, 10) => return Presenter,
                (1, _) => return Phone,
                (2, 7) => return Tablet,
                (3, _) => return Watch,
                (6, _) => return Remote,
                (12 | 13 | 14 | 16 | 17 | 18 | 49 | 50, _) => return Health,
                (33, _) => return Speaker,
                (37, 1) => return Earbuds,
                (37, _) => return Headphones,
                (41, _) => return HearingAid,
                _ => {}
            }
        }
    }
    // 手機走 Windows 藍牙電量屬性（BTHLE 的 Battery Service）回報，比只看音訊裝置更完整。
    if ["iphone", "ipad", "android", "galaxy", "pixel", "手機"].iter().any(|s| name.contains(s)) { return Phone; }
    if ["headphone", "headset", "耳機"].iter().any(|s| name.contains(s)) { Headphones } else { Other }
}


/// 釋放服務會喚醒後台線程並取消未完成的藍牙讀取。
pub struct Service {
    pub rx: std::sync::mpsc::Receiver<Vec<Device>>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl Drop for Service {
    fn drop(&mut self) {self.stop.store(true,std::sync::atomic::Ordering::Relaxed);}
}
pub fn spawn()->Service {
    let (tx,rx)=std::sync::mpsc::sync_channel(1);
    let stop=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    #[cfg(windows)] {
        let stopped=stop.clone();
        let _=std::thread::Builder::new().name("daozai-bluetooth".into()).spawn(move||windows::run(tx,stopped));
    }
    #[cfg(not(windows))] let _=tx;
    Service{rx,stop}
}

#[cfg(windows)]
pub fn spawn_receivers(store:compat::Store,wake:impl Fn()+Send+'static)->Service {
    let (tx,rx)=std::sync::mpsc::sync_channel(1);
    let stop=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stopped=stop.clone();
    let _=std::thread::Builder::new().name("daozai-usb-receivers".into()).spawn(move||receivers::run(tx,stopped,store,wake));
    Service{rx,stop}
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identifies_connected_audio_and_input_devices() {
        assert_eq!(device_kind("OpenMove by Shokz", true, Some(0x240418)), DeviceKind::Bone);
        assert_eq!(device_kind("QingLong 87 Ultra 8K", false, Some(961)), DeviceKind::Keyboard);
        assert_eq!(device_kind("MX Anywhere", false, Some(962)), DeviceKind::Mouse);
        assert_eq!(device_kind("耳機", true, Some(0x0404)), DeviceKind::Headphones);
        assert_eq!(device_kind("AirPods", true, None), DeviceKind::EarbudsStem);
        assert_eq!(device_kind("未知設備", false, None), DeviceKind::Other);
    }

    #[test]
    fn recognizes_specific_models_before_generic_words() {
        use DeviceKind::*;
        for (name, kind) in [
            ("小明的 AirPods Pro", EarbudsStemTips), ("AirPods Max", Headphones), ("AirPods", EarbudsStem),
            ("HUAWEI FreeBuds Pro 3", EarbudsStemTips), ("Galaxy Buds2 Pro", Earbuds), ("WF-1000XM5", Earbuds),
            ("WH-1000XM5", Headphones), ("Shokz OpenFit", OpenEar), ("OpenRun Pro by Shokz", Bone),
            ("Joy-Con (L)", JoyCon), ("Xbox Wireless Controller", Gamepad), ("Magic Trackpad", Trackpad),
            ("Surface Slim Pen 2", Pen), ("HUAWEI M-Pencil", Pen), ("Logi Spotlight", Presenter),
            ("DJI Mic 2", Mic), ("Galaxy Watch6", Watch), ("HUAWEI WATCH GT 4", Watch), ("Mi Smart Band 8", Band),
            ("小米手環 8", Band), ("MatePad Pro", Tablet), ("JBL Flip 6 speaker", Speaker),
        ] { assert_eq!(device_kind(name, false, None), kind, "{name}"); }
    }

    #[test]
    fn recognizes_class_and_appearance_codes() {
        use DeviceKind::*;
        // 經典藍牙：音頻 · 麥克風、電腦 · 平板、外設 · 手柄 / 遙控 / 數字筆、可穿戴 · 手錶。
        assert_eq!(device_kind("X", true, Some(0x0410)), Mic);
        assert_eq!(device_kind("X", true, Some(0x011C)), Tablet);
        assert_eq!(device_kind("X", true, Some(0x0508)), Gamepad);
        assert_eq!(device_kind("X", true, Some(0x050C)), Remote);
        assert_eq!(device_kind("X", true, Some(0x051C)), Pen);
        assert_eq!(device_kind("X", true, Some(0x0704)), Watch);
        // 低功耗外觀值：觸控板、演示遙控、數字筆、助聽器、入耳耳機、心率。
        assert_eq!(device_kind("X", false, Some(15 << 6 | 9)), Trackpad);
        assert_eq!(device_kind("X", false, Some(15 << 6 | 10)), Presenter);
        assert_eq!(device_kind("X", false, Some(15 << 6 | 7)), Pen);
        assert_eq!(device_kind("X", false, Some(41 << 6)), HearingAid);
        assert_eq!(device_kind("X", false, Some(37 << 6 | 1)), Earbuds);
        assert_eq!(device_kind("X", false, Some(13 << 6)), Health);
    }

    #[test]
    fn summary_omits_missing_battery() {
        let d = Device { id: "1".into(), name: "Headset".into(), battery: None, kind: DeviceKind::Headphones, ..Default::default() };
        assert_eq!(d.summary(), "Headset");
        let d = Device { battery: Some(80), ..d };
        assert_eq!(d.summary(), "Headset · 電量 80%");
    }

    #[test]
    fn picks_the_lowest_connected_audio_battery_and_ignores_other_devices() {
        let mut headset=device("AA");headset.kind=DeviceKind::Headphones;headset.battery=Some(80);
        let mut earbuds=device("BB");earbuds.kind=DeviceKind::Earbuds;earbuds.battery=Some(20);
        let mut mouse=device("CC");mouse.kind=DeviceKind::Mouse;mouse.battery=Some(5);
        let mut unknown_battery=device("DD");unknown_battery.kind=DeviceKind::Headphones;
        let devices=vec![headset,earbuds,mouse,unknown_battery];
        assert_eq!(audio_with_battery(&devices).map(|device|device.id.as_str()),Some("BB"));
        assert!(audio_with_battery(&[device("EE")]).is_none(),"沒回報電量的裝置不能硬顯示");
    }

    #[test]
    #[ignore]
    fn live_phone_battery_is_visible_to_the_app() {
        let service=spawn();
        let report=service.rx.recv_timeout(Duration::from_secs(10)).expect("藍牙掃描沒有回傳");
        for device in &report.devices { println!("{:?} {}",device.kind,device.summary()); }
        let phone=battery_with_fallback(&report.devices).filter(|device|device.kind==DeviceKind::Phone);
        assert!(phone.is_some_and(|device|device.battery.is_some()),"已連線手機應該回報電量");
    }
}

//! 经典蓝牙与低功耗蓝牙设备的连接状态和电量。

use std::time::{Duration,Instant};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub battery: Option<u8>,
    pub kind: DeviceKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceKind { Bone, Earbuds, Headphones, Speaker, Keyboard, Mouse, Gamepad, Phone, Other }

impl DeviceKind {
    pub fn is_audio(self) -> bool {
        matches!(self, DeviceKind::Bone|DeviceKind::Earbuds|DeviceKind::Headphones|DeviceKind::Speaker)
    }
}

fn device_kind(name: &str, classic: bool, class: Option<u32>) -> DeviceKind {
    use DeviceKind::*;
    let name = name.to_lowercase();
    if ["shokz", "aftershokz", "openmove", "openrun", "骨傳導"].iter().any(|s| name.contains(s)) { return Bone; }
    if ["earbuds", "airpods", "buds", "earphones", "入耳"].iter().any(|s| name.contains(s)) { return Earbuds; }
    if ["keyboard", "鍵盤"].iter().any(|s| name.contains(s)) { return Keyboard; }
    if ["mouse", "滑鼠"].iter().any(|s| name.contains(s)) { return Mouse; }
    if ["gamepad", "controller", "手柄"].iter().any(|s| name.contains(s)) { return Gamepad; }
    if ["speaker", "音箱", "音響"].iter().any(|s| name.contains(s)) { return Speaker; }
    if let Some(class) = class {
        if classic {
            match (class >> 8) & 0x1f {
                2 => return Phone,
                4 => return if matches!((class >> 2) & 0x3f, 5 | 7 | 10) { Speaker } else { Headphones },
                5 => return match (class >> 6) & 3 { 1 => Keyboard, 2 => Mouse, 3 => Keyboard, _ => Other },
                _ => {}
            }
        } else {
            match (class >> 6, class & 0x3f) {
                (15, 1) => return Keyboard,
                (15, 2) => return Mouse,
                (15, 3 | 4) => return Gamepad,
                (1, _) => return Phone,
                _ => {}
            }
        }
    }
    // 手機走 Windows 藍牙電量屬性（BTHLE 的 Battery Service）回報，比只看音訊裝置更完整。
    if ["iphone", "ipad", "android", "galaxy", "pixel", "手機"].iter().any(|s| name.contains(s)) { return Phone; }
    if ["headphone", "headset", "耳機"].iter().any(|s| name.contains(s)) { Headphones } else { Other }
}

impl Device {
    /// 通知条正文：`名称 · 电量 80%`，设备没上报电量时只写名称。
    pub fn summary(&self) -> String {
        match self.battery {
            Some(b) => format!("{} · 電量 {b}%", self.name),
            None => self.name.clone(),
        }
    }
    pub fn is_audio(&self) -> bool {
        matches!(self.kind, DeviceKind::Bone|DeviceKind::Earbuds|DeviceKind::Headphones|DeviceKind::Speaker)
    }
}

/// 桌機沒有系統電池時，挑一顆已連線且真的有回報電量的藍牙音訊裝置。
/// 同時連了多顆時取最低電量，讓快沒電的那顆優先被看見。
pub fn audio_with_battery(devices: &[Device]) -> Option<&Device> {
    devices.iter()
        .filter(|device| device.is_audio() && device.battery.is_some_and(|level| level <= 100))
        .min_by_key(|device| device.battery.unwrap_or(101))
}

/// 桌機沒有系統電池時，優先用已連線手機的電量；手機沒回報才退回耳機／音箱。
/// 手機是使用者最常看的一顆，而且 Windows 會同時列出經典藍牙與 BLE 兩筆同名裝置，
/// 因此只在這一層挑「真的有電量」的那筆，避免顯示到沒有電池資訊的重複項目。
pub fn battery_with_fallback(devices: &[Device]) -> Option<&Device> {
    devices.iter()
        .find(|device| device.kind == DeviceKind::Phone && device.battery.is_some_and(|level| level <= 100))
        .or_else(|| audio_with_battery(devices))
}

/// 仍在場的藍牙 PnP 節點（BTHLE／BTHENUM）。BLE 鏈路暫時閒置時，連線 AEP 會消失，
/// 但 PnP 節點還在，並保留最後一次回報的電量，用來讓頂欄電量不要整個不見。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresentNode {
    pub name: String,
    pub address: String,
    pub battery: u8,
    pub classic: bool,
}

/// 連線中的 AEP 沒有回報電量時，從仍在場的藍牙節點挑最後已知電量。
/// 優先順序與 `battery_with_fallback` 相同：手機先、再取音訊裝置裡電量最低的那顆。
pub fn present_power(nodes: &[PresentNode]) -> Option<(u8, DeviceKind)> {
    let kind_of = |node: &PresentNode| device_kind(&node.name, node.classic, None);
    if let Some(node) = nodes.iter()
        .filter(|node| node.battery <= 100 && kind_of(node) == DeviceKind::Phone)
        .min_by_key(|node| node.address.clone())
    {
        return Some((node.battery, DeviceKind::Phone));
    }
    nodes.iter()
        .filter(|node| node.battery <= 100 && kind_of(node).is_audio())
        .min_by_key(|node| node.battery)
        .map(|node| (node.battery, kind_of(node)))
}

/// 藍牙背景執行緒對外回報：連線裝置清單（給通知與即時電量），加上在場節點的最後已知電量。
#[derive(Clone, Debug, Default)]
pub struct Report {
    pub devices: Vec<Device>,
    pub present_power: Option<(u8, DeviceKind)>,
}

#[derive(Default)]
struct Cache {devices:Vec<Device>,battery_at:Option<Instant>}
impl Cache {
    fn update(&mut self,mut devices:Vec<Device>,now:Instant,read:impl FnOnce(&[Device])->Result<Vec<(String,u8)>,()>)->Vec<Device>{
        let changed=devices.len()!=self.devices.len() || devices.iter().any(|d|!self.devices.iter().any(|old|old.id==d.id));
        for device in &mut devices {device.battery=self.devices.iter().find(|old|old.id==device.id).and_then(|old|old.battery);}
        if devices.is_empty(){self.battery_at=None;}
        else if changed || self.battery_at.is_none_or(|at|now.duration_since(at)>=Duration::from_secs(60)) {
            // 失败同样限频，避免设备驱动异常时每两秒重新枚举整机。
            self.battery_at=Some(now);
            if let Ok(levels)=read(&devices){for device in &mut devices {device.battery=levels.iter().find(|(id,_)|id==&device.id).map(|(_,level)|*level);}}
        }
        self.devices=devices.clone();devices
    }
}

/// 释放服务时唤醒后台线程，停止后续扫描。
pub struct Service {
    pub rx: std::sync::mpsc::Receiver<Report>,
    stop: std::sync::mpsc::Sender<()>,
}
impl Drop for Service {
    fn drop(&mut self) {let _=self.stop.send(());}
}

pub fn spawn() -> Service {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let (stop, stopped)=std::sync::mpsc::channel();
    std::thread::Builder::new().name("daozai-bluetooth".into()).spawn(move || {
        #[cfg(windows)]
        let mut cache=Cache::default();
        loop {
            if stopped.try_recv()!=Err(std::sync::mpsc::TryRecvError::Empty) {return;}
            #[cfg(windows)]
            if let Ok(found)=query(&mut cache) {
                if matches!(tx.try_send(found),Err(std::sync::mpsc::TrySendError::Disconnected(_))) {return;}
            }
            #[cfg(not(windows))]
            let _=&tx;
            if stopped.recv_timeout(Duration::from_secs(2))!=Err(std::sync::mpsc::RecvTimeoutError::Timeout) {return;}
        }
    }).ok();
    Service {rx,stop}
}

#[cfg(windows)]
fn query(cache:&mut Cache) -> Result<Report, ()> {
    use windows::Devices::Bluetooth::{BluetoothConnectionStatus, BluetoothDevice, BluetoothLEDevice};
    use windows::Devices::Enumeration::DeviceInformation;
    let selectors = [
        (true, BluetoothDevice::GetDeviceSelectorFromConnectionStatus(BluetoothConnectionStatus::Connected).map_err(|_| ())?),
        (false, BluetoothLEDevice::GetDeviceSelectorFromConnectionStatus(BluetoothConnectionStatus::Connected).map_err(|_| ())?),
    ];
    let mut devices = Vec::new();
    for (classic, selector) in selectors {
        let selector = windows::core::HSTRING::from(format!("({selector}) AND System.Devices.Aep.IsConnected:=System.StructuredQueryType.Boolean#True"));
        let found = DeviceInformation::FindAllAsyncAqsFilter(&selector).map_err(|_| ())?.get().map_err(|_| ())?;
        for info in found {
            let Ok(id) = info.Id() else { continue };
            let Ok(name) = info.Name() else { continue };
            let name = name.to_string();
            if name.trim().is_empty() { continue; }
            let address = id.to_string().rsplit('-').next().unwrap_or_default().replace(':', "").to_ascii_uppercase();
            if devices.iter().any(|d: &Device| d.id == address) { continue; }
            let kind=if let Some(old)=cache.devices.iter().find(|old|old.id==address && old.name==name){old.kind}else{
                let class=if classic {
                    BluetoothDevice::FromIdAsync(&id).ok().and_then(|a|a.get().ok()).and_then(|d|{
                        let class=d.ClassOfDevice().ok().and_then(|c|c.RawValue().ok());let _=d.Close();class
                    })
                }else{
                    BluetoothLEDevice::FromIdAsync(&id).ok().and_then(|a|a.get().ok()).and_then(|d|{
                        let class=d.Appearance().ok().and_then(|c|c.RawValue().ok()).map(u32::from);let _=d.Close();class
                    })
                };
                device_kind(&name,classic,class)
            };
            devices.push(Device { id: address, kind, name, battery: None });
        }
    }
    devices.sort_by(|a,b|a.id.cmp(&b.id));
    let devices = cache.update(devices,Instant::now(),read_batteries);
    // 連線 AEP 沒回報電量時（例如 iPhone 的 BLE 鏈路暫時閒置），再從仍在場的 PnP 節點補最後已知電量。
    let present_nodes = scan_present_nodes();
    let present_power = present_power(&present_nodes);
    Ok(Report { devices, present_power })
}

/// 列舉仍在場的藍牙 PnP 節點，讀取名稱、位址與最後一次電量。
#[cfg(windows)]
fn scan_present_nodes() -> Vec<PresentNode> {
    use windows::core::PCWSTR;
    use windows::Win32::Devices::DeviceAndDriverInstallation::*;
    use windows::Win32::Devices::Properties::DEVPROPKEY;
    use windows::Win32::Foundation::HWND;

    // DEVPKEY_Bluetooth_Battery {104EA319-6EE2-4701-BD47-8DDBF425BBE5}, 2
    const BATTERY: DEVPROPKEY = DEVPROPKEY {
        fmtid: windows::core::GUID::from_u128(0x104ea319_6ee2_4701_bd47_8ddbf425bbe5),
        pid: 2,
    };
    let mut nodes = Vec::new();
    unsafe {
        let set = match SetupDiGetClassDevsW(
            None, PCWSTR::null(), HWND::default(), DIGCF_PRESENT | DIGCF_ALLCLASSES)
        { Ok(handle) => handle, Err(_) => return nodes };
        for i in 0..u32::MAX {
            let mut data = SP_DEVINFO_DATA {
                cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
                ..Default::default()
            };
            if SetupDiEnumDeviceInfo(set, i, &mut data).is_err() { break; }
            let Some(instance) = instance_id(set, &data) else { continue };
            let upper = instance.to_ascii_uppercase();
            // 只看藍牙實體裝置節點（BTHLE\DEV_ 或 BTHENUM\DEV_），略過 GATT 服務與協定節點。
            let classic = upper.starts_with("BTHENUM\\DEV_");
            let is_le = upper.starts_with("BTHLE\\DEV_");
            if !classic && !is_le { continue; }
            let Some(address) = upper.split("DEV_").nth(1).map(|s| s.split('\\').next().unwrap_or(s)) else { continue };
            let Some(level) = byte_prop(set, &data, &BATTERY).filter(|b| *b <= 100) else { continue };
            let name = string_prop(set, &data, &DEVICE_FRIENDLY_NAME).unwrap_or_else(|| address.to_string());
            nodes.push(PresentNode { name, address: address.to_string(), battery: level, classic });
        }
        let _ = SetupDiDestroyDeviceInfoList(set);
    }
    nodes
}

#[cfg(windows)]
fn read_batteries(devices:&[Device])->Result<Vec<(String,u8)>,()> {
    use windows::core::{GUID, PCWSTR};
    use windows::Win32::Devices::DeviceAndDriverInstallation::*;
    use windows::Win32::Devices::Properties::DEVPROPKEY;
    use windows::Win32::Foundation::HWND;

    // DEVPKEY_Bluetooth_Battery {104EA319-6EE2-4701-BD47-8DDBF425BBE5}, 2
    const BATTERY: DEVPROPKEY = DEVPROPKEY {
        fmtid: GUID::from_u128(0x104ea319_6ee2_4701_bd47_8ddbf425bbe5),
        pid: 2,
    };
    let mut batteries: Vec<(String, u8)> = Vec::new();
    unsafe {
        let set = SetupDiGetClassDevsW(None, PCWSTR::null(), HWND::default(), DIGCF_PRESENT | DIGCF_ALLCLASSES)
            .map_err(|_| ())?;
        for i in 0..u32::MAX {
            let mut data = SP_DEVINFO_DATA {
                cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
                ..Default::default()
            };
            if SetupDiEnumDeviceInfo(set, i, &mut data).is_err() {
                break;
            }
            let Some(instance) = instance_id(set, &data) else { continue };
            let upper = instance.to_ascii_uppercase();
            if !devices.iter().any(|device|upper.contains(&device.id)){continue;}
            if let Some(level) = byte_prop(set, &data, &BATTERY).filter(|b| *b <= 100) {
                batteries.push((upper.clone(), level));
            }
        }
        // 每次枚举后释放设备列表句柄。
        let _ = SetupDiDestroyDeviceInfoList(set);
    }
    Ok(devices.iter().filter_map(|device|batteries.iter().find(|(node,_)|node.contains(&device.id)).map(|(_,level)|(device.id.clone(),*level))).collect())
}

#[cfg(windows)]
unsafe fn instance_id(
    set: windows::Win32::Devices::DeviceAndDriverInstallation::HDEVINFO,
    data: &windows::Win32::Devices::DeviceAndDriverInstallation::SP_DEVINFO_DATA,
) -> Option<String> {
    use windows::Win32::Devices::DeviceAndDriverInstallation::SetupDiGetDeviceInstanceIdW;
    let mut buf = [0u16; 512];
    let mut need = 0u32;
    SetupDiGetDeviceInstanceIdW(set, data, Some(&mut buf), Some(&mut need)).ok()?;
    Some(String::from_utf16_lossy(&buf[..(need as usize).saturating_sub(1)]))
}

#[cfg(windows)]
unsafe fn byte_prop(
    set: windows::Win32::Devices::DeviceAndDriverInstallation::HDEVINFO,
    data: &windows::Win32::Devices::DeviceAndDriverInstallation::SP_DEVINFO_DATA,
    key: &windows::Win32::Devices::Properties::DEVPROPKEY,
) -> Option<u8> {
    use windows::Win32::Devices::DeviceAndDriverInstallation::SetupDiGetDevicePropertyW;
    use windows::Win32::Devices::Properties::DEVPROPTYPE;
    // DEVPROP_TYPE_BYTE
    const BYTE: DEVPROPTYPE = DEVPROPTYPE(0x3);
    let mut ty = DEVPROPTYPE::default();
    let mut buf = [0u8; 1];
    let mut size = 0u32;
    SetupDiGetDevicePropertyW(set, data, key, &mut ty, Some(&mut buf), Some(&mut size), 0).ok()?;
    (ty == BYTE && size == 1).then_some(buf[0])
}

/// DEVPKEY_Device_FriendlyName {A45C254E-DF1C-4EFD-8020-67D146A850E0}, 14
#[cfg(windows)]
const DEVICE_FRIENDLY_NAME: windows::Win32::Devices::Properties::DEVPROPKEY =
    windows::Win32::Devices::Properties::DEVPROPKEY {
        fmtid: windows::core::GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
        pid: 14,
    };

#[cfg(windows)]
unsafe fn string_prop(
    set: windows::Win32::Devices::DeviceAndDriverInstallation::HDEVINFO,
    data: &windows::Win32::Devices::DeviceAndDriverInstallation::SP_DEVINFO_DATA,
    key: &windows::Win32::Devices::Properties::DEVPROPKEY,
) -> Option<String> {
    use windows::Win32::Devices::DeviceAndDriverInstallation::SetupDiGetDevicePropertyW;
    use windows::Win32::Devices::Properties::DEVPROPTYPE;
    // DEVPROP_TYPE_STRING (UTF-16, NUL-terminated)
    const STRING: DEVPROPTYPE = DEVPROPTYPE(0x12);
    let mut ty = DEVPROPTYPE::default();
    let mut needed = 0u32;
    // 先探大小
    SetupDiGetDevicePropertyW(set, data, key, &mut ty, None, Some(&mut needed), 0).ok()?;
    if ty != STRING || needed == 0 { return None; }
    let mut buf = vec![0u16; (needed as usize) / 2];
    let byte_slice = unsafe {
        std::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, buf.len() * std::mem::size_of::<u16>())
    };
    SetupDiGetDevicePropertyW(set, data, key, &mut ty, Some(byte_slice), Some(&mut needed), 0).ok()?;
    let len = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(id:&str)->Device {Device{id:id.into(),name:id.into(),battery:None,kind:DeviceKind::Mouse}}

    #[test]
    fn idle_bluetooth_does_not_rescan_all_hardware_every_two_seconds(){
        let start=std::time::Instant::now();let mut cache=Cache::default();let mut scans=0;
        for second in (0..360).step_by(2){
            let found=cache.update(vec![device("AA")],start+Duration::from_secs(second),|_|{scans+=1;Ok(vec![("AA".into(),80)])});
            assert_eq!(found[0].battery,Some(80));
        }
        assert_eq!(scans,6,"六分鐘內同一裝置的電量掃描應最多每分鐘一次");
    }

    #[test]
    fn bluetooth_connection_change_refreshes_immediately_and_failed_scans_back_off(){
        let now=std::time::Instant::now();let mut cache=Cache::default();
        cache.update(vec![device("AA")],now,|_|Ok(vec![("AA".into(),80)]));
        let found=cache.update(vec![device("AA"),device("BB")],now+Duration::from_secs(2),|_|Ok(vec![("AA".into(),79),("BB".into(),60)]));
        assert_eq!(found[1].battery,Some(60));
        let found=cache.update(vec![device("AA"),device("BB")],now+Duration::from_secs(62),|_|Err(()));
        assert_eq!(found[0].battery,Some(79));
        cache.update(vec![device("AA"),device("BB")],now+Duration::from_secs(64),|_|panic!("失敗後不能立即重複掃描"));
        assert!(cache.update(vec![],now+Duration::from_secs(66),|_|panic!("無裝置不能掃描整機")).is_empty());
        cache.update(vec![device("AA")],now+Duration::from_secs(68),|_|Ok(vec![("AA".into(),78)]));
        assert_eq!(cache.devices[0].battery,Some(78));
    }

    #[test]
    fn identifies_connected_audio_and_input_devices() {
        assert_eq!(device_kind("OpenMove by Shokz", true, Some(0x240418)), DeviceKind::Bone);
        assert_eq!(device_kind("QingLong 87 Ultra 8K", false, Some(961)), DeviceKind::Keyboard);
        assert_eq!(device_kind("MX Anywhere", false, Some(962)), DeviceKind::Mouse);
        assert_eq!(device_kind("耳機", true, Some(0x0404)), DeviceKind::Headphones);
        assert_eq!(device_kind("AirPods", true, None), DeviceKind::Earbuds);
        assert_eq!(device_kind("iPhone 13 Por Max", false, None), DeviceKind::Phone);
        assert_eq!(device_kind("未知裝置", false, None), DeviceKind::Other);
    }

    #[test]
    fn phone_battery_wins_over_earbuds_but_missing_phone_falls_back_to_audio() {
        let mut phone=device("PHONE");phone.kind=DeviceKind::Phone;phone.battery=Some(77);
        let mut headset=device("AA");headset.kind=DeviceKind::Headphones;headset.battery=Some(80);
        assert_eq!(battery_with_fallback(&[headset.clone(),phone.clone()]).map(|d|d.id.as_str()),Some("PHONE"));
        let mut empty_phone=phone.clone();empty_phone.battery=None;
        assert_eq!(battery_with_fallback(&[headset,empty_phone]).map(|d|d.id.as_str()),Some("AA"));
        assert!(battery_with_fallback(&[device("EE")]).is_none(),"沒回報電量的裝置不能硬顯示");
    }

    #[test]
    fn present_node_power_keeps_last_known_phone_battery_when_link_idle() {
        let phone = PresentNode { name: "iPhone 13 Por Max".into(), address: "4FA8446CAF75".into(), battery: 99, classic: false };
        let headset = PresentNode { name: "耳機".into(), address: "AA".into(), battery: 80, classic: true };
        // 手機優先，即使 BLE 鏈路閒置（不在連線 AEP 清單）也能顯示最後已知電量。
        assert_eq!(present_power(&[headset.clone(), phone.clone()]), Some((99, DeviceKind::Phone)));
        // 沒有手機時退回音訊裝置，並取電量最低的那顆。
        let other = PresentNode { name: "Speaker".into(), address: "BB".into(), battery: 25, classic: true };
        assert_eq!(present_power(&[headset, other]), Some((25, DeviceKind::Speaker)));
        // 沒有任何在場節點回報電量時不硬顯示。
        assert!(present_power(&[]).is_none());
    }

    #[test]
    fn summary_omits_missing_battery() {
        let d = Device { id: "1".into(), name: "Headset".into(), battery: None, kind: DeviceKind::Headphones };
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

#[cfg(test)]
mod lifecycle_tests {
    #[test]
    fn dropping_service_signals_stop_even_without_successful_scans(){
        let (_tx,rx)=std::sync::mpsc::channel();
        let (stop,stopped)=std::sync::mpsc::channel();
        drop(super::Service {rx,stop});
        assert_eq!(stopped.recv_timeout(std::time::Duration::from_millis(100)),Ok(()));
    }
}

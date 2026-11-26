//! 多來源快照：只按明確身份合併，保留讀數來源、時間和連線代次。
use std::collections::BTreeSet;
use std::time::{Duration,Instant};

pub const FRESH_FOR:Duration=Duration::from_secs(180);

#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub enum DeviceKind {
    Headphones,Earbuds,EarbudsStem,EarbudsStemTips,OpenEar,Bone,HearingAid,Speaker,Mic,
    Keyboard,Mouse,Trackpad,Pen,Presenter,Remote,Gamepad,JoyCon,Phone,Tablet,Watch,Band,Health,Receiver24G,
    #[default] Other,
}
impl DeviceKind {
    /// 連線後會接管系統聲音的裝置；用於收起音量提示。
    pub fn is_audio(self)->bool {matches!(self,Self::Headphones|Self::Earbuds|Self::EarbudsStem|Self::EarbudsStemTips|Self::OpenEar|Self::Bone|Self::HearingAid|Self::Speaker)}
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum BatterySource {Windows,Gatt}
impl BatterySource {
    pub fn label(self)->&'static str {match self {Self::Windows=>"Windows 裝置電量",Self::Gatt=>"藍牙電池服務"}}
}

#[derive(Clone,Debug,PartialEq,Eq)]
pub struct BatteryReading {
    pub percent:u8,
    pub source:BatterySource,
    /// 該來源最近一次成功讀取的時間。
    pub observed:Instant,
    /// 本機首次觀察到當前讀數的時間。Windows 連續讀到相同電量時保留這個值。
    pub value_since:Instant,
    /// None 表示來源沒有上報充電狀態。
    pub charging:Option<bool>,
}
impl BatteryReading {
    pub(crate) fn new(percent:u8,source:BatterySource,observed:Instant,charging:Option<bool>)->Self {
        Self{percent,source,observed,value_since:observed,charging}
    }
}

#[derive(Clone,Debug,PartialEq,Eq)]
pub struct Device {
    pub id:String,
    pub name:String,
    pub battery:Option<u8>,
    pub kind:DeviceKind,
    pub reading:Option<BatteryReading>,
    pub connected:bool,
}
impl Default for Device {
    fn default()->Self {Self{id:String::new(),name:String::new(),battery:None,kind:DeviceKind::Other,reading:None,connected:true}}
}
impl Device {
    pub fn stale_at(&self,now:Instant)->bool {!self.connected||self.reading.as_ref().is_some_and(|r|now.saturating_duration_since(r.observed)>FRESH_FOR)}
    pub fn summary(&self)->String {match self.battery.filter(|_|!self.stale_at(Instant::now())) {Some(b)=>format!("{} · 電量 {b}%",self.name),None=>self.name.clone()}}
}

#[derive(Clone,Debug)]
pub(super) struct Endpoint {
    pub id:String,pub address:Option<String>,pub container:Option<String>,pub name:String,pub kind:DeviceKind,pub ble:bool,
}
impl Endpoint {
    fn aliases(&self)->BTreeSet<String> {
        let mut aliases=BTreeSet::from([format!("endpoint:{}",self.id.to_ascii_uppercase())]);
        if let Some(address)=self.address.as_deref().and_then(normalize_address){aliases.insert(format!("mac:{address}"));}
        if let Some(container)=self.container.as_ref().filter(|v|!v.is_empty()){aliases.insert(format!("container:{}",container.to_ascii_uppercase()));}
        aliases
    }
}
#[derive(Clone,Debug)]
pub(super) struct Target {pub endpoint:Endpoint,pub key:String,pub session:u64}
#[derive(Clone,Debug)]
pub(super) struct Sample {pub key:String,pub session:u64,pub reading:BatteryReading}

struct SourceReading { value:BatteryReading, active:bool }

struct Entry {
    key:String,aliases:BTreeSet<String>,name:String,kind:DeviceKind,connected:bool,seen:bool,last_seen:Instant,session:u64,readings:Vec<SourceReading>,
}
#[derive(Default)]
pub(super) struct State {entries:Vec<Entry>,endpoints:Vec<Endpoint>,generation:u64}
impl State {
    pub fn discover(&mut self,mut endpoints:Vec<Endpoint>,now:Instant) {
        endpoints.sort_by(|a,b|a.id.cmp(&b.id));
        for entry in &mut self.entries {entry.seen=false;}
        for endpoint in &endpoints {
            let aliases=endpoint.aliases();
            let mut matches:Vec<_>=self.entries.iter().enumerate().filter(|(_,e)|!e.aliases.is_disjoint(&aliases)).map(|(i,_)|i).collect();
            if matches.is_empty(){
                self.generation+=1;
                let key=endpoint.address.as_deref().and_then(normalize_address).map(|v|format!("mac:{v}")).unwrap_or_else(||format!("endpoint:{}",endpoint.id.to_ascii_uppercase()));
                self.entries.push(Entry{key,aliases:aliases.clone(),name:endpoint.name.clone(),kind:endpoint.kind,connected:true,seen:true,last_seen:now,session:self.generation,readings:vec![]});
                continue;
            }
            let first=matches.remove(0);
            for index in matches.into_iter().rev(){
                let other=self.entries.remove(index);
                let entry=&mut self.entries[first];entry.aliases.extend(other.aliases);
                for reading in other.readings {merge_reading(&mut entry.readings,reading);}
            }
            let entry=&mut self.entries[first];
            if !entry.connected {
                self.generation+=1;entry.session=self.generation;
                // 重連後須重新取值，同一個 Windows 緩存值仍沿用首次觀察時間。
                entry.readings.retain(|r|r.value.source==BatterySource::Windows);
                for r in &mut entry.readings {r.active=false;}
            }
            // 同一輪按端點 ID 排序，優先使用首個端點的名稱。
            if !entry.seen&&!endpoint.name.trim().is_empty() {entry.name.clone_from(&endpoint.name);}
            entry.aliases.extend(aliases);entry.connected=true;entry.seen=true;entry.last_seen=now;
            if entry.kind==DeviceKind::Other {entry.kind=endpoint.kind;}
        }
        for entry in &mut self.entries {entry.connected=entry.seen;}
        self.entries.retain(|e|e.connected||now.saturating_duration_since(e.last_seen)<Duration::from_secs(600));
        self.endpoints=endpoints;
    }
    pub fn targets(&self)->Vec<Target> {
        self.endpoints.iter().filter_map(|endpoint|{
            let aliases=endpoint.aliases();let entry=self.entries.iter().find(|e|e.connected&&!e.aliases.is_disjoint(&aliases))?;
            Some(Target{endpoint:endpoint.clone(),key:entry.key.clone(),session:entry.session})
        }).collect()
    }
    pub fn ingest(&mut self,sample:Sample) {
        if sample.reading.percent>100 {return;}
        if let Some(entry)=self.entries.iter_mut().find(|e|e.key==sample.key&&e.session==sample.session&&e.connected){merge_reading(&mut entry.readings,SourceReading{value:sample.reading,active:true});}
    }
    pub fn devices(&self,now:Instant)->Vec<Device> {
        let mut devices:Vec<_>=self.entries.iter().map(|e|{
            let reading=e.readings.iter().filter(|r|r.active).map(|r|&r.value).max_by_key(|r|{
                let fresh=now.saturating_duration_since(r.observed)<=FRESH_FOR;
                (fresh,fresh&&r.source==BatterySource::Gatt,r.observed)
            }).cloned();
            Device{id:e.key.clone(),name:e.name.clone(),kind:e.kind,battery:reading.as_ref().map(|r|r.percent),reading,connected:e.connected}
        }).collect();
        devices.sort_by(|a,b|b.connected.cmp(&a.connected).then_with(||a.id.cmp(&b.id)));devices
    }
}
fn merge_reading(readings:&mut Vec<SourceReading>,mut reading:SourceReading) {
    let Some(old)=readings.iter_mut().find(|r|r.value.source==reading.value.source) else {
        readings.push(reading);
        return;
    };
    if reading.value.observed<=old.value.observed {return;}
    if reading.value.source==BatterySource::Windows&&old.value.percent==reading.value.percent {
        reading.value.value_since=old.value.value_since.min(reading.value.value_since);
    }
    *old=reading;
}
pub(super) fn normalize_address(value:&str)->Option<String> {
    let value:String=value.chars().filter(|c|!matches!(c,':'|'-')).collect();
    (value.len()==12&&value.bytes().all(|c|c.is_ascii_hexdigit())&&value!="000000000000").then(||value.to_ascii_uppercase())
}
pub(super) fn pnp_address(instance:&str)->Option<String> {
    let upper = instance.to_ascii_uppercase();
    let mut parts = upper.split('\\');
    let (bus, hardware, instance) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() { return None; }
    let remote = match bus {
        "BTHENUM" | "BTHLE" if hardware.starts_with("DEV_") => &hardware[4..],
        "BTHLEDEVICE" if hardware.starts_with('{') => hardware.rsplit_once('_')?.1,
        "BTHENUM" if hardware.starts_with('{') => {
            let tail = instance.rsplit_once('&')?.1;
            let (address, suffix) = tail.split_once("_C")?;
            if suffix.len() != 8 || !suffix.bytes().all(|b| b.is_ascii_hexdigit()) { return None; }
            address
        }
        _ => return None,
    };
    if remote.len() != 12 { return None; }
    normalize_address(remote)
}

#[cfg(test)]
#[path="state_tests.rs"]
mod tests;

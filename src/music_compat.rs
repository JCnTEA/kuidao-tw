//! 音樂播放器相容狀態：每個播放器的版本，以及進度、時長、跳轉等每一項能不能用、為什麼不能用。
//! 讀取模塊寫入，島上據此決定是否顯示歌詞、是否提醒更新播放器；也負責儲存「自動適配」學到的讀取位置。
use std::{collections::HashMap,sync::Mutex};

/// 播放器的一項能力。
#[derive(Clone,Copy,PartialEq,Eq,Hash,Debug)]
pub enum Item {
    /// 播放進度（歌詞同步依賴它）。
    Progress,
    /// 歌曲時長。
    Duration,
    /// 拖動進度條跳轉。
    Seek,
    /// 播放器本機數據整體（波點、酷我：歌名、進度、歌詞都從這裡來）。
    Reader,
}

#[derive(Clone,PartialEq,Debug)]
pub enum Support {
    /// 已適配（版本在適配表裡）。
    Ok,
    /// 不在適配表裡，但已按數據特徵自動找到。
    Learned,
    /// 正在按數據特徵自動適配（需要播放幾秒）。
    Learning,
    /// 不可用，附原因。
    Unavailable(String),
}

#[derive(Clone,Debug)]
pub struct PlayerStatus {
    pub exe:&'static str,
    pub version:Option<String>,
    pub items:Vec<(Item,Support)>,
}
impl PlayerStatus {
    pub fn get(&self,item:Item)->Option<&Support> {self.items.iter().find(|(i,_)|*i==item).map(|(_,s)|s)}
}

pub fn display_name(exe:&str)->&'static str {
    match exe.to_ascii_lowercase().as_str() {
        "kugou.exe"=>"酷狗音樂","cloudmusic.exe"=>"網易雲音樂","bodian_pc.exe"=>"波點音樂","kwmusic.exe"=>"酷我音樂",
        "qqmusic.exe"=>"QQ 音樂","sodamusic.exe"=>"汽水音樂","spotify.exe"=>"Spotify","applemusic.exe"=>"Apple Music",_=>"播放器",
    }
}

static PLAYERS:Mutex<Vec<PlayerStatus>>=Mutex::new(Vec::new());

fn with_player<T>(exe:&'static str,f:impl FnOnce(&mut PlayerStatus)->T)->T {
    let mut players=PLAYERS.lock().unwrap_or_else(|e|e.into_inner());
    let index=match players.iter().position(|p|p.exe==exe) {
        Some(index)=>index,
        None=>{players.push(PlayerStatus{exe,version:None,items:Vec::new()});players.len()-1}
    };
    f(&mut players[index])
}

pub fn set_version(exe:&'static str,version:&str) {with_player(exe,|p|p.version=Some(version.to_owned()));}

pub fn set(exe:&'static str,item:Item,support:Support) {
    with_player(exe,|p|match p.items.iter_mut().find(|(i,_)|*i==item) {
        Some((_,old))=>*old=support,
        None=>p.items.push((item,support)),
    });
}

/// 播放器退出或重啟後，舊進程的結論作廢。
pub fn clear(exe:&'static str,item:Item) {
    let mut players=PLAYERS.lock().unwrap_or_else(|e|e.into_inner());
    if let Some(player)=players.iter_mut().find(|p|p.exe==exe){player.items.retain(|(i,_)|*i!=item);}
}

pub fn get(exe:&str)->Option<PlayerStatus> {
    PLAYERS.lock().unwrap_or_else(|e|e.into_inner()).iter().find(|p|p.exe.eq_ignore_ascii_case(exe)).cloned()
}

// ---- 版本較舊：島上提醒更新一次 ----

/// 待顯示的更新提醒，以及已經提醒過的「播放器/版本」（同一版本只提醒一次，重啟後也不再提醒）。
static NOTICES:Mutex<(Vec<&'static str>,Option<std::collections::HashSet<String>>)>=Mutex::new((Vec::new(),None));

fn notices_path()->std::path::PathBuf {crate::model::config_dir().join("music-update-notices.txt")}

/// 播放器版本比已適配的還舊，導致歌詞無法同步：排一條更新提醒。比已適配的新則是島仔還沒適配，不打擾用戶。
pub fn outdated(exe:&'static str) {
    let version=get(exe).and_then(|p|p.version).unwrap_or_default();
    let key=format!("{exe}/{version}");
    let mut notices=NOTICES.lock().unwrap_or_else(|e|e.into_inner());
    let shown=notices.1.get_or_insert_with(||{
        if cfg!(test) {return Default::default();}
        std::fs::read_to_string(notices_path()).unwrap_or_default().lines().take(256).map(str::to_owned).collect()
    });
    if !shown.insert(key) {return;}
    if !cfg!(test) {
        let path=notices_path();
        if let Some(parent)=path.parent(){let _=std::fs::create_dir_all(parent);}
        let _=std::fs::write(path,shown.iter().cloned().collect::<Vec<_>>().join("\n"));
    }
    if !notices.0.contains(&exe) {notices.0.push(exe);}
}

/// 界面每幀取一次：有待顯示的提醒時返回提示語。
pub fn take_update_notice()->Option<String> {
    let mut notices=NOTICES.lock().unwrap_or_else(|e|e.into_inner());
    if notices.0.is_empty() {return None;}
    let name=display_name(notices.0.remove(0));
    Some(format!("{name}版本較舊，更新後可同步歌詞"))
}

/// 當前播放源的歌詞同步是否受版本限制（進度或本機數據不可用）：島上據此給出提示。
pub fn lyrics_limited(source:&str)->bool {
    let Some(exe)=super::music_log::player_exe(source) else{return false;};
    get(exe).is_some_and(|p|[Item::Progress,Item::Reader].iter().any(|item|matches!(p.get(*item),Some(Support::Unavailable(_)))))
}

// ---- 自動適配學到的讀取位置：按組件檔案名、PE 時間戳與映像大小區分版本，儲存在配置目錄 ----

fn store_path()->std::path::PathBuf {crate::model::config_dir().join("music-compat.json")}
fn key(module:&str,stamp:u32,size:u32)->String {format!("{}/{stamp:08x}/{size}",module.to_ascii_lowercase())}

static LEARNED:Mutex<Option<HashMap<String,usize>>>=Mutex::new(None);

fn load()->HashMap<String,usize> {
    if cfg!(test) {return HashMap::new();}
    let Ok(bytes)=std::fs::read(store_path()) else{return HashMap::new();};
    if bytes.len()>64*1024 {return HashMap::new();}
    serde_json::from_slice::<HashMap<String,usize>>(&bytes).unwrap_or_default()
}

fn save(map:&HashMap<String,usize>) {
    if cfg!(test) {return;}
    if let Ok(text)=serde_json::to_string_pretty(map) {
        let path=store_path();
        if let Some(parent)=path.parent(){let _=std::fs::create_dir_all(parent);}
        let _=std::fs::write(path,text);
    }
}

pub fn learned(module:&str,stamp:u32,size:u32)->Option<usize> {
    let mut store=LEARNED.lock().unwrap_or_else(|e|e.into_inner());
    store.get_or_insert_with(load).get(&key(module,stamp,size)).copied()
}

pub fn remember(module:&str,stamp:u32,size:u32,offset:usize) {
    let mut store=LEARNED.lock().unwrap_or_else(|e|e.into_inner());
    let map=store.get_or_insert_with(load);
    map.insert(key(module,stamp,size),offset);save(map);
}

pub fn forget(module:&str,stamp:u32,size:u32) {
    let mut store=LEARNED.lock().unwrap_or_else(|e|e.into_inner());
    let map=store.get_or_insert_with(load);
    if map.remove(&key(module,stamp,size)).is_some(){save(map);}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn items_are_tracked_per_player_and_limit_lyrics_only_for_progress_or_reader(){
        set("KuGou.exe",Item::Seek,Support::Unavailable("接口不可用".into()));
        assert!(!lyrics_limited("kugou"),"跳轉不可用不影響歌詞");
        set("KuGou.exe",Item::Progress,Support::Learning);
        assert!(!lyrics_limited("kugou"),"自動適配中先不提示");
        set("KuGou.exe",Item::Progress,Support::Unavailable("版本不匹配".into()));
        assert!(lyrics_limited("kugou"));
        set("KuGou.exe",Item::Progress,Support::Learned);
        assert!(!lyrics_limited("kugou"));
        assert!(get("kugou.exe").unwrap().get(Item::Seek).is_some_and(|s|matches!(s,Support::Unavailable(_))));
        clear("KuGou.exe",Item::Seek);
        assert!(get("KuGou.exe").unwrap().get(Item::Seek).is_none());
        assert!(!lyrics_limited("chrome.exe"));
    }
    #[test]
    fn outdated_players_are_reminded_once_per_version(){
        set_version("kwmusic.exe","9.2.0");
        outdated("kwmusic.exe");outdated("kwmusic.exe");
        assert_eq!(take_update_notice().as_deref(),Some("酷我音樂版本較舊，更新後可同步歌詞"));
        assert_eq!(take_update_notice(),None,"同一版本只提醒一次");
        set_version("kwmusic.exe","9.3.0");outdated("kwmusic.exe");
        assert!(take_update_notice().is_some(),"換了版本仍舊較舊，再提醒一次");
    }

    #[test]
    fn learned_offsets_are_keyed_by_module_version(){
        remember("KGPlayer.dll",0x1234,4096,0x3949f8);
        assert_eq!(learned("kgplayer.dll",0x1234,4096),Some(0x3949f8));
        assert_eq!(learned("kgplayer.dll",0x1235,4096),None,"其他版本不能借用");
        forget("kgplayer.dll",0x1234,4096);
        assert_eq!(learned("kgplayer.dll",0x1234,4096),None);
    }
}

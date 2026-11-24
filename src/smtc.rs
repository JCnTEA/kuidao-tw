//! 系统媒体会话（SMTC）读取：Windows 10/11 的“正在播放”。
//! 与原工具使用同一套 API（GlobalSystemMediaTransportControlsSessionManager）。

#[path="apple_music.rs"]
mod apple_music;
#[cfg(windows)]
#[path="netease_timeline.rs"]
mod netease_timeline;
#[cfg(windows)]
#[path="netease_playback.rs"]
mod netease_playback;
#[cfg(windows)]
#[path="bodian.rs"]
pub(crate) mod bodian;
#[cfg(windows)]
#[path="kuwo.rs"]
pub(crate) mod kuwo;
#[cfg(windows)]
#[path="kugou_metadata.rs"]
mod kugou_metadata;
#[cfg(windows)]
#[path="kugou_control.rs"]
mod kugou_control;
#[cfg(windows)]
#[path="player_seek.rs"]
mod player_seek;
#[cfg(windows)]
#[path="player_process.rs"]
pub(crate) mod player_process;
#[cfg(windows)]
#[path="music_log.rs"]
mod music_log;

#[cfg(windows)]
#[path="music_gaps.rs"]
mod music_gaps;

#[cfg(windows)]
#[path="cover_search.rs"]
mod cover_search;

use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MediaKind {
    Music,
    Video,
    #[default]
    Unknown,
}

#[derive(Clone, Debug, Default)]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub source: String,
    pub song_id: Option<u64>,
    pub playing: bool,
    pub position: f64,
    /// 播放位置的采样时刻，避免封面读取和线程交接消耗歌词提前量。
    pub sampled_at: Option<std::time::Instant>,
    /// 播放事件的 UTC 毫秒，用于比较系统会话与本机日志的新旧。
    pub event_time_ms: Option<u64>,
    pub duration: f64,
    pub artwork: Option<Arc<eframe::egui::ColorImage>>,
    pub playback_type: MediaKind,
    /// 当前播放器在 Windows 音量混合器中的音量与静音状态。
    pub volume: Option<(u32,bool)>,
    /// 歌名與歌手是「聽聲音辨識」補上的，不是播放器給的。
    /// 這種情況 duration 仍是影片長度（含前奏、片尾），查歌詞時不能拿來比對。
    pub identified: bool,
}

/// 台灣版開關：大陸音樂來源（網易雲、酷狗、汽水、酷我、波點、QQ 音樂）預設不上島。
/// 程式與歌詞適配完整保留，日後台灣使用者有需要時把這裡改成 true 即可恢復。
const CN_MUSIC_SOURCES_ENABLED: bool = false;

fn is_cn_music_source(source: &str) -> bool {
    matches!(source,"kugou"|"kugou.exe"|"cloudmusic.exe"|"汽水音乐"|"汽水音樂"|"sodamusic.exe"|"kwmusic.exe"|"bodian_pc.exe")
        || is_qqmusic(source)
}

/// 是否允許這個播放器上島；關閉時仍保留底層辨識、時鐘與歌詞測試。
fn source_enabled(source: &str) -> bool {
    CN_MUSIC_SOURCES_ENABLED || !is_cn_music_source(&source.to_lowercase())
}

/// Chrome／Edge／Firefox／IE 等瀏覽器也會把分頁音訊註冊到 Windows 播放卡；
/// 這裡只認來源，不要求使用者安裝擴充功能，也不看瀏覽器是否在前景。
pub fn is_browser_source(source:&str)->bool{
    let source=source.to_ascii_lowercase();
    // 瀏覽器自報的 SMTC 來源有時不是 exe 名稱，而是「www.youtube.com-<hash>!App」這種
    // AppUserModelId；它同樣是分頁音樂，必須走瀏覽器那條標題正規化，否則整串影片標題
    // 會被當成歌名送去比對，永遠對不到。
    if source.contains("youtube.com") || source.contains("youtube music") {return true;}
    ["chrome","msedge","firefox","brave","opera","vivaldi","iexplore","internet explorer"]
        .iter().any(|name|source.contains(name))
}

/// YouTube 分頁在 SMTC 常缺少專輯欄位，甚至被瀏覽器標成影片；
/// 專輯或標題後綴是唯一穩定的辨識點。
fn is_youtube(media:&NowPlaying)->bool{
    let title=media.title.trim().to_ascii_lowercase();
    let album=media.album.trim().to_ascii_lowercase();
    album.contains("youtube") || title.ends_with(" - youtube") || title.ends_with(" - youtube music")
}

impl NowPlaying {
    pub fn kind(&self)->MediaKind {
        if self.title.trim().is_empty(){return MediaKind::Unknown;}
        // 使用者在瀏覽器播放 YouTube／YouTube Music 時，Windows 播放卡仍會保留工作階段，
        // 瀏覽器縮小或切到別的分頁都能讀到歌名；不能因缺少專輯或型別被標成影片就被濾掉。
        if is_browser_source(&self.source) && is_youtube(self) {return MediaKind::Music;}
        if self.playback_type==MediaKind::Video {return MediaKind::Video;}
        // 部分浏览器把所有媒体都上报为音乐，补充识别明确的视频页面标题。
        let title=self.title.trim().to_lowercase();
        if [" - 抖音"," - youtube","_嗶哩嗶哩_bilibili"," - 嗶哩嗶哩"].iter().any(|suffix|title.ends_with(suffix)) {
            return MediaKind::Video;
        }
        let source=self.source.to_lowercase();
        if (matches!(source.as_str(),"kugou"|"kugou.exe"|"cloudmusic.exe"|"汽水音乐"|"汽水音樂"|"sodamusic.exe"|"spotify"|"spotify.exe"|"kwmusic.exe"|"bodian_pc.exe") || is_qqmusic(&source) || is_kkbox(&source) || apple_music::is_source(&source)) && self.playback_type==MediaKind::Music{return MediaKind::Music;}
        if ["chrome","msedge","firefox","brave","opera"].iter().any(|name|source.contains(name)) {
            let album=self.album.trim().to_lowercase();
            if ["youtube","bilibili","抖音","嗶哩嗶哩"].contains(&album.as_str()){return MediaKind::Video;}
            // 網頁版 KKBOX／Spotify 的歌名就在視窗標題裡，專輯欄被填成服務名，仍要當成歌曲。
            if ["kkbox","spotify","spotify web","網頁播放"].contains(&album.as_str()){return MediaKind::Music;}
            // 视频作者也可能被放进 Artist；浏览器缺少专辑信息时不自动认作歌曲。
            if album.is_empty(){return MediaKind::Unknown;}
        }
        if self.artist.trim().is_empty(){MediaKind::Unknown}else{MediaKind::Music}
    }

    pub fn is_music(&self)->bool {self.kind()==MediaKind::Music}
}

type ArtworkRequest=(String,Box<dyn FnOnce()->Option<Vec<u8>>+Send>);
struct ArtworkWorker {
    requests:std::sync::mpsc::Sender<ArtworkRequest>,
    results:std::sync::mpsc::Receiver<(String,Option<Vec<u8>>)>,
}
impl ArtworkWorker {
    fn start()->Option<Self>{
        let (requests,rx)=std::sync::mpsc::channel::<ArtworkRequest>();
        let (tx,results)=std::sync::mpsc::channel();
        crate::resources::ARTWORK.spawn(move||{
            let _apartment=init_winrt();
            while let Ok((key,load))=rx.recv(){if tx.send((key,load())).is_err(){break;}}
        })?;
        Some(Self{requests,results})
    }
}

#[derive(Default)]
struct ArtworkCache {
    #[cfg(windows)]
    kugou:kugou_metadata::Cache,
    #[cfg(windows)]
    system:Option<(NowPlaying,std::time::Instant)>,
    #[cfg(windows)]
    manager:Option<windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager>,
    key: String,
    bytes: Vec<u8>,
    image: Option<Arc<eframe::egui::ColorImage>>,
    worker: Option<ArtworkWorker>,
    pending: bool,
    next_poll: Option<std::time::Instant>,
    /// 连续读到同一张封面的次数；稳定后放慢复查，每次复查都要向播放器发 3 个系统请求并读出整张图。
    unchanged: u8,
}

impl ArtworkCache {
    fn clear(&mut self,keep_manager:bool){
        let worker=self.worker.take();let pending=self.pending;
        #[cfg(windows)] let manager=if keep_manager{self.manager.take()}else{None};
        #[cfg(not(windows))] let _=keep_manager;
        *self=Self{worker,pending,..Default::default()};
        #[cfg(windows)] {self.manager=manager;}
    }
    #[cfg(windows)]
    fn system_result(&mut self,result:Result<Option<NowPlaying>,()>,now:std::time::Instant)->Option<NowPlaying>{
        match result {
            Ok(media)=>{self.system=media.clone().map(|media|(media,now));media},
            Err(())=>self.system.as_ref().filter(|(_,at)|now.duration_since(*at)<=Duration::from_secs(2)).map(|(media,_)|media.clone()),
        }
    }
    fn poll(&mut self,key:String,load:impl FnOnce()->Option<Vec<u8>>+Send+'static)->Option<Arc<eframe::egui::ColorImage>> {
        self.poll_at(key,load,std::time::Instant::now())
    }
    fn poll_at(&mut self,key:String,load:impl FnOnce()->Option<Vec<u8>>+Send+'static,now:std::time::Instant)->Option<Arc<eframe::egui::ColorImage>> {
        self.get(key.clone(),||None);
        if let Some(worker)=&self.worker {
            match worker.results.try_recv() {
                Ok((loaded,bytes))=>{
                    self.pending=false;
                    if loaded==key {self.get(key.clone(),||bytes);}
                },
                Err(std::sync::mpsc::TryRecvError::Disconnected)=>{self.worker=None;self.pending=false;},
                Err(std::sync::mpsc::TryRecvError::Empty)=>{},
            }
        }
        // 最多一个封面读取任务；旧任务结束后只采纳当前歌曲的结果。
        if !self.pending && self.next_poll.is_none_or(|at|now>=at) {
            let interval=match (&self.image,self.unchanged) {(None,_)=>500,(Some(_),0..=2)=>2000,(Some(_),_)=>30_000};
            self.next_poll=Some(now+Duration::from_millis(interval));
            if self.worker.is_none(){self.worker=ArtworkWorker::start();}
            if let Some(worker)=&self.worker{self.pending=worker.requests.send((key,Box::new(load))).is_ok();}
        }
        self.image.clone()
    }
    fn get(&mut self, key:String, load:impl FnOnce()->Option<Vec<u8>>)->Option<Arc<eframe::egui::ColorImage>> {
        if self.key!=key {self.key=key;self.bytes.clear();self.image=None;self.next_poll=None;self.unchanged=0;}
        // 封面可能晚于歌名更新；每次检查内容，只在图片变化时重新解码。
        if let Some(bytes)=load() {
            if bytes==self.bytes {self.unchanged=self.unchanged.saturating_add(1);}
            else {
                self.unchanged=0;
                if let Some(image)=decode_artwork(&bytes) {
                    if self.image.as_deref()!=Some(&image){self.image=Some(Arc::new(image));}
                }
                self.bytes=bytes;
            }
        }
        self.image.clone()
    }
}

fn decode_artwork(bytes:&[u8])->Option<eframe::egui::ColorImage> {
    let decoded=(||{
        if bytes.len()>8*1024*1024{return None;}
        let mut reader=image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().ok()?;
        let mut limits=image::Limits::default();limits.max_image_width=Some(4096);limits.max_image_height=Some(4096);limits.max_alloc=Some(64*1024*1024);
        reader.limits(limits);reader.decode().ok()
    })();
    let Some(decoded)=decoded else{crate::resources::REJECTED_DATA.fetch_add(1,std::sync::atomic::Ordering::Relaxed);return None;};
    let image=decoded.thumbnail(256,256).to_rgba8();
    Some(eframe::egui::ColorImage::from_rgba_unmultiplied([image.width() as usize,image.height() as usize],image.as_raw()))
}

#[cfg(windows)]
fn media_result<T:windows::core::RuntimeType+'static>(start:impl FnOnce()->windows::core::Result<windows::Foundation::IAsyncOperation<T>>)->Option<T>{
    media_result_in(&crate::resources::ASYNC,start)
}

/// 封面请求单独计数：播放器的封面接口卡住时，最多占满封面自己的名额，不影响歌名、进度和播放控制。
#[cfg(windows)]
fn artwork_result<T:windows::core::RuntimeType+'static>(start:impl FnOnce()->windows::core::Result<windows::Foundation::IAsyncOperation<T>>)->Option<T>{
    media_result_in(&crate::resources::ASYNC_ARTWORK,start)
}

#[cfg(windows)]
fn media_result_in<T:windows::core::RuntimeType+'static>(lane:&'static crate::resources::Lane,start:impl FnOnce()->windows::core::Result<windows::Foundation::IAsyncOperation<T>>)->Option<T>{
    // 必须在创建 WinRT 请求之前占用名额；超时请求保留到确认终止。
    thread_local!{static RETRY_AT:std::cell::Cell<Option<std::time::Instant>>=const{std::cell::Cell::new(None)};}
    let now=std::time::Instant::now();
    if RETRY_AT.with(|at|at.get().is_some_and(|at|now<at)){return None;}
    crate::resources::PENDING.reap(now);
    let lease=lane.acquire()?;
    let operation=match start(){Ok(operation)=>operation,Err(_)=>{RETRY_AT.with(|at|at.set(Some(now+Duration::from_secs(2))));return None;}};
    let start=std::time::Instant::now();
    while !operation.Status().is_ok_and(|status|status!=windows::Foundation::AsyncStatus::Started){
        if start.elapsed()>=Duration::from_millis(500){
            let _=operation.Cancel();
            crate::resources::TIMEOUTS.fetch_add(1,std::sync::atomic::Ordering::Relaxed);
            let after=std::time::Instant::now()+Duration::from_secs(30);
            RETRY_AT.with(|at|at.set(Some(after)));
            crate::resources::PENDING.retain(lease,move||operation.Status().is_ok_and(|status|status!=windows::Foundation::AsyncStatus::Started) && operation.Close().is_ok(),after);
            return None;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let result=operation.GetResults().ok();let _=operation.Close();result
}

#[cfg(windows)]
fn read_artwork(props:&windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties)->Option<Vec<u8>> {
    use windows::core::Interface;
    use windows::Storage::Streams::DataReader;
    let stream=artwork_result(||props.Thumbnail()?.OpenReadAsync())?;
    let size=stream.Size().ok()?;
    if size==0 || size>8*1024*1024 {return None;}
    let reader=DataReader::CreateDataReader(&stream.GetInputStreamAt(0).ok()?).ok()?;
    let count=artwork_result::<u32>(||reader.LoadAsync(size as u32)?.cast())?;
    if count!=size as u32 {return None;}
    let mut bytes=vec![0;count as usize];
    reader.ReadBytes(&mut bytes).ok()?;
    Some(bytes)
}

/// QQ 音乐在系统媒体会话里的身份名：22.76 及以前是 `QQMusic.exe`，22.82 起改为 `Tencent.QQMusic`。
/// 进度拖动、音量都要认出它；会话查找仍用原始身份名。
pub(crate) fn is_qqmusic(source:&str)->bool {
    matches!(source.to_ascii_lowercase().as_str(),"qqmusic"|"qqmusic.exe"|"tencent.qqmusic")
}

/// KKBOX 桌面版與 Microsoft Store 版在 SMTC 的名稱不一致，統一辨識。
pub(crate) fn is_kkbox(source:&str)->bool {
    let source=source.to_ascii_lowercase();
    source=="kkbox" || source=="kkbox.exe" || source.contains("skysoft.kkbox") || source.contains("kkbox")
}

pub type Shared = Arc<Mutex<NowPlaying>>;

pub fn shared() -> Shared {
    Arc::new(Mutex::new(NowPlaying::default()))
}

fn choose_media(system:Option<NowPlaying>,native:Option<NowPlaying>)->Option<NowPlaying>{
    if let (Some(system),Some(native))=(&system,&native){
        if system.source.eq_ignore_ascii_case("cloudmusic.exe") && native.source.eq_ignore_ascii_case("cloudmusic.exe"){
            // SMTC 与网易云日志的歌手分隔符、合唱署名写法不同；按规范化后的歌名与歌手集合判断，避免同一首歌丢失 song_id。
            // 系统会话常只报主艺人（如「雄之助」对「雄之助、WaMi」），署名一方包含另一方也算同一首（#202）。
            let same=crate::lyrics::song_relation(&system.title,&system.artist,&native.title,&native.artist).is_some();
            let newer=match (native.event_time_ms,system.event_time_ms){(Some(n),Some(s))=>Some(n>s),_=>None};
            let use_native=newer.unwrap_or(same && system.playing==native.playing);
            let mut media=if use_native{native.clone()}else{system.clone()};
            if same {
                let fallback=if use_native{system}else{native};
                if media.artist.is_empty(){media.artist=fallback.artist.clone();}
                if media.duration<=0.0{media.duration=fallback.duration;}
                if media.song_id.is_none(){media.song_id=fallback.song_id;}
                if media.artwork.is_none(){media.artwork=system.artwork.clone().or_else(||native.artwork.clone());}
            }
            return Some(media);
        }
    }
    if system.as_ref().is_some_and(|m|m.playing && m.is_music()){system}
    else if native.as_ref().is_some_and(|m|m.playing && m.is_music()){native}
    else if system.as_ref().is_some_and(NowPlaying::is_music){system}
    else{native.or(system)}
}

fn media_rank(media:&NowPlaying)->u8{
    match (media.is_music(),media.playing){(true,true)=>0,(true,false)=>1,(false,true)=>2,_=>3}
}

fn choose_media_for_source(system:Option<NowPlaying>,native:Option<NowPlaying>,previous:&str,native_pending:bool)->Option<NowPlaying>{
    // 暂停或载入下一首时保留控制对象，其他客户端开始播放则照常切换。
    if previous.eq_ignore_ascii_case("cloudmusic.exe") && !system.as_ref().is_some_and(|m|m.playing && m.is_music())
        && (native.is_some() || native_pending) {
        if native.is_some() && system.as_ref().is_some_and(|m|m.source.eq_ignore_ascii_case(previous)){return choose_media(system,native);}
        return native;
    }
    choose_media(system,native)
}

#[cfg(windows)]
fn query(artwork:&mut ArtworkCache) -> Option<NowPlaying> {
    let result=query_system(artwork);
    let system=artwork.system_result(result,std::time::Instant::now());
    // 播放卡有「正在播放」的工作階段就直接用，時間軸與控制都最準。
    if system.as_ref().is_some_and(|media|media.playing && !media.title.trim().is_empty()) {return system;}
    // Windows 播放卡沒有登記瀏覽器分頁時（部分 Chrome／Edge 版本不註冊 SMTC），
    // 改用音量混音器正在出聲的瀏覽器 + 視窗標題當後備來源。
    crate::browser_media::probe().or(system)
}

#[cfg(windows)]
fn query_system(artwork:&mut ArtworkCache) -> Result<Option<NowPlaying>,()> {
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager;

    if artwork.manager.is_none(){artwork.manager=media_result(GlobalSystemMediaTransportControlsSessionManager::RequestAsync);}
    let mgr=artwork.manager.clone().ok_or(())?;
    let current=mgr.GetCurrentSession().ok();
    let mut sessions=current.iter().cloned().collect::<Vec<_>>();
    let all=mgr.GetSessions().map_err(|_|{artwork.manager=None;})?;
    for session in all{if !sessions.contains(&session){sessions.push(session);}}
    if sessions.is_empty(){return Ok(None);}
    // 先比较全部会话；视频不能遮挡音乐，封面只为最终选中的会话读取。
    let previous=artwork.system.as_ref().map(|(media,_)|media.source.clone());
    let mut unreadable_previous=false;
    let (session,mut media)=sessions.iter().filter_map(|session|{
        let media=read_session(session,artwork);
        if media.is_none() && session.SourceAppUserModelId().ok().is_some_and(|id|previous.as_deref()==Some(id.to_string().as_str())){unreadable_previous=true;}
        media.map(|media|(session,media))
    })
        .min_by_key(|(_,media)|media_rank(media)).ok_or(())?;
    music_log::player(&media.source);
    // 当前播放器正在重载时，不把暂停中的另一客户端当成用户切换了播放源。
    if media_rank(&media)>0 && unreadable_previous{return Err(());}
    let key=format!("{}|{}|{}|{}",media.source,media.title,media.artist,media.album);
    let session=session.clone();
    let title=media.title.clone();
    media.artwork=artwork.poll(key,move ||{
        let props=artwork_result(||session.TryGetMediaPropertiesAsync())?;
        if media_text(props.Title())?!=title{return None;}read_artwork(&props)
    });
    Ok(Some(media))
}

#[cfg(windows)]
fn media_text(value:windows::core::Result<windows::core::HSTRING>)->Option<String>{
    let value=value.unwrap_or_default();
    if value.len()>4096{crate::resources::REJECTED_DATA.fetch_add(1,std::sync::atomic::Ordering::Relaxed);return None;}
    Some(value.to_string())
}

#[cfg(windows)]
fn read_session(session:&windows::Media::Control::GlobalSystemMediaTransportControlsSession,artwork:&mut ArtworkCache)->Option<NowPlaying>{
    let props=media_result(||session.TryGetMediaPropertiesAsync())?;
    let title = media_text(props.Title())?;
    let mut artist = media_text(props.Artist())?;
    if title.is_empty() {
        return None;
    }
    let mut playing = false;
    let mut position = 0.0;
    let mut duration = 0.0;
    let mut sampled_at=None;
    let mut event_time_ms=None;
    let mut playback_type=props.PlaybackType().ok().and_then(|kind|kind.Value().ok());
    if let Ok(info) = session.GetPlaybackInfo() {
        let info_type=info.PlaybackType().ok().and_then(|kind|kind.Value().ok());
        if playback_type.is_none() || info_type.is_some_and(|kind|kind.0==2){playback_type=info_type;}
        if let Ok(status) = info.PlaybackStatus() {
            playing = status == windows::Media::Control::GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing;
        }
    }
    if let Ok(tl) = session.GetTimelineProperties() {
        event_time_ms=tl.LastUpdatedTime().ok().and_then(|at|at.UniversalTime.checked_sub(116444736000000000))
            .filter(|at|*at>0).map(|at|at as u64/10000);
        sampled_at=Some(std::time::Instant::now());
        if let Ok(p) = tl.Position() {
            position = p.Duration as f64 / 10_000_000.0;
        }
        // SMTC 只在事件发生时更新 Position；播放中要按 LastUpdatedTime 外推到「现在」。
        if playing {
            if let Ok(at) = tl.LastUpdatedTime() {
                let now = unsafe { windows::Win32::System::SystemInformation::GetSystemTimeAsFileTime() };
                let now = ((now.dwHighDateTime as i64) << 32) | now.dwLowDateTime as i64;
                let lag = (now - at.UniversalTime) as f64 / 10_000_000.0;
                if (0.0..86_400.0).contains(&lag) {
                    position += lag;
                }
            }
        }
        if let Ok(e) = tl.EndTime() {
            duration = e.Duration as f64 / 10_000_000.0;
        }
    }
    let source=media_text(session.SourceAppUserModelId())?;
    let mut has_position=position>0.0 || duration>0.0 || event_time_ms.is_some();
    if source.eq_ignore_ascii_case("cloudmusic.exe") && duration<=0.0 {
        if let Some((current,total))=netease_timeline::timeline() {
            position=current;duration=total;sampled_at=Some(std::time::Instant::now());has_position=true;
        }
    }
    if source.to_ascii_lowercase().contains("kugou") {
        if let Some((singer,total))=artwork.kugou.get(&title,&artist,duration<=0.0) {
            artist=singer;
            if duration<=0.0{duration=total;}
        }
        if let Some(current)=netease_timeline::kugou_position() {
            position=current;sampled_at=Some(std::time::Instant::now());has_position=true;
        }
    }
    let mut album=media_text(props.AlbumTitle())?;
    if apple_music::is_source(&source){
        (artist,album)=apple_music::metadata(artist,album);
    }
    if !has_position{sampled_at=None;}
    let playback_type=match playback_type.map(|kind|kind.0){Some(1)=>MediaKind::Music,Some(2)=>MediaKind::Video,_=>MediaKind::Unknown};
    Some(NowPlaying { title, artist, album, source, song_id:None, playing, position, sampled_at, event_time_ms, duration, artwork:None, playback_type, volume:None, identified:false })
}

#[cfg(not(windows))]
fn query(_artwork:&mut ArtworkCache) -> Option<NowPlaying> {
    None
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Transport {
    Toggle,
    Next,
    Prev,
    /// 跳到第几秒。
    Seek(f64),
    SetVolume(u32),
    SetMute(bool),
}

type ControlRequest=(Transport,String,std::time::Instant,Option<std::sync::mpsc::SyncSender<bool>>);
pub struct ControlWorker {requests:std::sync::mpsc::SyncSender<ControlRequest>}
impl ControlWorker {
    pub fn start(ctx:eframe::egui::Context)->Option<Self>{
        Self::start_with(ctx,|command,source|match command{
            Transport::Seek(seconds)=>seek(seconds,source),
            Transport::SetVolume(percent)=>crate::music_volume::set_volume(source,percent),
            Transport::SetMute(muted)=>crate::music_volume::set_mute(source,muted),
            _=>{control(command,source);true}
        })
    }
    fn start_with(ctx:eframe::egui::Context,handle:impl Fn(Transport,&str)->bool+Send+'static)->Option<Self>{
        let (requests,rx)=std::sync::mpsc::sync_channel::<ControlRequest>(8);
        crate::resources::CONTROL.spawn(move||{
            let _apartment=init_winrt();
            while let Ok(first)=rx.recv(){
                // 拖动进度条会连续排入多次跳转；QQ 音乐的跳转要遍历播放器界面并等待回读，只执行同一播放器的最后一次。
                let mut batch=vec![first];
                while let Ok(next)=rx.try_recv(){batch.push(next);}
                let mut batch=batch.into_iter().peekable();
                while let Some((command,source,created,result))=batch.next(){
                    let superseded=batch.peek().is_some_and(|next|next.1==source && matches!((command,next.0),
                        (Transport::Seek(_),Transport::Seek(_)) | (Transport::SetVolume(_),Transport::SetVolume(_))));
                    let success=if superseded {false}
                        else if created.elapsed()>Duration::from_secs(5){crate::resources::CONTROL_DROPPED.fetch_add(1,std::sync::atomic::Ordering::Relaxed);false}
                        else{handle(command,&source)};
                    if let Some(tx)=result{let _=tx.try_send(success);ctx.request_repaint();}
                }
            }
        })?;
        Some(Self{requests})
    }
    pub fn submit(&self,command:Transport,source:String)->Result<Option<std::sync::mpsc::Receiver<bool>>,()>{
        let (result,receiver)=if matches!(command,Transport::Seek(_)|Transport::SetVolume(_)|Transport::SetMute(_)){let (tx,rx)=std::sync::mpsc::sync_channel(1);(Some(tx),Some(rx))}else{(None,None)};
        self.requests.try_send((command,source,std::time::Instant::now(),result)).map_err(|_|{crate::resources::CONTROL_DROPPED.fetch_add(1,std::sync::atomic::Ordering::Relaxed);})?;
        Ok(receiver)
    }
}

/// 控制当前媒体会话（播放/暂停、上一首、下一首）。
#[cfg(windows)]
pub fn control(cmd: Transport,source:&str) {
    if let Transport::SetVolume(percent)=cmd {crate::music_volume::set_volume(source,percent);return;}
    if let Transport::SetMute(muted)=cmd {crate::music_volume::set_mute(source,muted);return;}
    if let Transport::Seek(seconds)=cmd {let _=seek(seconds,source);return;}
    if source.eq_ignore_ascii_case("bodian_pc.exe"){let _=bodian::control(cmd);return;}
    if source.eq_ignore_ascii_case("kwmusic.exe"){let _=kuwo::control(cmd);return;}
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager;
    let _apartment=init_winrt();
    let session=media_result(GlobalSystemMediaTransportControlsSessionManager::RequestAsync)
        .and_then(|mgr|mgr.GetSessions().ok()).and_then(|sessions|sessions.into_iter().find(|session|session.SourceAppUserModelId().ok().is_some_and(|id|id.to_string()==source)));
    let Some(session)=session else{
        if source.eq_ignore_ascii_case("cloudmusic.exe"){netease_playback::control(cmd);}
        return;
    };
    let _=media_result(||match cmd {
        Transport::Toggle => session.TryTogglePlayPauseAsync(),
        Transport::Next => session.TrySkipNextAsync(),
        Transport::Prev => session.TrySkipPreviousAsync(),
        Transport::Seek(secs) => session.TryChangePlaybackPositionAsync((secs.max(0.0) * 10_000_000.0) as i64),
        Transport::SetVolume(_)|Transport::SetMute(_)=>unreachable!(),
    });
}

#[cfg(not(windows))]
pub fn control(_cmd: Transport,_source:&str) {}

/// QQ 音乐自带的 `/seekto <秒>` 命令：由客户端的单实例通道转交给正在运行的 QQ 音乐，不依赖窗口和界面结构，
/// 最小化时同样有效。22.82 起界面改由 QBCore 渲染、点击进度条的回退失效，系统跳转也只回报成功不实际跳转，
/// 这是新版唯一可用的跳转方式（本机 22.82 实测）。只在 QQ 音乐已运行时调用其自身程序，不会启动新的客户端。
/// 方案来自 cayww 的 PR #162。
#[cfg(windows)]
fn qq_seek_command(seconds:f64)->bool{
    let Some(path)=player_process::pids_named("QQMusic.exe").into_iter().find_map(player_process::image_path) else{return false;};
    let directory=std::path::Path::new(&path).parent().map(std::path::Path::to_path_buf);
    let mut command=std::process::Command::new(&path);
    if let Some(directory)=directory {command.current_dir(directory);}
    command.arg("/seekto").arg((seconds.max(0.0).round() as u64).to_string())
        .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
        .spawn().is_ok()
}

/// 跳转在工作线程执行；专用回退必须回读实际位置，避免播放器接口假成功。
#[cfg(windows)]
pub fn seek(seconds:f64,source:&str)->bool {
    if !seconds.is_finite() || seconds<0.0{return false;}
    static LOCK:Mutex<()>=Mutex::new(());
    let Ok(_lock)=LOCK.lock()else{return false;};
    if source.eq_ignore_ascii_case("kwmusic.exe"){return kuwo::seek(seconds);}
    if matches!(source.to_ascii_lowercase().as_str(),"kugou"|"kugou.exe") {
        let native=kugou_control::seek(seconds);
        music_log::state("酷狗跳轉",if native{"本機介面可用"}else{"本機介面不可用，改用系統跳轉"});
        if native{return true;}
    }
    let _apartment=init_winrt();
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager as Manager;
    let session=media_result(Manager::RequestAsync).and_then(|mgr|mgr.GetSessions().ok())
        .and_then(|sessions|sessions.into_iter().find(|s|s.SourceAppUserModelId().ok().is_some_and(|id|id.to_string().eq_ignore_ascii_case(source))));
    if apple_music::is_source(source){return session.as_ref().is_some_and(|session|seek_apple(session,source,seconds));}
    let native=is_qqmusic(source);
    let timeline=||->Option<(f64,f64)>{
        let tl=session.as_ref()?.GetTimelineProperties().ok()?;
        Some((tl.Position().ok()?.Duration as f64/10_000_000.0,tl.EndTime().ok()?.Duration as f64/10_000_000.0))
    };
    let duration=timeline().map(|(_,duration)|duration).unwrap_or(0.0);
    let target=if duration>0.0{seconds.min(duration)}else{seconds};
    let enabled=session.as_ref().and_then(|s|s.GetPlaybackInfo().ok()).and_then(|i|i.Controls().ok())
        .and_then(|c|c.IsPlaybackPositionEnabled().ok()).unwrap_or(false);
    let paused_qq=native && session.as_ref().and_then(|s|s.GetPlaybackInfo().ok())
        .and_then(|info|info.PlaybackStatus().ok()).is_some_and(|status|status!=windows::Media::Control::GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing);
    let accepted=(!native || enabled) && session.as_ref().and_then(|s|media_result(||s.TryChangePlaybackPositionAsync((target*10_000_000.0)as i64)))==Some(true);
    if !native{return accepted;}
    // 依次尝试：系统接口 → QQ 音乐自带的 /seekto 命令 → 点击界面进度条（旧版）。每一级都回读位置确认。
    for stage in 0..3 {
        match stage {
            0=>{if !accepted{continue;}}
            1=>{if !qq_seek_command(target){continue;}}
            _=>{if !player_seek::seek(source,target,duration){return false;}}
        }
        let start=std::time::Instant::now();
        let timeout=match stage {0=>Duration::from_millis(600),1=>Duration::from_secs(3),_=>Duration::from_secs(2)};
        let mut pause_requested=false;
        while start.elapsed()<timeout {
            std::thread::sleep(Duration::from_millis(100));
            if timeline().is_some_and(|(current,total)|(total-duration).abs()<1.5 && seek_arrived(current,target,start.elapsed().as_secs_f64())){
                // QQ 的原生进度条会自动续播；确认续播事件后再恢复暂停，避免命令被覆盖。
                if paused_qq && session.as_ref().and_then(|s|s.GetPlaybackInfo().ok()).and_then(|i|i.PlaybackStatus().ok())
                    ==Some(windows::Media::Control::GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing){
                    if !pause_requested {
                        if session.as_ref().and_then(|s|media_result(||s.TryTogglePlayPauseAsync()))!=Some(true){return false;}
                        pause_requested=true;
                    }
                    continue;
                }
                return true;
            }
        }
    }
    false
}

#[cfg(windows)]
fn seek_apple(session:&windows::Media::Control::GlobalSystemMediaTransportControlsSession,source:&str,seconds:f64)->bool{
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status;
    let status=||session.GetPlaybackInfo().ok()?.PlaybackStatus().ok();
    let Some(before)=status()else{return false;};let paused=before!=Status::Playing;
    let Some(duration)=session.GetTimelineProperties().ok().and_then(|tl|tl.EndTime().ok()).map(|time|time.Duration as f64/10_000_000.0)else{return false;};
    if duration<=0.0{return false;}let target=seconds.min(duration);
    let wait=|timeout:Duration,ready:&dyn Fn()->bool|{
        let start=std::time::Instant::now();
        loop{if ready(){return true;}if start.elapsed()>=timeout{return false;}std::thread::sleep(Duration::from_millis(100));}
    };
    let success=(||{
        // Apple Music 暂停时忽略滑块设置；跳转完成后恢复原来的暂停状态。
        if paused && (media_result(||session.TryPlayAsync())!=Some(true) || !wait(Duration::from_secs(2),&||status()==Some(Status::Playing))){return false;}
        for attempt in 0..2 {
            if !player_seek::seek(source,target,duration){return false;}
            let start=std::time::Instant::now();
            let timeout=if attempt==0{Duration::from_millis(500)}else{Duration::from_secs(2)};
            if wait(timeout,&||session.GetTimelineProperties().ok().is_some_and(|tl|{
                tl.EndTime().ok().is_some_and(|total|(total.Duration as f64/10_000_000.0-duration).abs()<1.5)
                    && tl.Position().ok().is_some_and(|position|seek_arrived(position.Duration as f64/10_000_000.0,target,start.elapsed().as_secs_f64()))
            })){return true;}
        }
        false
    })();
    if paused {
        let mut accepted=media_result(||session.TryPauseAsync())==Some(true);
        let start=std::time::Instant::now();let mut paused_at=None;let mut seen_pause=false;let mut retried=false;
        // 原生滑块可能稍后再次续播，连续确认暂停后才结束；只补发一次暂停。
        while start.elapsed()<Duration::from_secs(3){
            let current=status();
            if current==Some(Status::Paused){
                seen_pause=true;let since=paused_at.get_or_insert_with(std::time::Instant::now);
                if since.elapsed()>=Duration::from_millis(500){return success && accepted;}
            }else{
                paused_at=None;
                if current==Some(Status::Playing) && seen_pause && !retried{accepted&=media_result(||session.TryPauseAsync())==Some(true);retried=true;}
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        return false;
    }
    success
}

#[cfg(not(windows))]
pub fn seek(_seconds:f64,_source:&str)->bool {false}

fn seek_arrived(current:f64,target:f64,elapsed:f64)->bool {
    current.is_finite() && target.is_finite() && current>=target-3.0 && current<=target+elapsed+3.0
}

#[test]
fn seek_confirmation_rejects_unchanged_position_and_accepts_playback_advance(){
    assert!(!seek_arrived(14.0,27.0,2.0));
    assert!(!seek_arrived(180.0,60.0,2.0));
    assert!(seek_arrived(60.0,60.0,0.2));
    assert!(seek_arrived(62.0,60.0,2.0));
    assert!(!seek_arrived(f64::NAN,60.0,2.0));
}

#[cfg(windows)]
struct MediaApartment(Option<windows::Win32::System::Com::CO_MTA_USAGE_COOKIE>);
#[cfg(windows)]
impl Drop for MediaApartment {fn drop(&mut self){if let Some(cookie)=self.0 {let _=unsafe{windows::Win32::System::Com::CoDecrementMTAUsage(cookie)};}}}
#[cfg(windows)]
fn init_winrt()->MediaApartment {MediaApartment(unsafe{windows::Win32::System::Com::CoIncrementMTAUsage().ok()})}

#[cfg(not(windows))]
fn init_winrt() {}

// 音乐客户端可能重复上报同一播放位置，保留该位置首次采样的时间。
#[derive(Default)]
struct PlaybackClock { previous: Option<NowPlaying> }
impl PlaybackClock {
    fn update(&mut self, media: &mut NowPlaying) {
        if !media.is_music() || media.sampled_at.is_none() {
            self.previous=None;
            return;
        }
        if let Some(previous)=&self.previous {
            if media.playing && previous.playing && media.source==previous.source
                && media.title==previous.title && media.artist==previous.artist && media.album==previous.album
                && media.song_id==previous.song_id
                && media.position==previous.position && previous.sampled_at.is_some() {
                media.sampled_at=previous.sampled_at;
            }
        }
        self.previous=Some(media.clone());
    }
}

#[test]
fn kugou_repeated_samples_do_not_freeze_lyrics() {
    let mut clock=PlaybackClock::default();
    let start=std::time::Instant::now();
    let mut media=NowPlaying{source:"KuGou.exe".into(),title:"歌曲".into(),artist:"歌手".into(),
        position:42.0,playing:true,sampled_at:Some(start),..Default::default()};
    clock.update(&mut media);
    let lyrics=crate::lyrics::parse_lrc("[00:42.00]第一句\n[00:45.00]第二句");
    for second in 1..=8 {
        let now=start+Duration::from_secs(second);
        media.sampled_at=Some(now);
        clock.update(&mut media);
        let position=media.position+now.duration_since(media.sampled_at.unwrap()).as_secs_f64();
        assert_eq!(position,42.0+second as f64,"重複取樣不能把進度拉回原地");
        if second>=3 {assert_eq!(lyrics.index_at(position as f32),Some(1));}
    }
    // 新位置、暂停、恢复、切歌和切换播放器必须建立新的采样基准。
    for (position,playing,title,source) in [(12.0,true,"歌曲","KuGou.exe"),(12.0,false,"歌曲","KuGou.exe"),
        (12.0,true,"歌曲","KuGou.exe"),(12.0,true,"新歌","KuGou.exe"),(12.0,true,"新歌","QQMusic.exe")] {
        let now=media.sampled_at.unwrap()+Duration::from_secs(1);
        media.position=position;media.playing=playing;media.title=title.into();media.source=source.into();media.sampled_at=Some(now);
        clock.update(&mut media);
        assert_eq!(media.sampled_at,Some(now));
        assert_eq!(media.position,position);
    }
}

#[test]
fn other_clients_repeated_samples_keep_the_clock_running() {
    for source in ["QQMusic.exe","SodaMusic.exe","Spotify.exe","cloudmusic.exe","msedge.exe"] {
        let mut clock=PlaybackClock::default();
        let start=std::time::Instant::now();
        let mut media=NowPlaying{source:source.into(),title:"歌曲".into(),artist:"歌手".into(),album:"專輯".into(),
            position:10.0,duration:120.0,playing:true,sampled_at:Some(start),..Default::default()};
        clock.update(&mut media);
        for second in 1..=5 {
            media.sampled_at=Some(start+Duration::from_secs(second));
            clock.update(&mut media);
            assert_eq!(media.sampled_at,Some(start),"{source} 重複位置不能重設取樣時刻");
        }
        for position in [16.0,17.0,4.0,90.0] {
            let now=start+Duration::from_secs(10);
            media.position=position;media.sampled_at=Some(now);
            clock.update(&mut media);
            assert_eq!(media.sampled_at,Some(now),"{source} 必須跟隨新位置");
        }
        for change in 0..7 {
            let now=start+Duration::from_secs(20+change);
            match change {
                0=>media.playing=false,
                1=>media.playing=true,
                2=>media.title="新歌".into(),
                3=>media.artist="另一歌手".into(),
                4=>media.album="另一專輯".into(),
                5=>media.song_id=Some(123),
                _=>media.source="另一個播放器".into(),
            }
            media.sampled_at=Some(now);clock.update(&mut media);
            assert_eq!(media.sampled_at,Some(now),"{source} 狀態變化必須重設時鐘");
        }
        // 会话消失或缺少有效时间轴后，恢复时不能沿用旧计时。
        clock.update(&mut NowPlaying::default());
        let now=start+Duration::from_secs(30);
        media.sampled_at=Some(now);clock.update(&mut media);assert_eq!(media.sampled_at,Some(now));
        media.sampled_at=None;clock.update(&mut media);assert_eq!(media.sampled_at,None);
        media.sampled_at=Some(now);clock.update(&mut media);assert_eq!(media.sampled_at,Some(now));
    }
}

#[derive(Default)]
struct MediaMemory {last:Option<(NowPlaying,std::time::Instant)>}
impl MediaMemory {
    fn update(&mut self,media:&mut NowPlaying,now:std::time::Instant){
        if media.playback_type==MediaKind::Video{self.last=None;return;}
        if media.title.trim().is_empty(){
            // 会话重建时可短暂返回空值；保留窗口不刷新期限，真正退出最多延迟两秒收起。
            if let Some((old,at))=&self.last {
                if now.duration_since(*at)<=Duration::from_secs(2) && (media.source.is_empty() || media.source==old.source){
                    *media=old.clone();return;
                }
            }
            self.last=None;return;
        }
        let complete=!media.artist.trim().is_empty() && media.duration>0.0;
        if let Some((old,at))=&self.last {
            // 一路来源暂时没有 song_id（SMTC 不提供）不算换歌；两路都有且不同才是另一首。
            let same=media.source==old.source && media.title==old.title
                && (media.song_id.is_none() || old.song_id.is_none() || media.song_id==old.song_id)
                && (media.artist.trim().is_empty() || crate::lyrics::artist_set(&media.artist)==crate::lyrics::artist_set(&old.artist));
            if same && now.duration_since(*at)<=Duration::from_secs(2) {
                if media.artist.trim().is_empty(){media.artist=old.artist.clone();}
                if media.song_id.is_none(){media.song_id=old.song_id;}
                if media.duration<=0.0{media.duration=old.duration;}
                // 同一歌曲的秒数舍入波动不代表换歌，避免重置歌词和重复请求。
                if (media.duration-old.duration).abs()<=1.0{media.duration=old.duration;}
            }else if !same {self.last=None;}
        }
        if complete{self.last=Some((media.clone(),now));}
    }
}

pub struct Poller {
    enabled:Arc<std::sync::atomic::AtomicBool>,stop:Arc<std::sync::atomic::AtomicBool>,worker:Option<std::thread::JoinHandle<()>>,
    interval_ms:Arc<std::sync::atomic::AtomicU32>,
}
/// 看着音乐页或刚点过播放控制时的采样间隔，与改版前一致。
pub const POLL_FAST:Duration=Duration::from_millis(250);
/// 岛上收起显示歌词时：换歌、暂停最多晚半秒，歌词进度由本地时钟推算，不受影响。
pub const POLL_LYRICS:Duration=Duration::from_millis(500);
/// 其他时候只需跟上歌名和播放状态。
pub const POLL_BACKGROUND:Duration=Duration::from_millis(1000);
/// 没有任何播放器在放时，按请求间隔的 4 倍再放慢，限制在 1～2 秒。
fn idle_interval(requested:u32)->u32{requested.saturating_mul(4).clamp(1000,2000)}
impl Poller {
    pub fn store(&self,enabled:bool,order:std::sync::atomic::Ordering){self.enabled.store(enabled,order);}
    /// 调整采样间隔；变快时立即唤醒采样线程补查一次，展开音乐页或点控制按钮后马上看到最新状态。
    pub fn set_interval(&self,interval:Duration){
        let ms=interval.as_millis().clamp(50,10_000) as u32;
        let previous=self.interval_ms.swap(ms,std::sync::atomic::Ordering::Relaxed);
        if ms<previous {if let Some(worker)=&self.worker{worker.thread().unpark();}}
    }
}
impl Drop for Poller {
    fn drop(&mut self){
        self.stop.store(true,std::sync::atomic::Ordering::Release);
        if let Some(worker)=&self.worker{worker.thread().unpark();}
    }
}

/// 后台线程：刷新正在播放；持有者销毁后停止，不在界面线程等待 COM 返回。
pub fn spawn_poller(state: Shared,ctx:eframe::egui::Context)->Poller {
    let enabled=Arc::new(std::sync::atomic::AtomicBool::new(true));
    let active=enabled.clone();
    let stop=Arc::new(std::sync::atomic::AtomicBool::new(false));let stopped=stop.clone();
    let interval_ms=Arc::new(std::sync::atomic::AtomicU32::new(POLL_FAST.as_millis() as u32));let requested=interval_ms.clone();
    let worker=crate::resources::POLLER.spawn(move || {
        let _apartment=init_winrt();
        let mut artwork=ArtworkCache::default();
        let mut playback_clock=PlaybackClock::default();
        let mut memory=MediaMemory::default();
        let mut selected_source=String::new();
        let mut volume=crate::music_volume::Reader::default();
        #[cfg(windows)]
        let mut netease=netease_playback::Reader::default();
        #[cfg(windows)]
        let mut bodian=bodian::Reader::default();
        #[cfg(windows)]
        let mut kuwo=kuwo::Reader::default();
        #[cfg(windows)]
        let mut cover_search=cover_search::CoverSearch::default();
        #[cfg(windows)]
        let mut gaps=music_gaps::Tracker::default();
        loop {
            if stopped.load(std::sync::atomic::Ordering::Acquire) || Arc::strong_count(&state)==1{break;}
            if !active.load(std::sync::atomic::Ordering::Relaxed){
                artwork.clear(false);playback_clock=PlaybackClock::default();memory=MediaMemory::default();selected_source.clear();
                #[cfg(windows)] {netease=netease_playback::Reader::default();bodian=bodian::Reader::default();kuwo=kuwo::Reader::default();}
                if let Ok(mut current)=state.lock(){if !current.title.is_empty(){*current=NowPlaying::default();ctx.request_repaint();}}
                std::thread::park_timeout(Duration::from_millis(250));continue;
            }
            let system=query(&mut artwork).filter(|media|source_enabled(&media.source));
            #[cfg(windows)]
            let native=netease.poll().into_iter().chain(bodian.poll()).chain(kuwo.poll()).min_by_key(media_rank)
                .filter(|media|source_enabled(&media.source));
            #[cfg(windows)]
            let native_pending=netease.pending();
            #[cfg(not(windows))]
            let native=None;
            #[cfg(not(windows))]
            let native_pending=false;
            let mut np=choose_media_for_source(system,native,&selected_source,native_pending).unwrap_or_default();
            // 瀏覽器只給影片標題（甚至是分頁名、或整個空字串）時，直接聽系統正在播的聲音認歌，
            // 順便把「唱到第幾秒」補進位置，歌詞才能從正確的段落進場。
            // 標題是不是空的都要聽：Chrome 常常整個標題都給不出來。
            #[cfg(windows)]
            if crate::audio_id::enabled() {
                if np.source.is_empty() {
                    // 播放卡和視窗標題都問不到東西（Edge 常見）：改看「音量混音器裡有瀏覽器在出聲」，
                    // 只要有人在播就開始聽，認出來之後直接補成島上的播放資訊。
                    if let Some(source)=crate::browser_media::audible_source() {
                        crate::audio_id::request();
                        if let Some(found)=crate::audio_id::cached() {
                            np.source=source.to_string();
                            np.playing=true;
                            np.playback_type=MediaKind::Music;
                            crate::audio_id::apply(&mut np,&found);
                        }
                    }
                } else if crate::smtc::is_browser_source(&np.source) && np.playing {
                    crate::audio_id::request();
                    if let Some(found)=crate::audio_id::cached(){crate::audio_id::apply(&mut np,&found);}
                }
            }
            memory.update(&mut np,std::time::Instant::now());
            np.volume=volume.poll(&np.source);
            if !np.source.is_empty(){selected_source=np.source.clone();}
            #[cfg(windows)]
            netease.fill_artwork(&mut np);
            // 播放器没给封面（缺系统缩略图、电台 / FM 等模式）时按歌名在线找一张。
            #[cfg(windows)]
            if np.artwork.is_none(){np.artwork=cover_search.fill(&np,std::time::Instant::now());}
            playback_clock.update(&mut np);
            #[cfg(windows)]
            if let Some(line)=gaps.observe(&np,std::time::Instant::now()){music_gaps::record(&line,&np.source);}
            let idle=np.title.is_empty();
            if idle {
                artwork.clear(true);
            }
            if let Ok(mut cur) = state.lock() {
                let changed=cur.title!=np.title || cur.artist!=np.artist || cur.playing!=np.playing
                    || cur.source!=np.source || cur.song_id!=np.song_id || cur.duration!=np.duration || cur.volume!=np.volume
                    || cur.artwork.as_ref().zip(np.artwork.as_ref()).map_or(cur.artwork.is_some()!=np.artwork.is_some(),|(old,new)|!Arc::ptr_eq(old,new));
                *cur = np;
                if changed{ctx.request_repaint();}
            }
            let wait=requested.load(std::sync::atomic::Ordering::Relaxed);
            std::thread::park_timeout(Duration::from_millis(if idle{idle_interval(wait)}else{wait}.into()));
        }
    });
    Poller{enabled,stop,worker,interval_ms}
}

#[cfg(test)]
mod tests {
    #[test]
    fn youtube_app_id_counts_as_browser(){
        // Chrome 對 YouTube 分頁自報的是 AppUserModelId，不是 chrome.exe。
        assert!(is_browser_source("www.youtube.com-54E21B02_pd8mbgmqs65xy!App"));
        assert!(is_browser_source("chrome.exe"));
        assert!(is_browser_source("msedge.exe"));
        // 播放器自報的 App ID 不能被當成瀏覽器，否則會走錯正規化。
        assert!(!is_browser_source("SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify"));
        assert!(!is_browser_source("cloudmusic.exe"));
    }

    use super::*;
    #[test]
    fn qqmusic_is_recognised_by_old_and_new_session_names(){
        for source in ["QQMusic.exe","qqmusic","Tencent.QQMusic"] {
            assert!(is_qqmusic(source),"{source}");
            let media=NowPlaying{title:"歌".into(),source:source.into(),playback_type:MediaKind::Music,..Default::default()};
            assert!(media.is_music(),"{source} 應按音樂處理");
        }
        for source in ["Tencent.QQ","QQMusic.exe.bak","chrome.exe"] {assert!(!is_qqmusic(source),"{source}");}
        #[cfg(windows)]
        assert_eq!(music_log::player_exe("Tencent.QQMusic"),Some("QQMusic.exe"));
    }
    #[test]
    #[cfg(windows)]
    fn resource_media_metadata_cannot_feed_unbounded_text_to_the_renderer(){
        assert!(media_text(Ok(windows::core::HSTRING::from("字".repeat(4097)))).is_none());
        assert_eq!(media_text(Ok(windows::core::HSTRING::from("正常歌名"))).as_deref(),Some("正常歌名"));
    }
    /// 控制线程全进程只允许一个，用到它的测试需要串行。
    static CONTROL_TESTS:Mutex<()>=Mutex::new(());
    /// 封面线程全进程最多两个，其中一项测试会故意把它们占满；用到封面线程的测试需要串行，否则并行时会互相抢名额而误报。
    static ARTWORK_TESTS:Mutex<()>=Mutex::new(());
    /// 上一个测试释放工作对象后，后台线程要稍后才退出并交还名额；开始前先等名额空出来。
    fn wait_lane_idle(lane:&'static crate::resources::Lane){
        let until=std::time::Instant::now()+Duration::from_secs(5);
        while lane.active()>0 && std::time::Instant::now()<until{std::thread::sleep(Duration::from_millis(5));}
        assert_eq!(lane.active(),0,"{} 名額被其他測試長期佔用",lane.name);
    }

    #[test]
    fn queued_seeks_for_the_same_player_run_only_the_latest(){
        let _serial=CONTROL_TESTS.lock().unwrap_or_else(|e|e.into_inner());wait_lane_idle(&crate::resources::CONTROL);
        let (entered,started)=std::sync::mpsc::channel();let (release,wait)=std::sync::mpsc::channel::<()>();
        let handled=Arc::new(Mutex::new(Vec::new()));let log=handled.clone();
        let worker=ControlWorker::start_with(eframe::egui::Context::default(),move|command,source|{
            if command==Transport::Toggle{let _=entered.send(());let _=wait.recv();}
            log.lock().unwrap().push((command,source.to_string()));true
        }).unwrap();
        worker.submit(Transport::Toggle,"QQMusic.exe".into()).unwrap();started.recv_timeout(Duration::from_secs(2)).unwrap();
        let mut results=Vec::new();
        for (seconds,source) in [(10.0,"QQMusic.exe"),(20.0,"QQMusic.exe"),(30.0,"QQMusic.exe"),(40.0,"Spotify.exe")] {
            results.push(worker.submit(Transport::Seek(seconds),source.into()).unwrap().unwrap());
        }
        worker.submit(Transport::Next,"QQMusic.exe".into()).unwrap();
        drop(release);
        let outcomes:Vec<bool>=results.iter().map(|rx|rx.recv_timeout(Duration::from_secs(2)).unwrap()).collect();
        assert_eq!(outcomes,[false,false,true,true],"被後續跳轉覆蓋的請求不執行");
        let until=std::time::Instant::now()+Duration::from_secs(2);
        while handled.lock().unwrap().len()<4 && std::time::Instant::now()<until{std::thread::sleep(Duration::from_millis(5));}
        assert_eq!(*handled.lock().unwrap(),vec![(Transport::Toggle,"QQMusic.exe".into()),(Transport::Seek(30.0),"QQMusic.exe".into()),(Transport::Seek(40.0),"Spotify.exe".into()),(Transport::Next,"QQMusic.exe".into())]);
        drop(worker);
    }

    #[test]
    fn music_volume_queue_coalesces_only_the_same_players_volume_changes(){
        let _serial=CONTROL_TESTS.lock().unwrap_or_else(|e|e.into_inner());wait_lane_idle(&crate::resources::CONTROL);
        let (entered,started)=std::sync::mpsc::channel();let (release,wait)=std::sync::mpsc::channel::<()>();
        let handled=Arc::new(Mutex::new(Vec::new()));let log=handled.clone();
        let worker=ControlWorker::start_with(eframe::egui::Context::default(),move|command,source|{
            if command==Transport::Toggle{let _=entered.send(());let _=wait.recv();}
            log.lock().unwrap().push((command,source.to_string()));true
        }).unwrap();
        worker.submit(Transport::Toggle,"QQMusic.exe".into()).unwrap();started.recv_timeout(Duration::from_secs(2)).unwrap();
        let mut results=Vec::new();
        for (command,source) in [(Transport::SetVolume(10),"QQMusic.exe"),(Transport::SetVolume(20),"QQMusic.exe"),(Transport::SetVolume(30),"cloudmusic.exe"),(Transport::SetMute(true),"QQMusic.exe")]{
            results.push(worker.submit(command,source.into()).unwrap().unwrap());
        }
        drop(release);
        let outcomes:Vec<_>=results.iter().map(|rx|rx.recv_timeout(Duration::from_secs(2)).unwrap()).collect();
        assert_eq!(outcomes,[false,true,true,true]);
        assert_eq!(*handled.lock().unwrap(),vec![(Transport::Toggle,"QQMusic.exe".into()),(Transport::SetVolume(20),"QQMusic.exe".into()),(Transport::SetVolume(30),"cloudmusic.exe".into()),(Transport::SetMute(true),"QQMusic.exe".into())]);
    }

    #[test]
    fn resource_control_queue_stays_bounded_when_the_player_is_stuck(){
        let _serial=CONTROL_TESTS.lock().unwrap_or_else(|e|e.into_inner());wait_lane_idle(&crate::resources::CONTROL);
        let (entered,started)=std::sync::mpsc::channel();let (release,wait)=std::sync::mpsc::channel::<()>();
        let worker=ControlWorker::start_with(eframe::egui::Context::default(),move|_,_|{let _=entered.send(());let _=wait.recv();true}).unwrap();
        worker.submit(Transport::Next,"player".into()).unwrap();started.recv_timeout(Duration::from_secs(2)).unwrap();
        let mut admitted=0;
        for _ in 0..10_000 {if worker.submit(Transport::Next,"player".into()).is_ok(){admitted+=1;}}
        drop(worker);drop(release);
        let until=std::time::Instant::now()+Duration::from_secs(2);
        while crate::resources::CONTROL.active()>0 && std::time::Instant::now()<until{std::thread::sleep(Duration::from_millis(5));}
        assert_eq!(admitted,8,"播放器卡住時只允許有限的排隊操作");assert_eq!(crate::resources::CONTROL.active(),0);
    }
    #[test]
    fn resource_poller_stops_even_when_shared_state_is_still_owned(){
        // 采样线程会建自己的封面缓存并占用封面名额。
        let _serial=ARTWORK_TESTS.lock().unwrap_or_else(|e|e.into_inner());wait_lane_idle(&crate::resources::ARTWORK);
        let state=shared();let poller=spawn_poller(state.clone(),eframe::egui::Context::default());
        poller.store(false,std::sync::atomic::Ordering::Relaxed);drop(poller);
        let until=std::time::Instant::now()+Duration::from_secs(5);
        while crate::resources::POLLER.active()>0 && std::time::Instant::now()<until{std::thread::sleep(Duration::from_millis(10));}
        assert_eq!(crate::resources::POLLER.active(),0);assert!(state.lock().is_ok());
    }
    #[test]
    #[cfg(windows)]
    fn resource_full_async_budget_prevents_starting_another_operation(){
        let permits:Vec<_>=(0..4).map(|_|crate::resources::ASYNC.acquire().unwrap()).collect();
        assert!(media_result::<bool>(||panic!("配額耗盡時不能建立 WinRT 請求")).is_none());drop(permits);
    }
    #[test]
    fn resource_reconnections_cannot_accumulate_blocked_artwork_workers(){
        let _serial=ARTWORK_TESTS.lock().unwrap_or_else(|e|e.into_inner());wait_lane_idle(&crate::resources::ARTWORK);
        use std::sync::{Condvar,atomic::{AtomicUsize,Ordering}};
        let gate=Arc::new((Mutex::new(false),Condvar::new()));
        let started=Arc::new(AtomicUsize::new(0));
        for _ in 0..30 {
            let mut cache=ArtworkCache::default();let gate=gate.clone();let started=started.clone();
            cache.poll("song".into(),move||{
                started.fetch_add(1,Ordering::SeqCst);
                let _released=gate.1.wait_while(gate.0.lock().unwrap(),|open|!*open).unwrap();None
            });
            std::thread::sleep(Duration::from_millis(5));
        }
        let count=started.load(Ordering::SeqCst);
        *gate.0.lock().unwrap()=true;gate.1.notify_all();
        assert!(count<=2,"反覆斷開和重連產生了 {count} 個阻塞封面執行緒");
    }
    #[test]
    fn resource_artwork_dimensions_are_checked_before_decoding(){
        let image=image::DynamicImage::new_rgba8(4097,1);
        let mut bytes=std::io::Cursor::new(Vec::new());image.write_to(&mut bytes,image::ImageFormat::Png).unwrap();
        assert!(decode_artwork(bytes.get_ref()).is_none(),"超大尺寸封面應在解碼前拒絕");
    }

    #[test]
    fn newer_native_event_preserves_matching_system_metadata(){
        let system=NowPlaying{source:"cloudmusic.exe".into(),title:"歌曲".into(),artist:"歌手".into(),duration:120.0,event_time_ms:Some(1000),..Default::default()};
        let native=NowPlaying{artist:String::new(),duration:0.0,event_time_ms:Some(2000),playing:true,position:12.0,..system.clone()};
        for previous in ["","cloudmusic.exe"] {
            let selected=choose_media_for_source(Some(system.clone()),Some(native.clone()),previous,false).unwrap();
            assert_eq!(selected.artist,"歌手");
            assert_eq!(selected.duration,120.0);
            assert!(selected.playing);assert_eq!(selected.position,12.0);
        }
    }

    #[test]
    fn unchanged_cover_does_not_start_a_worker_on_every_poll(){
        let _serial=ARTWORK_TESTS.lock().unwrap_or_else(|e|e.into_inner());wait_lane_idle(&crate::resources::ARTWORK);
        let mut cache=ArtworkCache::default();
        let now=std::time::Instant::now();
        let calls=Arc::new(std::sync::atomic::AtomicUsize::new(0));
        for _ in 0..30 {
            let calls=calls.clone();
            cache.poll_at("same-song".into(),move ||{calls.fetch_add(1,std::sync::atomic::Ordering::SeqCst);Some(cover_bytes(eframe::egui::Color32::RED))},now);
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(cache.image.is_some(),"首張封面仍應及時載入");
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst),1,"同一封面不能每次重新整理都建立執行緒和讀取系統流");
    }

    #[test]
    fn stable_cover_is_rechecked_rarely_but_a_new_song_starts_fast(){
        let _serial=ARTWORK_TESTS.lock().unwrap_or_else(|e|e.into_inner());wait_lane_idle(&crate::resources::ARTWORK);
        let mut cache=ArtworkCache::default();
        let now=std::time::Instant::now();
        for _ in 0..3 {cache.get("song".into(),||Some(cover_bytes(eframe::egui::Color32::RED)));}
        assert_eq!(cache.unchanged,2);
        cache.poll_at("song".into(),||None,now);
        assert_eq!(cache.next_poll,Some(now+Duration::from_millis(2000)),"剛拿到封面時仍短間隔複查，接住遲到的真實封面");
        cache.pending=false;cache.next_poll=None;cache.get("song".into(),||Some(cover_bytes(eframe::egui::Color32::RED)));
        cache.poll_at("song".into(),||None,now);
        assert_eq!(cache.next_poll,Some(now+Duration::from_secs(30)),"連續三次相同後放慢到 30 秒");
        cache.get("song".into(),||Some(cover_bytes(eframe::egui::Color32::BLUE)));assert_eq!(cache.unchanged,0,"封面變化後恢復快速複查");
        cache.get("next".into(),||None);assert_eq!(cache.unchanged,0);assert!(cache.next_poll.is_none(),"切歌立即讀取新封面");
    }

    #[test]
    fn changing_tracks_reuses_the_cover_worker_without_waiting_for_old_backoff(){
        let _serial=ARTWORK_TESTS.lock().unwrap_or_else(|e|e.into_inner());wait_lane_idle(&crate::resources::ARTWORK);
        let mut cache=ArtworkCache::default();let mut threads=std::collections::HashSet::new();
        for song in ["first","second","third"] {
            let (tx,rx)=std::sync::mpsc::channel();
            cache.poll(song.into(),move||{tx.send(std::thread::current().id()).unwrap();Some(cover_bytes(eframe::egui::Color32::BLUE))});
            threads.insert(rx.recv_timeout(Duration::from_secs(2)).expect("切歌后應立即讀取新封面"));
            let start=std::time::Instant::now();
            while cache.image.is_none(){
                assert!(start.elapsed()<Duration::from_secs(2));
                std::thread::sleep(Duration::from_millis(5));cache.poll(song.into(),||None);
            }
        }
        assert_eq!(threads.len(),1,"連續切歌應複用同一個讀取執行緒");
    }

    #[test]
    fn idle_sampling_slows_down_but_stays_within_two_seconds(){
        assert_eq!(idle_interval(POLL_FAST.as_millis() as u32),1000);
        assert_eq!(idle_interval(POLL_LYRICS.as_millis() as u32),2000);
        assert_eq!(idle_interval(POLL_BACKGROUND.as_millis() as u32),2000);
        assert_eq!(idle_interval(u32::MAX),2000);
    }

    #[test]
    #[ignore="需要本機存在媒體會話；只讀驗證關閉音樂頁後停止取樣、重新開啟後恢復"]
    fn live_media_sampling_can_be_disabled_and_resumed(){
        let state=shared();let enabled=spawn_poller(state.clone(),eframe::egui::Context::default());
        let wait=|present:bool|{
            let start=std::time::Instant::now();
            while state.lock().unwrap().title.is_empty()==present {
                assert!(start.elapsed()<Duration::from_secs(10),"媒體取樣開關沒有生效");
                std::thread::sleep(Duration::from_millis(100));
            }
        };
        wait(true);enabled.store(false,std::sync::atomic::Ordering::Relaxed);wait(false);
        std::thread::sleep(Duration::from_millis(500));assert!(state.lock().unwrap().title.is_empty());
        enabled.store(true,std::sync::atomic::Ordering::Relaxed);wait(true);
    }




    #[test]
    fn native_pause_and_loading_do_not_select_another_paused_player(){
        let native=NowPlaying{source:"cloudmusic.exe".into(),title:"網易雲歌曲".into(),artist:"歌手".into(),..Default::default()};
        for source in ["QQMusic.exe","kugou","汽水音乐"] {
            let other=NowPlaying{source:source.into(),title:"其他歌曲".into(),artist:"歌手".into(),..Default::default()};
            assert_eq!(choose_media_for_source(Some(other.clone()),Some(native.clone()),"cloudmusic.exe",false).unwrap().source,"cloudmusic.exe","暫停後必須仍能控制原播放器");
            assert!(choose_media_for_source(Some(other.clone()),None,"cloudmusic.exe",true).is_none(),"載入期間不能把按鈕指向其他播放器");
            assert_eq!(choose_media_for_source(Some(other.clone()),None,"cloudmusic.exe",false).unwrap().source,source,"退出後允許回到系統會話");
            let mut playing=other;playing.playing=true;
            assert_eq!(choose_media_for_source(Some(playing),Some(native.clone()),"cloudmusic.exe",false).unwrap().source,source,"其他客戶端開始播放時應正常切換");
        }
    }

    #[test]
    fn slow_cover_does_not_block_metadata_or_replace_a_new_song(){
        let _serial=ARTWORK_TESTS.lock().unwrap_or_else(|e|e.into_inner());wait_lane_idle(&crate::resources::ARTWORK);
        let mut cache=ArtworkCache::default();
        let (release,wait)=std::sync::mpsc::channel();
        let start=std::time::Instant::now();
        let unblock=std::thread::spawn(move ||{std::thread::sleep(Duration::from_millis(150));release.send(()).unwrap();});
        assert!(cache.poll("old".into(),move ||{wait.recv().unwrap();Some(cover_bytes(eframe::egui::Color32::RED))}).is_none());
        assert!(start.elapsed()<Duration::from_millis(100),"封面讀取不能阻塞歌曲重新整理");
        assert!(cache.poll("new".into(),||None).is_none());
        unblock.join().unwrap();
        std::thread::sleep(Duration::from_millis(30));
        assert!(cache.poll("new".into(),||None).is_none(),"舊歌曲的遲到封面不能覆蓋新歌曲");
    }

    #[test]
    fn netease_song_id_survives_a_lead_artist_only_system_session(){
        // #202：系统会话只报主艺人，网易云日志有完整署名和歌曲 ID；选系统会话时也要带上 ID，才能按 ID 取词。
        let system=NowPlaying{source:"cloudmusic.exe".into(),title:"Loop".into(),artist:"雄之助".into(),playing:true,event_time_ms:Some(2),..Default::default()};
        let native=NowPlaying{source:"cloudmusic.exe".into(),title:"Loop".into(),artist:"雄之助、WaMi".into(),playing:true,song_id:Some(1822506511),duration:185.0,event_time_ms:Some(1),..Default::default()};
        let media=choose_media(Some(system.clone()),Some(native.clone())).unwrap();
        assert_eq!((media.song_id,media.duration),(Some(1822506511),185.0));
        let other=NowPlaying{artist:"RHYM".into(),..native};
        assert_eq!(choose_media(Some(system),Some(other)).unwrap().song_id,None,"不同歌手的同名歌不能借用 ID");
    }

    #[test]
    fn native_cover_survives_a_missing_system_thumbnail(){
        let cover=Arc::new(eframe::egui::ColorImage::new([2,2],eframe::egui::Color32::BLUE));
        let native=NowPlaying{source:"cloudmusic.exe".into(),title:"歌曲".into(),artist:"歌手".into(),playing:true,artwork:Some(cover.clone()),..Default::default()};
        let mut system=native.clone();system.artwork=None;
        assert!(Arc::ptr_eq(choose_media(Some(system),Some(native)).unwrap().artwork.as_ref().expect("系統封面缺失時應保留原生封面"),&cover));
    }

    #[test]
    fn other_music_clients_keep_their_own_covers_when_netease_is_open(){
        let cover=Arc::new(eframe::egui::ColorImage::new([2,2],eframe::egui::Color32::RED));
        let native=NowPlaying{source:"cloudmusic.exe".into(),title:"網易雲歌曲".into(),artist:"歌手".into(),playing:true,
            artwork:Some(Arc::new(eframe::egui::ColorImage::new([2,2],eframe::egui::Color32::BLUE))),..Default::default()};
        for source in ["SodaMusic.exe","KuGou.exe","QQMusic.exe","Spotify.exe"]{
            let system=NowPlaying{source:source.into(),title:"當前歌曲".into(),artist:"歌手".into(),playing:true,artwork:Some(cover.clone()),..Default::default()};
            let selected=choose_media(Some(system.clone()),Some(native.clone())).unwrap();
            assert_eq!(selected.source,source);assert!(Arc::ptr_eq(selected.artwork.as_ref().unwrap(),&cover));
            let mut paused=system;paused.playing=false;let mut native_paused=native.clone();native_paused.playing=false;
            assert!(Arc::ptr_eq(choose_media(Some(paused),Some(native_paused)).unwrap().artwork.as_ref().unwrap(),&cover));
            assert_eq!(choose_media(None,Some(native.clone())).unwrap().source,"cloudmusic.exe");
        }
    }

    #[test]
    fn native_music_recovers_after_the_previous_player_closes(){
        let netease=NowPlaying{source:"cloudmusic.exe".into(),title:"歌曲".into(),artist:"歌手".into(),playing:true,position:42.0,duration:240.0,..Default::default()};
        assert_eq!(choose_media(None,Some(netease.clone())).unwrap().source,"cloudmusic.exe","網易雲未註冊 SMTC 時仍應接回正在播放的歌曲");
        let old=NowPlaying{source:"SodaMusic.exe".into(),title:"上一首".into(),artist:"歌手".into(),..Default::default()};
        assert_eq!(choose_media(Some(old.clone()),Some(netease.clone())).unwrap().source,"cloudmusic.exe","舊客戶端的暫停會話不能擋住正在播放的網易雲");
        let mut current=old.clone();current.playing=true;
        assert_eq!(choose_media(Some(current),Some(netease)).unwrap().source,"SodaMusic.exe","正常播放的系統當前會話應保持優先");
        assert!(choose_media(None,None).is_none(),"播放器全部退出後不能殘留歌曲");
    }

    #[test]
    fn bodian_native_metadata_is_not_merged_into_a_netease_session(){
        let system=NowPlaying{source:"cloudmusic.exe".into(),title:"同名歌曲".into(),artist:"歌手".into(),playing:true,..Default::default()};
        let native=NowPlaying{source:"bodian_pc.exe".into(),song_id:Some(123),duration:240.0,..system.clone()};
        let chosen=choose_media(Some(system.clone()),Some(native.clone())).unwrap();
        assert_eq!(chosen.source,"cloudmusic.exe");assert_eq!(chosen.song_id,None);assert_eq!(chosen.duration,0.0);
        let mut paused=system;paused.playing=false;
        assert_eq!(choose_media(Some(paused),Some(native)).unwrap().source,"bodian_pc.exe");
    }

    fn cover_bytes(color:eframe::egui::Color32)->Vec<u8> {
        let image=image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(2,2,image::Rgba(color.to_array())));
        let mut bytes=std::io::Cursor::new(Vec::new());
        image.write_to(&mut bytes,image::ImageFormat::Png).unwrap();
        bytes.into_inner()
    }

    #[test]
    fn kkbox_desktop_and_store_sources_are_music(){
        for source in ["KKBOX.exe","kkbox","com.skysoft.kkbox.android"] {
            let media=NowPlaying{source:source.into(),title:"歌曲".into(),playback_type:MediaKind::Music,..Default::default()};
            assert_eq!(media.kind(),MediaKind::Music,"{source} 應被辨識成 KKBOX 音樂");
        }
    }

    #[test]
    fn known_music_clients_accept_missing_artist_but_not_video(){
        for source in ["kugou","QQMusic.exe","cloudmusic.exe","汽水音乐","SodaMusic.exe","Spotify.exe","KKBOX.exe","com.skysoft.kkbox.android","kwmusic.exe","bodian_pc.exe","AppleMusic.exe","AppleInc.AppleMusicWin_nzyj5cx40ttqa!App"] {
            let mut media=NowPlaying{source:source.into(),title:"歌曲".into(),playback_type:MediaKind::Music,..Default::default()};
            assert!(media.is_music(),"{source} 缺少歌手時仍應顯示歌曲");
            media.playback_type=MediaKind::Video;assert!(!media.is_music());
        }
    }

    #[cfg(windows)]
    #[test]
    fn transient_system_failure_is_not_the_same_as_player_exit(){
        let mut cache=ArtworkCache::default();let start=std::time::Instant::now();
        let media=NowPlaying{title:"歌".into(),source:"QQMusic.exe".into(),playback_type:MediaKind::Music,..Default::default()};
        cache.system_result(Ok(Some(media)),start);
        assert_eq!(cache.system_result(Err(()),start+Duration::from_secs(1)).unwrap().title,"歌");
        assert!(cache.system_result(Err(()),start+Duration::from_secs(3)).is_none());
        cache.system_result(Ok(None),start+Duration::from_secs(3));
        assert!(cache.system_result(Err(()),start+Duration::from_secs(4)).is_none());
    }

    #[test]
    fn active_music_wins_over_video_and_stale_paused_sessions(){
        let music=NowPlaying{source:"QQMusic.exe".into(),title:"歌".into(),playback_type:MediaKind::Music,playing:true,..Default::default()};
        let video=NowPlaying{source:"Chrome".into(),title:"影片".into(),playback_type:MediaKind::Video,playing:true,..Default::default()};
        let paused=NowPlaying{playing:false,..music.clone()};
        for sessions in [vec![video.clone(),paused.clone(),music.clone()],vec![paused.clone(),music.clone(),video.clone()]] {
            let selected=sessions.iter().min_by_key(|m|media_rank(m)).unwrap();assert!(selected.is_music() && selected.playing);
        }
        assert!(choose_media(Some(video),Some(paused)).unwrap().is_music());
    }

    #[test]
    fn qq_session_dropouts_and_duration_rounding_keep_one_track(){
        let start=std::time::Instant::now();let mut memory=MediaMemory::default();
        let song=NowPlaying{source:"QQMusic.exe".into(),title:"灰色的夜".into(),artist:"歌手".into(),duration:240.4,playing:true,playback_type:MediaKind::Music,..Default::default()};
        for step in 0..1440 {
            let mut sample=match step%3 {
                0=>song.clone(),
                1=>NowPlaying::default(),
                _=>NowPlaying{duration:240.6,..song.clone()},
            };
            memory.update(&mut sample,start+Duration::from_millis(step*250));
            assert_eq!(sample.title,song.title,"短暫丟失會話不能讓歌詞條反覆收起");
            assert_eq!(sample.duration,song.duration,"時長舍入抖動不能重新取詞");
        }
        let mut paused=NowPlaying{playing:false,..song.clone()};
        memory.update(&mut paused,start+Duration::from_secs(360));assert!(!paused.playing);
        let mut gone=NowPlaying::default();
        memory.update(&mut gone,start+Duration::from_secs(363));assert!(gone.title.is_empty());
        let mut next=NowPlaying{title:"下一首".into(),..song};
        memory.update(&mut next,start+Duration::from_secs(364));assert_eq!(next.title,"下一首");
    }

    #[test]
    fn metadata_gap_does_not_clear_lyrics_or_hide_loop_reset(){
        let start=std::time::Instant::now();let mut memory=MediaMemory::default();
        let full=NowPlaying{source:"QQMusic.exe".into(),title:"歌".into(),artist:"歌手".into(),duration:120.0,position:119.0,playing:true,playback_type:MediaKind::Music,..Default::default()};
        memory.update(&mut full.clone(),start);
        let mut gap=NowPlaying{artist:String::new(),duration:0.0,position:0.0,..full.clone()};
        memory.update(&mut gap,start+Duration::from_secs(1));assert_eq!(gap.artist,"歌手");assert_eq!(gap.duration,120.0);assert_eq!(gap.position,0.0);
        let mut expired=NowPlaying{artist:String::new(),duration:0.0,..full.clone()};
        memory.update(&mut expired,start+Duration::from_secs(3));assert!(expired.artist.is_empty());
        let mut other=NowPlaying{title:"另一首".into(),artist:String::new(),duration:0.0,..full};
        memory.update(&mut other,start+Duration::from_millis(1500));assert!(other.artist.is_empty());
        memory.update(&mut NowPlaying::default(),start+Duration::from_secs(4));assert!(memory.last.is_none());
    }

    #[test]
    fn netease_keeps_song_id_when_smtc_spells_artists_differently(){
        let system=NowPlaying{source:"cloudmusic.exe".into(),title:"歌曲".into(),artist:"甲 / 乙".into(),playing:true,duration:200.0,event_time_ms:Some(3000),playback_type:MediaKind::Music,..Default::default()};
        let native=NowPlaying{artist:"乙/甲".into(),song_id:Some(42),event_time_ms:Some(1000),..system.clone()};
        assert_eq!(choose_media(Some(system.clone()),Some(native)).unwrap().song_id,Some(42),"SMTC 較新時也要沿用網易雲日誌裡的歌曲 ID");
        let start=std::time::Instant::now();let mut memory=MediaMemory::default();
        memory.update(&mut NowPlaying{song_id:Some(42),..system.clone()},start);
        let mut later=system.clone();memory.update(&mut later,start+Duration::from_millis(500));
        assert_eq!(later.song_id,Some(42),"短暫缺少 song_id 不能被當作換歌");
        let mut other=NowPlaying{song_id:Some(43),..system};memory.update(&mut other,start+Duration::from_secs(1));
        assert_eq!(other.song_id,Some(43));
    }

    #[test]
    fn netease_reconciliation_uses_event_time_not_poll_time(){
        let system=NowPlaying{source:"cloudmusic.exe".into(),title:"新歌".into(),artist:"歌手".into(),playing:true,event_time_ms:Some(2000),..Default::default()};
        let mut native=NowPlaying{title:"舊歌".into(),event_time_ms:Some(1000),..system.clone()};
        assert_eq!(choose_media(Some(system.clone()),Some(native.clone())).unwrap().title,"新歌");
        native.event_time_ms=Some(3000);native.title="最新歌曲".into();
        assert_eq!(choose_media(Some(system),Some(native)).unwrap().title,"最新歌曲");
    }

    #[test]
    fn stale_netease_metadata_does_not_replace_new_system_track(){
        let old=NowPlaying{source:"cloudmusic.exe".into(),title:"舊歌".into(),artist:"歌手".into(),..Default::default()};
        let new=NowPlaying{title:"新歌".into(),playing:true,..old.clone()};
        let result=choose_media(Some(new),Some(old)).unwrap();assert_eq!(result.title,"新歌");assert!(result.playing);
    }

    #[test]
    fn kugou_music_without_artist_is_still_a_song(){
        let mut np=NowPlaying{source:"kugou".into(),title:"想你就寫信 (你說想哭就彈琴)".into(),playing:true,playback_type:MediaKind::Music,..Default::default()};
        assert!(np.is_music(),"酷狗漏報歌手不能清空正在播放的音樂");
        np.playback_type=MediaKind::Video;assert!(!np.is_music());
        np.playback_type=MediaKind::Music;np.title.clear();assert!(!np.is_music());
    }

    #[test]
    fn media_kind_checks_video_type_before_song_metadata(){
        let mut np=NowPlaying{title:"Video title".into(),artist:"Video author".into(),playback_type:MediaKind::Video,..Default::default()};
        assert_eq!(np.kind(),MediaKind::Video);
        np.playback_type=MediaKind::Music;np.title="小唐的Token的抖音 - 抖音".into();
        assert_eq!(np.kind(),MediaKind::Video,"瀏覽器錯誤上報音樂型別時仍應識別明確的影片標題");
        np.title="Movie - YouTube".into();assert_eq!(np.kind(),MediaKind::Video);
        np.title="阿拉斯加海灣".into();np.artist="藍心羽".into();assert!(np.is_music());
        np.artist.clear();assert_eq!(np.kind(),MediaKind::Unknown,"只有頁面標題不能確認是歌曲");
    }

    #[test]
    fn browser_video_authors_are_not_enough_to_identify_a_song(){
        let mut np=NowPlaying{title:"A video".into(),artist:"Video creator".into(),source:"Chrome".into(),playback_type:MediaKind::Music,..Default::default()};
        assert_eq!(np.kind(),MediaKind::Unknown);
        np.album="YouTube".into();assert_eq!(np.kind(),MediaKind::Music,"YouTube 分頁要上島，不能當成一般影片濾掉");
        np.title="阿拉斯加海灣".into();np.artist="藍心羽".into();np.album="阿拉斯加海灣".into();
        assert_eq!(np.kind(),MediaKind::Music,"網易雲網頁版的完整歌曲後設資料仍應識別為音樂");
        np.source="Spotify.exe".into();np.album.clear();assert_eq!(np.kind(),MediaKind::Music,"音樂客戶端不強制要求專輯資訊");
    }

    #[test]
    fn youtube_in_any_browser_is_music_even_without_album_or_when_marked_video(){
        for source in ["chrome.exe","Google.Chrome.SxS","msedge.exe","firefox.exe","iexplore.exe","brave.exe","opera.exe"] {
            let mut np=NowPlaying{title:"Movie - YouTube".into(),source:source.into(),playback_type:MediaKind::Music,..Default::default()};
            assert_eq!(np.kind(),MediaKind::Music,"{source} 的 YouTube 分頁應上島");
            np.playback_type=MediaKind::Video;
            assert_eq!(np.kind(),MediaKind::Music,"{source} 把 YouTube 標成影片時仍要顯示");
            np.title="Song - YouTube Music".into();np.album.clear();
            assert_eq!(np.kind(),MediaKind::Music,"YouTube Music 缺少專輯時仍要顯示");
            np.title="A video".into();np.album="bilibili".into();
            assert_eq!(np.kind(),MediaKind::Video,"非 YouTube 的瀏覽器影片維持影片分類");
        }
    }

    #[test]
    fn artwork_cache_retries_late_images_and_clears_on_song_change(){
        let mut cache=ArtworkCache::default();
        assert!(cache.get("first".into(),||None).is_none());
        let first=cache.get("first".into(),||Some(cover_bytes(eframe::egui::Color32::RED))).unwrap();
        let cached=cache.get("first".into(),||Some(cover_bytes(eframe::egui::Color32::RED))).unwrap();
        assert!(Arc::ptr_eq(&first,&cached));
        let missing=cache.get("first".into(),||None).unwrap();
        assert!(Arc::ptr_eq(&first,&missing),"短暫缺失時應保留當前歌曲封面");
        let invalid=cache.get("first".into(),||Some(b"invalid image".to_vec())).unwrap();
        assert!(Arc::ptr_eq(&first,&invalid),"損壞的縮圖不應清空當前有效封面");
        assert!(cache.get("second".into(),||None).is_none(),"切歌后不能保留上一首封面");
    }

    #[test]
    fn artwork_cache_replaces_a_temporary_cover_on_the_same_song(){
        let mut cache=ArtworkCache::default();
        let temporary=cache.get("song".into(),||Some(cover_bytes(eframe::egui::Color32::RED))).unwrap();
        // 网页可能先提供占位图，随后才发布真实封面。
        let actual=cache.get("song".into(),||Some(cover_bytes(eframe::egui::Color32::BLUE))).unwrap();
        assert!(!Arc::ptr_eq(&temporary,&actual),"同一首歌的真實封面到達後必須替換佔點陣圖");
        assert_eq!(actual.pixels[0],eframe::egui::Color32::BLUE);
    }

    #[test]
    fn artwork_decodes_png_jpeg_webp_and_rejects_invalid_bytes(){
        let original=image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(600,300,image::Rgb([24,120,210])));
        for format in [image::ImageFormat::Png,image::ImageFormat::Jpeg,image::ImageFormat::WebP] {
            let mut bytes=std::io::Cursor::new(Vec::new());
            original.write_to(&mut bytes,format).unwrap();
            let decoded=decode_artwork(bytes.get_ref()).unwrap();
            assert_eq!(decoded.size,[256,128]);
            assert!(decoded.pixels.iter().all(|p|p.a()==255));
        }
        assert!(decode_artwork(b"invalid image").is_none());
    }

    #[cfg(windows)]
    #[test]
    #[ignore="會連續切換當前播放器的四首歌曲，僅用於本機驗收"]
    fn live_rapid_track_changes(){
        let state=shared();let _poller=spawn_poller(state.clone(),eframe::egui::Context::default());
        std::thread::sleep(Duration::from_secs(2));
        let first=state.lock().unwrap().clone();
        assert!(first.is_music() && first.playing,"請先播放音樂");
        let expected=std::env::var("KUIDAO_EXPECT_SOURCE").expect("請指定實測客戶端").to_lowercase();
        assert!(first.source.to_lowercase().contains(&expected),"正在播放的客戶端不符：{}",first.source);
        for _ in 0..4 {
            control(Transport::Next,&first.source);
            std::thread::sleep(Duration::from_millis(400));
        }
        let start=std::time::Instant::now();
        let mut requested=String::new();
        let mut result=None;
        let mut fetched=None;
        let mut observed=String::new();
        let mut stable_since=std::time::Instant::now();
        loop {
            let media=state.lock().unwrap().clone();
            let key=format!("{}|{}|{}|{:.0}|{:?}",media.source,media.title,media.artist,media.duration,media.song_id);
            if observed!=key {observed=key.clone();stable_since=std::time::Instant::now();}
            if media.source==first.source && media.title!=first.title && requested!=key && stable_since.elapsed()>Duration::from_secs(2) {
                requested=key;
                println!("取詞：{} / {} 時長={:.0}",media.title,media.artist,media.duration);
                fetched=None;
                let (tx,rx)=std::sync::mpsc::channel();result=Some(rx);
                let song=media.clone();
                std::thread::spawn(move ||{let _=tx.send(crate::lyrics::fetch_for_source(&song.source,&song.title,&song.artist,song.duration,song.song_id));});
            }
            let native=crate::lyrics::read_native(&media.source,&media.title,&media.artist,media.duration);
            if let Some(outcome)=result.as_ref().and_then(|rx|rx.try_recv().ok()) {
                match outcome {Ok(lyrics)=>fetched=Some(lyrics),Err(error)=>println!("取詞結果：{error:?}")}
            }
            let lyrics=native.or_else(||fetched.clone());
            if media.source==first.source && media.title!=first.title && stable_since.elapsed()>Duration::from_secs(2) && media.artwork.is_some() && lyrics.as_ref().is_some_and(|l|l.synced) {
                println!("連續切歌后 {:.2}s：客戶端={} 歌曲={} 封面已就緒 歌詞={}行",start.elapsed().as_secs_f64(),media.source,media.title,lyrics.unwrap().lines.len());
                break;
            }
            assert!(start.elapsed()<Duration::from_secs(20),"切歌后未恢復：{} cover={}",media.title,media.artwork.is_some());
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    #[cfg(windows)]
    #[test]
    #[ignore="按指定來源傳送一次播放控制，僅用於本機驗收"]
    fn live_client_transport(){
        let source=std::env::var("KUIDAO_CONTROL_SOURCE").unwrap();
        let cmd=match std::env::var("KUIDAO_CONTROL").unwrap().as_str(){"next"=>Transport::Next,"prev"=>Transport::Prev,"toggle"=>Transport::Toggle,_=>panic!("未知控制")};
        control(cmd,&source);
    }

    #[cfg(windows)]
    #[test]
    #[ignore="真實客戶端進度跳轉驗收；結束後恢復位置，需要 KUIDAO_CONTROL_SOURCE"]
    fn live_client_seek(){
        use windows::Win32::{Foundation::POINT,UI::WindowsAndMessaging::{GetCursorPos,GetForegroundWindow}};
        let _apartment=init_winrt();
        let source=std::env::var("KUIDAO_CONTROL_SOURCE").unwrap();
        let manager=media_result(windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager::RequestAsync).unwrap();
        let snapshot=||{
            let mut rows=Vec::new();
            for session in manager.GetSessions().unwrap(){
                if let Some(media)=read_session(&session,&mut ArtworkCache::default()) {rows.push(media);}
            }
            if let Some(media)=netease_playback::Reader::default().poll(){rows.retain(|m|!m.source.eq_ignore_ascii_case("cloudmusic.exe"));rows.push(media);}
            rows
        };
        let before=snapshot();
        let first=before.iter().find(|m|m.source.eq_ignore_ascii_case(&source)).expect("請先開啟指定播放器並載入歌曲");
        println!("before {} position={:.2} playing={}",first.source,first.position,first.playing);
        let start=std::time::Instant::now();
        let foreground=unsafe{GetForegroundWindow()};let mut cursor=POINT::default();unsafe{GetCursorPos(&mut cursor).unwrap();}
        let mut success=true;
        for target in [first.duration*0.23,first.duration*0.67] {
            let ok=seek(target,&source);
            let after=snapshot();let current=after.iter().find(|m|m.source.eq_ignore_ascii_case(&source)).unwrap();
            println!("{source} target={target:.2} actual={:.2} playing={} confirmed={ok}",current.position,current.playing);
            success &= ok && current.title==first.title && current.playing==first.playing && (current.position-target).abs()<4.0;
            for other in before.iter().filter(|m|!m.source.eq_ignore_ascii_case(&source)) {
                let next=after.iter().find(|m|m.source==other.source).unwrap();
                let expected=other.position+if other.playing{start.elapsed().as_secs_f64()}else{0.0};
                success &= next.title==other.title && next.playing==other.playing && (next.position-expected).abs()<4.0;
            }
        }
        let restore=(first.position+if first.playing{start.elapsed().as_secs_f64()}else{0.0}).min(first.duration-1.0);
        let restored=seek(restore,&source);
        let mut after_cursor=POINT::default();unsafe{GetCursorPos(&mut after_cursor).unwrap();}
        println!("restore={restore:.2} confirmed={restored}; foreground_unchanged={} cursor_unchanged={}",unsafe{GetForegroundWindow()}==foreground,cursor==after_cursor);
        assert!(success && restored,"實際進度、播放狀態或其他客戶端狀態不符合預期");
        assert_eq!(unsafe{GetForegroundWindow()},foreground,"跳轉不能搶佔前臺");
        assert_eq!(cursor,after_cursor,"跳轉不能移動系統滑鼠");
    }

    #[cfg(windows)]
    #[test]
    #[ignore="需要網易雲及其他播放器同時執行，驗證控制不串臺"]
    fn live_netease_controls_are_isolated(){
        let _apartment=init_winrt();
        let snapshot=||{
            let mgr=media_result(windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager::RequestAsync).unwrap();
            let mut rows=Vec::new();
            for session in mgr.GetSessions().unwrap() {
                let source=session.SourceAppUserModelId().unwrap().to_string();
                if source.eq_ignore_ascii_case("cloudmusic.exe"){continue;}
                let props=media_result(||session.TryGetMediaPropertiesAsync()).unwrap();
                rows.push((source,props.Title().unwrap().to_string(),session.GetPlaybackInfo().unwrap().PlaybackStatus().unwrap().0));
            }
            rows.sort();rows
        };
        let mut reader=netease_playback::Reader::default();
        for command in [Transport::Next,Transport::Prev,Transport::Toggle,Transport::Toggle] {
            std::thread::sleep(Duration::from_millis(600));
            let before=reader.poll().expect("請先開啟網易雲並載入歌曲");
            let others=snapshot();
            let foreground=unsafe {windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow()};
            control(command,"cloudmusic.exe");
            let start=std::time::Instant::now();
            loop {
                std::thread::sleep(Duration::from_millis(100));
                assert_eq!(snapshot(),others,"網易雲控制不能改變其他客戶端的歌曲或播放狀態");
                assert!(start.elapsed()<Duration::from_secs(8),"網易雲沒有響應 {command:?}");
                let Some(after)=reader.poll() else{continue;};
                let changed=match command {Transport::Toggle=>after.playing!=before.playing,_=>after.song_id!=before.song_id};
                if changed {println!("{command:?}：{} -> {}，playing={}，其他客戶端未變化",before.title,after.title,after.playing);break;}
            }
            assert_eq!(unsafe {windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow()},foreground,"後台控制不能搶佔焦點");
        }
    }

    #[cfg(windows)]
    #[test]
    #[ignore="暫停指定客戶端之外的系統媒體會話，隔離本機播放驗收"]
    fn live_pause_other_clients(){
        let _apartment=init_winrt();
        let keep=std::env::var("KUIDAO_CONTROL_SOURCE").unwrap();
        let manager=media_result(windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager::RequestAsync).unwrap();
        for session in manager.GetSessions().unwrap() {
            let source=session.SourceAppUserModelId().unwrap().to_string();
            if source!=keep {
                println!("暫停 {source}: {:?}",media_result(||session.TryPauseAsync()));
            }
        }
    }

    #[cfg(windows)]
    #[test]
    #[ignore="需要讀取當前系統媒體會話"]
    fn live_sessions_metadata(){
        let _apartment=init_winrt();
        let manager=windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager::RequestAsync().unwrap().get().unwrap();
        for session in manager.GetSessions().unwrap() {
            let props=session.TryGetMediaPropertiesAsync().unwrap().get().unwrap();
            println!("source={} title={} artist={} type={:?} state={:?} duration={:?}",session.SourceAppUserModelId().unwrap(),props.Title().unwrap(),props.Artist().unwrap(),props.PlaybackType().ok().and_then(|kind|kind.Value().ok()),session.GetPlaybackInfo().unwrap().PlaybackStatus(),session.GetTimelineProperties().unwrap().EndTime());
        }
    }

    #[cfg(windows)]
    #[test]
    #[ignore="只讀驗證當前酷狗歌曲的補全資訊和進度"]
    fn live_kugou_complete_timeline(){
        let _apartment=init_winrt();let mut cache=ArtworkCache::default();
        let start=std::time::Instant::now();
        let first=query(&mut cache).expect("需要正在播放的酷狗會話");
        println!("歌曲={} 歌手={} 曲長={} 進度={} 播放={} 讀取耗時={:?}",first.title,first.artist,first.duration,first.position,first.playing,start.elapsed());
        assert!(first.source.eq_ignore_ascii_case("kugou") && first.is_music());
        assert!(first.duration>0.0 && first.position<=first.duration);
        if let Ok(expected)=std::env::var("KUIDAO_EXPECT_DURATION"){assert!((first.duration-expected.parse::<f64>().unwrap()).abs()<1.0);}
        std::thread::sleep(Duration::from_secs(3));
        let next=query(&mut cache).unwrap();assert_eq!(next.title,first.title);
        assert_eq!(next.duration,first.duration);
        if first.playing && next.playing {assert!(next.position>first.position+1.0,"播放進度必須遞增");}
        else if !first.playing && !next.playing {assert!((next.position-first.position).abs()<0.2,"暫停不能繼續計時");}
        println!("三秒後：進度={} 播放={}",next.position,next.playing);
    }

    #[cfg(windows)]
    #[test]
    #[ignore="驗證酷狗實際前進、後退及跳轉後歌詞，需保持酷狗播放"]
    fn live_kugou_seek_changes_actual_position(){
        let _apartment=init_winrt();let mut cache=ArtworkCache::default();
        let first=query(&mut cache).unwrap();assert_eq!(first.source,"kugou");
        let foreground=unsafe{windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow()};
        for target in [15.0,42.0,6.0] {
            control(Transport::Seek(target),&first.source);
            let start=std::time::Instant::now();
            loop {
                std::thread::sleep(Duration::from_millis(100));
                let media=query(&mut cache).unwrap();
                assert_eq!(media.title,first.title);
                if (media.position-target).abs()<2.5 {
                    let lyrics=crate::lyrics::read_native(&media.source,&media.title,&media.artist,media.duration).unwrap();
                    assert!(lyrics.synced);println!("跳轉 {target:.0}s，實際 {:.2}s，歌詞 {} 行",media.position,lyrics.lines.len());break;
                }
                assert!(start.elapsed()<Duration::from_secs(4),"酷狗未執行跳轉：目標={target}，實際={}",media.position);
            }
            assert_eq!(unsafe{windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow()},foreground,"跳轉不應搶佔前臺");
        }
    }

    #[cfg(windows)]
    #[test]
    #[ignore="將當前酷狗歌曲跳至尾聲，驗證真實單曲迴圈後仍有原生歌詞"]
    fn live_kugou_repeat_retains_lyrics(){
        let _apartment=init_winrt();let mut cache=ArtworkCache::default();
        let first=query(&mut cache).unwrap();assert_eq!(first.source,"kugou");assert!(first.playing && first.duration>5.0);
        assert!(crate::lyrics::read_native(&first.source,&first.title,&first.artist,first.duration).is_some());
        control(Transport::Seek(first.duration-3.0),&first.source);
        std::thread::sleep(Duration::from_millis(500));
        let mut previous=query(&mut cache).unwrap().position;
        println!("尾聲：{} / {}，進度={previous}，曲長={}",first.title,first.artist,first.duration);
        if previous<=first.duration-5.0{println!("酷狗未接受跳轉，等待歌曲自然迴圈");}
        let start=std::time::Instant::now();let mut wrapped=None;
        loop {
            std::thread::sleep(Duration::from_millis(250));
            let media=query(&mut cache).unwrap();
            assert_eq!((&media.title,&media.artist),(&first.title,&first.artist),"迴圈前後歌曲身份必須一致");
            assert_eq!(media.duration,first.duration);
            let lyrics=crate::lyrics::read_native(&media.source,&media.title,&media.artist,media.duration).expect("迴圈前後原生歌詞不能消失");
            if media.position+1.0<previous {wrapped=Some(std::time::Instant::now());println!("已迴圈到開頭：進度={}",media.position);}
            if wrapped.is_some_and(|at|at.elapsed()>Duration::from_secs(8)) {
                println!("第二遍：進度={}，歌詞={}行，當前={:?}",media.position,lyrics.lines.len(),lyrics.index_at(media.position as f32).map(|i|&lyrics.lines[i].text));break;
            }
            previous=media.position;assert!(start.elapsed().as_secs_f64()<first.duration+30.0,"未觀察到單曲迴圈");
        }
    }

    #[cfg(windows)]
    #[test]
    #[ignore="需要指定 KUIDAO_EXPECT_SOURCE，並在本機播放該客戶端的音樂"]
    fn live_music_client_cover_lyrics_and_progress(){
        let expected=std::env::var("KUIDAO_EXPECT_SOURCE").expect("請指定實測客戶端").to_lowercase();
        let state=shared();let _poller=spawn_poller(state.clone(),eframe::egui::Context::default());
        let start=std::time::Instant::now();
        let first=loop{
            let media=state.lock().unwrap().clone();
            if media.source.to_lowercase().contains(&expected) && media.playing && media.artwork.is_some(){break media;}
            assert!(start.elapsed()<Duration::from_secs(15),"客戶端未就緒：source={} title={} playing={} cover={}",media.source,media.title,media.playing,media.artwork.is_some());
            std::thread::sleep(Duration::from_millis(100));
        };
        let position=|media:&NowPlaying|media.position+if media.playing{media.sampled_at.map_or(0.0,|at|at.elapsed().as_secs_f64())}else{0.0};
        let before=position(&first);
        let lyrics=crate::lyrics::fetch_for_source(&first.source,&first.title,&first.artist,first.duration,first.song_id).expect("未取得歌詞");
        assert!(lyrics.synced && !lyrics.lines.is_empty(),"未取得同步歌詞");
        std::thread::sleep(Duration::from_secs(4));
        let next=state.lock().unwrap().clone();let after=position(&next);
        assert_eq!((&next.source,&next.title),(&first.source,&first.title),"取樣期間切歌，請重新實測");
        assert!(next.playing && after>before+2.0,"播放進度沒有推進：{before:.2} -> {after:.2}");
        let cover=next.artwork.expect("播放中封面消失");
        println!("客戶端={} 歌曲={} 歌手={} 封面={}x{} 歌詞={}行 進度={before:.2}->{after:.2} 歌詞行={:?}->{:?}",
            first.source,first.title,first.artist,cover.size[0],cover.size[1],lyrics.lines.len(),lyrics.index_at(before as f32),lyrics.index_at(after as f32));
    }

    #[cfg(windows)]
    #[test]
    #[ignore="驗證指定實際客戶端的切歌、暫停和恢復，不依賴線上歌詞服務"]
    fn live_playback_switch_pause_progress(){
        let expected=std::env::var("KUIDAO_EXPECT_SOURCE").unwrap().to_lowercase();
        let state=shared();let _poller=spawn_poller(state.clone(),eframe::egui::Context::default());
        let wait=|predicate:&dyn Fn(&NowPlaying)->bool|{
            let start=std::time::Instant::now();loop {
                let media=state.lock().unwrap().clone();
                if media.source.to_lowercase().contains(&expected) && predicate(&media){break media;}
                assert!(start.elapsed()<Duration::from_secs(15),"客戶端狀態沒有就緒：{} / {} playing={}",media.source,media.title,media.playing);
                std::thread::sleep(Duration::from_millis(100));
            }
        };
        let first=wait(&|m|m.is_music() && m.playing && m.artwork.is_some());
        control(Transport::Next,&first.source);
        let next=wait(&|m|m.title!=first.title && m.playing && m.artwork.is_some());
        control(Transport::Toggle,&next.source);let paused=wait(&|m|!m.playing);
        std::thread::sleep(Duration::from_millis(700));assert!(!state.lock().unwrap().playing);
        control(Transport::Toggle,&paused.source);let playing=wait(&|m|m.playing);
        let position=|m:&NowPlaying|m.position+m.sampled_at.map_or(0.0,|at|at.elapsed().as_secs_f64());
        let before=position(&playing);std::thread::sleep(Duration::from_secs(3));let after=state.lock().unwrap().clone();
        assert_eq!(after.title,playing.title);assert!(position(&after)>before+1.0);
        println!("客戶端={} 切歌={} -> {} 暫停/恢復透過，進度={} -> {}，曲長={}",first.source,first.title,next.title,playing.position,after.position,after.duration);
    }

    #[cfg(windows)]
    #[test]
    #[ignore="需要當前系統有提供封面的媒體會話"]
    fn live_current_session_artwork(){
        let _apartment=init_winrt();
        let mut cache=ArtworkCache::default();
        let start=std::time::Instant::now();
        let np=loop {
            let np=query(&mut cache).expect("當前沒有媒體會話");
            if np.artwork.is_some(){break np;}
            assert!(start.elapsed()<Duration::from_secs(10),"當前媒體會話沒有可讀封面");
            std::thread::sleep(Duration::from_millis(100));
        };
        println!("當前媒體：{} / {}；來源 {}；型別 {:?}",np.title,np.artist,np.source,np.kind());
        let cover=np.artwork.expect("當前媒體會話沒有可讀封面");
        println!("系統封面尺寸：{}×{}",cover.size[0],cover.size[1]);
        if let Ok(path)=std::env::var("KUIDAO_ARTWORK_PROBE") {
            let pixels:Vec<u8>=cover.pixels.iter().flat_map(|p|p.to_srgba_unmultiplied()).collect();
            image::save_buffer(path,&pixels,cover.size[0] as u32,cover.size[1] as u32,image::ColorType::Rgba8).unwrap();
        }
    }
}

#[cfg(all(test,windows))]
mod read_cost {
    use super::*;
    #[test]
    #[ignore="本機實測：每一類讀取在真實環境下每輪花多少時間（只讀，不改播放狀態）"]
    fn live_read_cost_per_poll(){
        let _apartment=init_winrt();
        let mut artwork=ArtworkCache::default();
        let mut netease=netease_playback::Reader::default();let mut bodian=bodian::Reader::default();let mut kuwo=kuwo::Reader::default();
        let mut cost=[std::time::Duration::ZERO;4];let rounds=40;
        let mut last=None;
        for _ in 0..rounds {
            let t=std::time::Instant::now();last=query(&mut artwork);cost[0]+=t.elapsed();
            let t=std::time::Instant::now();let _=netease.poll();cost[1]+=t.elapsed();
            let t=std::time::Instant::now();let _=bodian.poll();cost[2]+=t.elapsed();
            let t=std::time::Instant::now();let _=kuwo.poll();cost[3]+=t.elapsed();
            std::thread::sleep(POLL_FAST);
        }
        let media=last.unwrap_or_default();
        println!("當前：{} · {} 時長{:.1}",media.source,media.title,media.duration);
        for (name,total) in ["系統會話(含酷狗/網易雲補全)","網易雲日誌","波點","酷我"].iter().zip(cost){println!("{name}: 平均 {:?}/輪",total/rounds);}
    }
}
#[cfg(all(test,windows))]
#[test]
#[ignore="本機實測：一次程序快照、一次波點/酷我發現各要多久"]
fn live_discovery_cost(){
    let t=std::time::Instant::now();for _ in 0..20{let _=player_process::pids_named("bodian_pc.exe");}println!("程序快照: {:?}/次",t.elapsed()/20);
    let t=std::time::Instant::now();let n=netease_timeline::kugou_position();println!("酷狗進度(含首次發現): {:?} {:?}",t.elapsed(),n);
    let t=std::time::Instant::now();for _ in 0..20{let _=netease_timeline::kugou_position();}println!("酷狗進度: {:?}/次",t.elapsed()/20);
    let mut cache=kugou_metadata::Cache::default();
    let t=std::time::Instant::now();for _ in 0..20{let _=cache.get("x","y",false);}println!("酷狗視窗核對: {:?}/次",t.elapsed()/20);
}

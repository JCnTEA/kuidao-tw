//! 系統會話缺少曲長時，只讀酷狗本機的歌曲元數據。
use std::{ffi::c_void,time::{Duration,Instant}};

fn track(raw:&[u8],filename:&str)->Option<(String,f64)>{
    let value=serde_json::Deserializer::from_slice(raw).into_iter::<serde_json::Value>().next()?.ok()?;
    if value.get("type")?.as_str()? != "song" || value.get("filename")?.as_str()? != filename{return None;}
    let hash=value.get("hash")?.as_str()?;
    if hash.len()!=32 || !hash.bytes().all(|b|b.is_ascii_hexdigit()){return None;}
    let duration=value.get("duration")?;
    let duration=duration.as_str().and_then(|v|v.parse::<f64>().ok()).or_else(||duration.as_f64())?/1000.0;
    if !duration.is_finite() || duration<=0.0 || duration>=86400.0{return None;}
    Some((filename.split_once(" - ")?.0.to_owned(),duration))
}

fn scan(bytes:&[u8],filename:&str)->Option<(String,f64)>{
    // 整段標記用字面量查找（內部為向量化子串搜索），不再逐個引號比對。
    static MARKER:std::sync::OnceLock<regex::bytes::Regex>=std::sync::OnceLock::new();
    let marker=MARKER.get_or_init(||regex::bytes::Regex::new(r#""duration":"#).unwrap());
    for found in marker.find_iter(bytes) {
        let offset=found.start();
        // 只保留歌曲字段；不讀取或記錄周圍的請求、帳號或日誌內容。
        let end=(offset+8192).min(bytes.len());
        let mut raw=Vec::with_capacity(end-offset+1);raw.push(b'{');raw.extend_from_slice(&bytes[offset..end]);
        if let Some(metadata)=track(&raw,filename){return Some(metadata);}
    }
    None
}

type Handle=*mut c_void;
#[repr(C)]
#[derive(Default)]
struct Region {base:usize,allocation:usize,allocation_protect:u32,size:usize,state:u32,protect:u32,kind:u32}
#[link(name="kernel32")]
extern "system" {
    fn OpenProcess(access:u32,inherit:i32,pid:u32)->Handle;
    fn CloseHandle(handle:Handle)->i32;
    fn ReadProcessMemory(handle:Handle,address:usize,buffer:*mut c_void,size:usize,read:*mut usize)->i32;
    fn VirtualQueryEx(handle:Handle,address:usize,region:*mut Region,size:usize)->usize;
}
#[link(name="user32")]
extern "system" {
    fn EnumWindows(callback:unsafe extern "system" fn(Handle,isize)->i32,param:isize)->i32;
    fn GetWindowTextW(window:Handle,text:*mut u16,size:i32)->i32;
    fn GetWindowThreadProcessId(window:Handle,pid:*mut u32)->u32;
}
struct Process(Handle);
impl Drop for Process {fn drop(&mut self){unsafe{CloseHandle(self.0);}}}

fn window_filename(text:&str,title:&str,artist:&str)->Option<String>{
    let filename=text.trim().strip_suffix(" - 酷狗音樂")?;
    let singer=filename.strip_suffix(&format!(" - {title}"))?;
    if singer.is_empty(){return None;}
    if !artist.trim().is_empty() && artist!=singer {
        let separators=['、','/','&',',','，',';','；'];
        let singers:Vec<_>=singer.split(separators).map(str::trim).filter(|s|!s.is_empty()).collect();
        let reported:Vec<_>=artist.split(separators).map(str::trim).filter(|s|!s.is_empty()).collect();
        if reported.is_empty() || !reported.iter().all(|s|singers.contains(s)){return None;}
    }
    Some(filename.to_owned())
}

fn current(title:&str,artist:&str)->Option<(u32,String)>{
    struct Search<'a>{title:&'a str,artist:&'a str,result:Option<(u32,String)>}
    unsafe extern "system" fn visit(window:Handle,param:isize)->i32{
        let search=&mut *(param as *mut Search<'_>);
        let mut text=[0u16;1024];let len=GetWindowTextW(window,text.as_mut_ptr(),1024);
        if len<=0{return 1;}
        let text=String::from_utf16_lossy(&text[..len as usize]);
        let Some(filename)=window_filename(&text,search.title,search.artist)else{return 1;};
        let mut pid=0;GetWindowThreadProcessId(window,&mut pid);
        // 標題相似的其他窗口（如瀏覽器）只做有限查詢，不申請內存讀取權限。
        if !super::player_process::image_is(pid,"KuGou.exe"){return 1;}
        search.result=Some((pid,filename.to_owned()));0
    }
    let mut search=Search{title,artist,result:None};
    unsafe{EnumWindows(visit,&mut search as *mut Search<'_> as isize);}
    search.result
}

/// 酷狗主窗口的歌曲標題（「歌手 - 歌名 - 酷狗音樂」），只取酷狗進程的窗口，供診斷時對照寫法。
fn kugou_windows()->Vec<String>{
    unsafe extern "system" fn visit(window:Handle,param:isize)->i32{
        let found=&mut *(param as *mut Vec<String>);
        let mut text=[0u16;1024];let len=GetWindowTextW(window,text.as_mut_ptr(),1024);
        if len<=0{return 1;}
        let text=String::from_utf16_lossy(&text[..len as usize]);
        let Some(song)=text.trim().strip_suffix(" - 酷狗音樂")else{return 1;};
        let mut pid=0;GetWindowThreadProcessId(window,&mut pid);
        if found.len()<4 && super::player_process::image_is(pid,"KuGou.exe") && !found.iter().any(|s|s==song){found.push(song.to_owned());}
        1
    }
    let mut found=Vec::new();
    unsafe{EnumWindows(visit,&mut found as *mut Vec<String> as isize);}
    found
}

/// 一輪掃描的進度：跨多次輪詢接著掃，掃到地址空間末尾繞回開頭，回到起點即完成一整圈。
#[derive(Clone,Copy,Debug,Default)]
struct Scan {cursor:usize,start:usize,wrapped:bool,spent:Duration,bytes:u64}
impl Scan {fn from(cursor:usize)->Self{Self{cursor,start:cursor,..Default::default()}}}

/// 每次輪詢最多佔用媒體線程這麼久，避免歌名、進度更新被長時間阻塞。
const SLICE:Duration=Duration::from_millis(250);
/// 一輪的總時間預算（與原先單次 2 秒相同，但拆成多片完成，且不再限制只掃前 256MB）。
const ATTEMPT:Duration=Duration::from_secs(2);

enum Step {Found((String,f64),usize),Pending,Done}

fn read_slice(pid:u32,filename:&str,state:&mut Scan,slice:Duration)->Step {read_slice_in(pid,"KuGou.exe",filename,state,slice)}

fn read_slice_in(pid:u32,exe:&str,filename:&str,state:&mut Scan,slice:Duration)->Step {
    // 進程號來自緩存時可能已被複用：讀取前再核對一次身份。
    if !super::player_process::image_is(pid,exe){return Step::Done;}
    let process=Process(unsafe{OpenProcess(0x410,0,pid)});if process.0.is_null(){return Step::Done;}
    let started=Instant::now();let mut bytes=Vec::new();
    // 每片至少讀一塊再看時間：保證每次輪詢都有進展，不會因準備工作耗時而原地打轉。
    let mut worked=false;
    let finish=|state:&mut Scan,started:Instant,step:Step|{state.spent+=started.elapsed();step};
    loop {
        if state.spent+started.elapsed()>=ATTEMPT {return finish(state,started,Step::Done);}
        if worked && started.elapsed()>=slice {return finish(state,started,Step::Pending);}
        if state.wrapped && state.cursor>=state.start {return finish(state,started,Step::Done);}
        let mut region=Region::default();
        let end=(unsafe{VirtualQueryEx(process.0,state.cursor,&mut region,std::mem::size_of::<Region>())}!=0).then(||region.base.checked_add(region.size)).flatten();
        let Some(end)=end else {
            // 地址空間末尾：繞回開頭繼續，已繞過一次說明整圈掃完。
            if state.wrapped || state.start==0 {return finish(state,started,Step::Done);}
            state.wrapped=true;state.cursor=0;continue;
        };
        if region.state!=0x1000 || region.kind!=0x20000 || region.protect!=4 {state.cursor=end;continue;}
        let offset=state.cursor.saturating_sub(region.base);
        if offset>=region.size {state.cursor=end;continue;}
        let size=(region.size-offset).min(1024*1024);let mut count=0;
        // 複用同一塊緩衝，避免每 1 MiB 重新分配並清零。
        if bytes.len()<size{bytes.resize(size,0);}
        if unsafe{ReadProcessMemory(process.0,region.base+offset,bytes.as_mut_ptr().cast(),size,&mut count)}!=0 {
            if let Some(metadata)=scan(&bytes[..count],filename){return finish(state,started,Step::Found(metadata,region.base));}
        }
        state.bytes+=size as u64;worked=true;
        // 相鄰兩塊重疊 8KB，跨塊的歌曲字段也能完整讀到。
        state.cursor=if size<=8192 || offset+size>=region.size {end}else{region.base+offset+size-8192};
    }
}

/// 每首歌的掃描結果寫入歌詞診斷日誌（曲長影響歌詞匹配），便於定位長期運行後找不到的問題。
fn log_scan(filename:&str,outcome:&str,state:&Scan,round:usize){
    if cfg!(test){return;}
    let line=format!("{} 酷狗曲長 · {filename} → {outcome} 第{round}輪 掃描{}MB 用時{}ms\n",
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),state.bytes/1024/1024,state.spent.as_millis());
    crate::resources::append_log(&crate::model::config_dir().join("diagnostics"),"lyrics.log",&line);
}

/// 同一首歌最多掃描這麼多次；第 n 次失敗後等待 RETRY[n] 再試，用完即放棄。
const RETRY:[Duration;3]=[Duration::from_secs(5),Duration::from_secs(30),Duration::MAX];
/// 歌曲不變時，窗口標題最多每秒核對一次（讀取其他進程的窗口標題是跨進程訊息）。
const WINDOW_RECHECK:Duration=Duration::from_secs(1);

#[derive(Default)]
pub struct Cache {
    key:Option<(u32,String)>,value:Option<(String,f64)>,checked:Option<Instant>,seen:Option<Instant>,
    failures:usize,
    /// 上次窗口核對：用哪組標題/歌手、何時、結果。
    looked:Option<(String,String,Instant,Option<(u32,String)>)>,
    /// 進行中的一輪掃描；一輪結束（預算用完或掃完一整圈）後，下一輪從這裡接著掃，不再從頭開始。
    scan:Option<Scan>,resume:usize,
    /// 上一首命中的進程和內存區塊：換歌後優先從這裡掃，歌曲資訊通常落在同一片內存。
    hint:Option<(u32,usize)>,
    /// 找不到對應酷狗窗口的歌曲：標題、歌手、開始時間、是否已寫進診斷日誌。
    missing:Option<(String,String,Instant,bool)>,
}
impl Cache {
    /// 同一首歌持續 3 秒對不上酷狗窗口時，把兩邊的寫法各記一次（切歌瞬間的短暫不同步不記）。
    fn report_miss(&mut self,title:&str,artist:&str){
        if !self.missing.as_ref().is_some_and(|(t,a,..)|t==title && a==artist) {
            self.missing=Some((title.to_owned(),artist.to_owned(),Instant::now(),false));
        }
        let Some((_,_,since,reported))=self.missing.as_mut() else{return;};
        if *reported || since.elapsed()<Duration::from_secs(3) {return;}
        *reported=true;
        let windows=kugou_windows();
        let shown=if windows.is_empty(){"無".to_owned()}else{windows.iter().map(|text|format!("{text}（{}字）",text.chars().count())).collect::<Vec<_>>().join("；")};
        let line=format!("{} 酷狗曲長 · {title} - {artist}（{}字）→ 未匹配到酷狗視窗，視窗標題：{shown}
",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),title.chars().count()+artist.chars().count()+3);
        if !cfg!(test) {crate::resources::append_log(&crate::model::config_dir().join("diagnostics"),"lyrics.log",&line);}
    }
    fn recent_key(&self,title:&str,artist:&str)->Option<(u32,String)>{
        if self.seen?.elapsed()>Duration::from_secs(2){return None;}
        let key=self.key.as_ref()?;
        window_filename(&format!("{} - 酷狗音樂",key.1),title,artist)?;
        Some(key.clone())
    }
    fn window(&mut self,title:&str,artist:&str,fresh:bool)->Option<(u32,String)>{
        if !fresh {
            if let Some((t,a,at,found))=&self.looked {
                if t==title && a==artist && at.elapsed()<WINDOW_RECHECK {return found.clone();}
            }
        }
        let found=current(title,artist);
        self.looked=Some((title.to_owned(),artist.to_owned(),Instant::now(),found.clone()));
        found
    }
    fn due(&self)->bool{
        self.value.is_none() && self.failures<RETRY.len()
            && self.checked.is_none_or(|at|self.failures==0 || at.elapsed()>=RETRY[self.failures-1])
    }
    /// `need_duration` 為假（系統已給出有效曲長）時只用窗口標題補全歌手，不掃描進程內存。
    pub fn get(&mut self,title:&str,artist:&str,need_duration:bool)->Option<(String,f64)>{
        // 循環重載時窗口標題會短暫消失；同一首歌最多沿用兩秒已核對的身份。
        let key=if let Some(key)=self.window(title,artist,false){self.seen=Some(Instant::now());key}
            else if let Some(key)=self.recent_key(title,artist){key}
            else{self.report_miss(title,artist);return None;};
        self.missing=None;
        if self.key.as_ref()!=Some(&key){
            self.key=Some(key.clone());self.value=None;self.checked=None;self.failures=0;self.scan=None;
            self.resume=self.hint.filter(|(pid,_)|*pid==key.0).map_or(0,|(_,base)|base);
        }
        if need_duration && (self.scan.is_some() || self.due()) {
            let mut state=self.scan.take().unwrap_or_else(||Scan::from(self.resume));
            match read_slice(key.0,&key.1,&mut state,SLICE) {
                Step::Found(value,base)=>{
                    log_scan(&key.1,"命中",&state,self.failures+1);
                    super::music_compat::set("KuGou.exe",super::music_compat::Item::Duration,super::music_compat::Support::Ok);
                    self.value=Some(value);self.hint=Some((key.0,base));
                }
                Step::Pending=>self.scan=Some(state),
                Step::Done=>{
                    self.failures+=1;self.checked=Some(Instant::now());self.resume=state.cursor;
                    log_scan(&key.1,"未找到",&state,self.failures);
                    if self.failures>=RETRY.len() {
                        super::music_compat::set("KuGou.exe",super::music_compat::Item::Duration,super::music_compat::Support::Unavailable("這首歌在酷狗內存裡沒有找到時長".into()));
                    }
                }
            }
            // 掃描期間可能已切歌，每一片之後都重新核對一次窗口。
            if self.window(title,artist,true).is_some_and(|next|next!=key){return None;}
        }
        // 曲長尚未就緒也能使用已核對的窗口署名，不能讓合唱退化為單個歌手。
        let singer=key.1.strip_suffix(&format!(" - {title}"))?.to_owned();
        Some((singer,self.value.as_ref().map_or(0.0,|(_,duration)|*duration)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeat_title_gap_keeps_only_recent_matching_identity(){
        let mut cache=Cache{key:Some((1,"添兒唄、劉瑾屹 - 溺雲歌".into())),seen:Some(Instant::now()),..Default::default()};
        assert!(cache.recent_key("溺雲歌","添兒唄").is_some());
        assert!(cache.recent_key("溺雲歌 (Live)","添兒唄").is_none());
        assert!(cache.recent_key("溺雲歌","其他歌手").is_none());
        cache.seen=Some(Instant::now()-Duration::from_secs(3));assert!(cache.recent_key("溺雲歌","添兒唄").is_none());
    }
    #[test]
    fn repeat_with_primary_artist_keeps_the_same_native_identity(){
        let window="添兒唄、劉瑾屹 - 溺雲歌 - 酷狗音樂 ";
        for artist in ["添兒唄、劉瑾屹","添兒唄","", "劉瑾屹/添兒唄"] {
            assert_eq!(window_filename(window,"溺雲歌",artist).as_deref(),Some("添兒唄、劉瑾屹 - 溺雲歌"),"迴圈播放時系統歌手欄位變化不能丟失本機歌曲身份");
        }
        for artist in ["添兒","另一歌手","添兒唄、另一歌手"] {assert!(window_filename(window,"溺雲歌",artist).is_none());}
        assert!(window_filename(window,"溺雲歌 (Live)","添兒唄").is_none());
    }
    #[test]
    fn window_mismatch_is_reported_once_after_it_persists(){
        let mut cache=Cache::default();
        cache.report_miss("See Tình","Hoàng Thùy Linh");
        assert!(cache.missing.as_ref().is_some_and(|m|!m.3),"剛切歌時的短暫不同步不記");
        cache.missing.as_mut().unwrap().2=Instant::now()-Duration::from_secs(4);
        cache.report_miss("See Tình","Hoàng Thùy Linh");
        assert!(cache.missing.as_ref().is_some_and(|m|m.3));
        cache.report_miss("別的歌","Hoàng Thùy Linh");
        assert!(cache.missing.as_ref().is_some_and(|m|m.0=="別的歌" && !m.3),"換歌重新計時");
    }
    #[test]
    fn failed_scans_back_off_then_stop_for_the_same_song(){
        let mut cache=Cache::default();
        assert!(cache.due());
        cache.checked=Some(Instant::now());cache.failures=1;assert!(!cache.due(),"失敗後不能下一輪立刻重掃");
        cache.checked=Some(Instant::now()-Duration::from_secs(6));assert!(cache.due());
        cache.failures=2;assert!(!cache.due());
        cache.checked=Some(Instant::now()-Duration::from_secs(31));assert!(cache.due());
        cache.failures=3;cache.checked=Some(Instant::now()-Duration::from_secs(3600));assert!(!cache.due(),"同一首歌三次失敗後不再掃描");
        cache.failures=0;cache.value=Some(("a".into(),1.0));assert!(!cache.due(),"已拿到曲長不再掃描");
    }
    #[test]
    fn scan_finds_markers_anywhere_including_after_quotes(){
        let raw=br#""""duration":x "\"duration":"232000","filename":"A - B","type":"song","hash":"8c4c4839a9aa418f528ebb7a8ce42afe"}"#;
        assert_eq!(scan(raw,"A - B"),Some(("A".into(),232.0)));
        assert!(scan(b"","A - B").is_none());assert!(scan(b"\"","A - B").is_none());
    }
    #[test]
    fn scan_ignores_unrelated_and_truncated_objects(){
        let raw=br#"junk{"duration":"1000","filename":"Other - Song","type":"song","hash":"0123456789abcdef0123456789abcdef"}garbage{"duration":"232000","filename":"cici_ - Song (Live)","type":"song","hash":"8c4c4839a9aa418f528ebb7a8ce42afe"}trailing"#;
        assert_eq!(scan(raw,"cici_ - Song (Live)"),Some(("cici_".into(),232.0)));
        assert!(scan(&raw[..raw.len()-20],"cici_ - Song (Live)").is_none());
    }
    /// 在本測試進程的堆內存裡放一段歌曲元數據，用本進程模擬酷狗進程。
    fn own_process()->(u32,String){
        let exe=std::env::current_exe().unwrap().file_name().unwrap().to_str().unwrap().to_owned();
        (std::process::id(),exe)
    }
    fn planted(filename:&str)->Vec<u8>{
        // 運行時拼接，避免整段元數據以字面量形式出現在映像裡（只掃描可寫的私有內存）。
        let marker=["\"dura","tion\":"].concat();let duration=["23","2000"].concat();
        let json=format!("{{{marker}\"{duration}\",\"filename\":\"{filename}\",\"type\":\"song\",\"hash\":\"8c4c4839a9aa418f528ebb7a8ce42afe\"}}");
        let mut blob=vec![0u8;64*1024];blob[30000..30000+json.len()].copy_from_slice(json.as_bytes());blob
    }
    /// 與 `Cache::get` 相同的續掃方式：一輪結束後下一輪從停下的位置接著掃。
    /// 返回（結果與命中區塊、是否繞回過開頭、切片數）。
    fn search(filename:&str,start:usize,slice:Duration)->(Option<((String,f64),usize)>,bool,usize){
        let (pid,exe)=own_process();search_in(pid,&exe,filename,start,slice)
    }
    fn search_in(pid:u32,exe:&str,filename:&str,start:usize,slice:Duration)->(Option<((String,f64),usize)>,bool,usize){
        let (mut cursor,mut wrapped,mut slices)=(start,false,0);
        for _ in 0..20 {
            let mut state=Scan::from(cursor);
            loop {
                match read_slice_in(pid,exe,filename,&mut state,slice) {
                    Step::Found(value,base)=>return (Some((value,base)),wrapped||state.wrapped,slices),
                    Step::Pending=>slices+=1,
                    Step::Done=>{wrapped|=state.wrapped;cursor=state.cursor;break;}
                }
            }
        }
        (None,wrapped,slices)
    }
    #[test]
    fn sliced_scan_resumes_across_polls_and_prefers_the_last_hit(){
        let filename="測試歌手 - 切片續掃";let blob=planted(filename);
        // 每片時間為零：每次輪詢只讀一塊就交還線程，進度必須保留到下一次。
        let (pid,exe)=own_process();let mut state=Scan::from(0);
        assert!(matches!(read_slice_in(pid,&exe,"測試歌手 - 不存在",&mut state,Duration::ZERO),Step::Pending));
        assert!(state.bytes>0 && state.cursor>0,"每片都應有進展");
        let (found,_,slices)=search(filename,0,Duration::ZERO);
        let (value,base)=found.expect("應能找到");
        assert_eq!(value,("測試歌手".into(),232.0));println!("零時長切片：{slices} 片後找到");
        // 下一首從上次命中的區塊開始：第一片就能找到。
        let mut hinted=Scan::from(base);
        assert!(matches!(read_slice_in(pid,&exe,filename,&mut hinted,SLICE),Step::Found(..)),"優先區塊應在第一片命中");
        drop(blob);
    }
    #[test]
    fn scan_wraps_from_the_end_of_memory_and_stops_after_a_pass(){
        let filename="測試歌手 - 繞回開頭";let blob=planted(filename);
        // 從用戶地址空間頂端開始：查詢失敗後應繞回開頭繼續，而不是直接放棄。
        let (found,wrapped,_)=search(filename,0x7fff_ffff_0000,SLICE);
        assert_eq!(found.map(|(value,_)|value),Some(("測試歌手".into(),232.0)));assert!(wrapped);
        drop(blob);
        // 內存裡沒有的歌：一輪用完預算或掃完一整圈就結束，不會一直掃下去。
        let (pid,exe)=own_process();let mut state=Scan::from(0);let started=Instant::now();
        loop {match read_slice_in(pid,&exe,"測試歌手 - 不存在",&mut state,SLICE){Step::Pending=>{},Step::Done=>break,Step::Found(..)=>panic!("不應命中")}}
        assert!(started.elapsed()<ATTEMPT+SLICE*2,"一輪不超過預算：{:?}",started.elapsed());
        // 不是酷狗進程：直接結束，不申請讀取權限。
        assert!(matches!(read_slice(pid,"測試歌手 - 不存在",&mut Scan::from(0),SLICE),Step::Done));
    }
    /// 正在播放的酷狗：主窗口標題形如「歌手 - 歌名 - 酷狗音樂」。
    fn kugou_now()->Option<(u32,String)>{
        struct Found(Option<(u32,String)>);
        unsafe extern "system" fn visit(window:Handle,param:isize)->i32{
            let found=&mut *(param as *mut Found);
            let mut text=[0u16;1024];let len=GetWindowTextW(window,text.as_mut_ptr(),1024);if len<=0{return 1;}
            let text=String::from_utf16_lossy(&text[..len as usize]);
            let Some(filename)=text.trim().strip_suffix(" - 酷狗音樂") else{return 1;};
            if !filename.contains(" - "){return 1;}
            let mut pid=0;GetWindowThreadProcessId(window,&mut pid);
            if !super::super::player_process::image_is(pid,"KuGou.exe"){return 1;}
            found.0=Some((pid,filename.to_owned()));0
        }
        let mut found=Found(None);unsafe{EnumWindows(visit,&mut found as *mut Found as isize);}found.0
    }
    #[test]
    #[ignore="需要本機酷狗正在播放：只讀掃描真實酷狗記憶體，強制從命中位置之後開始，驗證繞回開頭和跨輪續掃"]
    fn live_kugou_forced_wraparound_finds_the_same_duration(){
        let (pid,filename)=kugou_now().expect("未找到正在播放的酷狗視窗");
        let started=Instant::now();
        let (baseline,_,_)=search_in(pid,"KuGou.exe",&filename,0,SLICE);
        let (value,mut base)=baseline.expect("從頭掃描應能找到當前歌曲");
        println!("當前歌曲：{filename}，曲長 {} 秒；從頭掃描命中，用時 {:?}",value.1,started.elapsed());
        let process=Process(unsafe{OpenProcess(0x410,0,pid)});assert!(!process.0.is_null());
        // 從命中區塊之後開始掃：數據在起點之下，只能掃到地址空間末尾、繞回開頭才能找到。
        // 內存裡可能有多份副本：命中的若仍在起點之上，就把起點挪到它之後再試。
        for attempt in 1..=20 {
            let mut region=Region::default();
            assert!(unsafe{VirtualQueryEx(process.0,base,&mut region,std::mem::size_of::<Region>())}!=0);
            let after=region.base+region.size;let started=Instant::now();
            let (forced,wrapped,slices)=search_in(pid,"KuGou.exe",&filename,after,SLICE);
            let (forced_value,forced_base)=forced.expect("強制從高地址開始後應能找到");
            assert_eq!(kugou_now().map(|(_,f)|f).as_deref(),Some(filename.as_str()),"測試期間切歌了，請重跑");
            assert_eq!(forced_value,value,"繞回後找到的曲長應與從頭掃描一致");
            if wrapped {
                println!("第 {attempt} 次：起點 {after:#x}，繞回開頭後在 {forced_base:#x} 命中，{} 片、用時 {:?}",slices+1,started.elapsed());
                return;
            }
            println!("第 {attempt} 次：起點之上還有副本（{forced_base:#x}），繼續把起點後移");
            base=forced_base;
        }
        panic!("連續 20 次都在起點之上找到副本，未能驗證繞回");
    }
    #[test]
    fn exact_track_duration_rejects_other_versions_and_invalid_values(){
        let raw=br#"{"duration":"232000","filename":"cici_ - Song (Live)","type":"song","hash":"8c4c4839a9aa418f528ebb7a8ce42afe","progress":"560000"}"#;
        assert_eq!(track(raw,"cici_ - Song (Live)"),Some(("cici_".into(),232.0)));
        assert!(track(raw,"cici_ - Song").is_none());
        assert!(track(raw,"Other - Song (Live)").is_none());
        for invalid in ["0","-1000","NaN","86400000"] {
            let changed=String::from_utf8(raw.to_vec()).unwrap().replace("232000",invalid);
            assert!(track(changed.as_bytes(),"cici_ - Song (Live)").is_none());
        }
        assert!(track(b"broken","cici_ - Song (Live)").is_none());
    }
}

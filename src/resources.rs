//! 媒體後台任務的進程級上限，以及有容量限制的本機資源記錄。
use std::{path::{Path,PathBuf},sync::{atomic::{AtomicUsize,Ordering},Mutex},time::{Duration,Instant}};

pub struct Lane { pub name:&'static str, limit:usize, active:AtomicUsize, started:AtomicUsize, denied:AtomicUsize }
impl Lane {
    pub const fn new(name:&'static str,limit:usize)->Self{Self{name,limit,active:AtomicUsize::new(0),started:AtomicUsize::new(0),denied:AtomicUsize::new(0)}}
    pub fn acquire(&'static self)->Option<Lease>{
        if self.active.fetch_update(Ordering::AcqRel,Ordering::Relaxed,|n|(n<self.limit).then_some(n+1)).is_err(){self.denied.fetch_add(1,Ordering::Relaxed);return None;}
        self.started.fetch_add(1,Ordering::Relaxed);Some(Lease(self))
    }
    pub fn spawn(&'static self,run:impl FnOnce()+Send+'static)->Option<std::thread::JoinHandle<()>>{
        self.acquire()?.spawn(run)
    }
    pub fn active(&self)->usize{self.active.load(Ordering::Acquire)}
}
pub struct Lease(&'static Lane);
impl Lease {
    pub fn spawn(self,run:impl FnOnce()+Send+'static)->Option<std::thread::JoinHandle<()>>{
        std::thread::Builder::new().name(self.0.name.into()).spawn(move||{let _lease=self;run();}).ok()
    }
}
impl Drop for Lease {fn drop(&mut self){self.0.active.fetch_sub(1,Ordering::Release);}}

pub static POLLER:Lane=Lane::new("daozai-media",1);
// 允許舊實例正在退出與新實例短暫交接；阻塞任務跨緩存重建仍受此上限約束。
pub static ARTWORK:Lane=Lane::new("daozai-artwork",2);
pub static COVER:Lane=Lane::new("daozai-cover",2);
pub static LYRICS:Lane=Lane::new("daozai-lyrics",1);
pub static NATIVE_LYRICS:Lane=Lane::new("daozai-native-lyrics",2);
pub static CONTROL:Lane=Lane::new("daozai-control",1);
/// 音量混合器讀寫；驅動卡住時最多佔住這一個線程。
pub static VOLUME:Lane=Lane::new("daozai-volume",1);
pub static ASYNC:Lane=Lane::new("winrt-request",4);
pub static ASYNC_ARTWORK:Lane=Lane::new("winrt-artwork",2);
/// 系統檔案選擇視窗：模態對話框放到背景執行緒，介面才不會整段凍住；一次只開一個。
pub static PICKER:Lane=Lane::new("daozai-file-picker",1);
static MONITOR:Lane=Lane::new("daozai-diagnostics",1);
pub static TIMEOUTS:AtomicUsize=AtomicUsize::new(0);
pub static REJECTED_DATA:AtomicUsize=AtomicUsize::new(0);
pub static CONTROL_DROPPED:AtomicUsize=AtomicUsize::new(0);
pub static TEXTURES:AtomicUsize=AtomicUsize::new(0);
pub static TEXTURE_BYTES:AtomicUsize=AtomicUsize::new(0);
pub static GALLEYS:AtomicUsize=AtomicUsize::new(0);
/// 主界面畫了多少幀（每次採樣後清零）：閒置時本應很低，持續接近 60 幀/秒說明有東西在空轉重繪。
pub static FRAMES:AtomicUsize=AtomicUsize::new(0);
/// 本幀插件定時、結果回收和提醒的耗時（微秒），慢幀記錄分段用。
pub static PLUGIN_FRAME_US:AtomicUsize=AtomicUsize::new(0);

struct Pending { _lease:Lease, ready:Box<dyn Fn()->bool+Send>, after:Instant }
pub struct PendingRequests(Mutex<Vec<Pending>>);
impl PendingRequests {
    pub const fn new()->Self{Self(Mutex::new(Vec::new()))}
    pub fn retain(&self,lease:Lease,ready:impl Fn()->bool+Send+'static,after:Instant){
        self.0.lock().unwrap_or_else(|e|e.into_inner()).push(Pending{_lease:lease,ready:Box::new(ready),after});
    }
    pub fn reap(&self,now:Instant){self.0.lock().unwrap_or_else(|e|e.into_inner()).retain(|pending|now<pending.after || !(pending.ready)());}
}
pub static PENDING:PendingRequests=PendingRequests::new();

#[derive(Clone,Copy,Debug,Default)]
pub struct Snapshot {pub private_bytes:usize,pub working_bytes:usize,pub handles:u32,pub threads:u32}
impl Snapshot {
    pub fn abnormal(&self)->bool{self.private_bytes>=512*1024*1024 || self.threads>=128 || self.handles>=4096}
    /// 遠超正常範圍（正常約 20–40 線程、500–600 句柄、幾百 MB）：繼續運行會拖慢使用者電腦。
    pub fn severe(&self)->bool{self.private_bytes>=1536*1024*1024 || self.threads>=400 || self.handles>=16384}
}

/// 資源保護：連續兩次採樣都嚴重超標時「熔斷」，由界面暫停音樂讀取與歌詞等後台任務。
/// 只能觸發一次，重啟鬼島後恢復；單次尖峰不會觸發。
pub struct Guard {strikes:AtomicUsize,tripped:std::sync::atomic::AtomicBool}
impl Guard {
    pub const fn new()->Self{Self{strikes:AtomicUsize::new(0),tripped:std::sync::atomic::AtomicBool::new(false)}}
    /// 返回 true 表示這次採樣剛好觸發熔斷。
    pub fn observe(&self,value:&Snapshot)->bool{
        if !value.severe(){self.strikes.store(0,Ordering::Relaxed);return false;}
        self.strikes.fetch_add(1,Ordering::Relaxed)+1>=2 && !self.tripped.swap(true,Ordering::AcqRel)
    }
    pub fn tripped(&self)->bool{self.tripped.load(Ordering::Acquire)}
}
pub static GUARD:Guard=Guard::new();

#[cfg(windows)]
pub fn snapshot()->Option<Snapshot>{
    use std::ffi::c_void;
    #[repr(C)] #[derive(Default)]
    struct Memory {size:u32,faults:u32,peak:usize,working:usize,paged_peak:usize,paged:usize,nonpaged_peak:usize,nonpaged:usize,pagefile:usize,pagefile_peak:usize,private:usize}
    #[repr(C)] #[derive(Default)]
    struct Thread {size:u32,usage:u32,id:u32,owner:u32,base:i32,delta:i32,flags:u32}
    #[link(name="kernel32")]
    extern "system" {
        fn GetCurrentProcess()->*mut c_void;
        fn GetCurrentProcessId()->u32;
        fn K32GetProcessMemoryInfo(process:*mut c_void,memory:*mut Memory,size:u32)->i32;
        fn GetProcessHandleCount(process:*mut c_void,count:*mut u32)->i32;
        fn CreateToolhelp32Snapshot(flags:u32,pid:u32)->*mut c_void;
        fn Thread32First(snapshot:*mut c_void,entry:*mut Thread)->i32;
        fn Thread32Next(snapshot:*mut c_void,entry:*mut Thread)->i32;
        fn CloseHandle(handle:*mut c_void)->i32;
    }
    unsafe {
        let mut memory=Memory{size:std::mem::size_of::<Memory>()as u32,..Default::default()};let mut handles=0;
        if K32GetProcessMemoryInfo(GetCurrentProcess(),&mut memory,memory.size)==0 || GetProcessHandleCount(GetCurrentProcess(),&mut handles)==0{return None;}
        let snapshot=CreateToolhelp32Snapshot(4,0);if snapshot as isize == -1{return None;}
        let mut entry=Thread{size:std::mem::size_of::<Thread>()as u32,..Default::default()};let mut threads=0;
        let mut valid=Thread32First(snapshot,&mut entry);
        while valid!=0 {if entry.owner==GetCurrentProcessId(){threads+=1;}valid=Thread32Next(snapshot,&mut entry);}
        CloseHandle(snapshot);Some(Snapshot{private_bytes:memory.private,working_bytes:memory.working,handles,threads})
    }
}
#[cfg(not(windows))]
pub fn snapshot()->Option<Snapshot>{None}

const LOG_LIMIT:u64=512*1024;
fn append(directory:&Path,line:&str)->std::io::Result<()> {append_rotating(directory,"resources",LOG_LIMIT,line)}

/// 追加到 `<name>.log`，超過上限時整份滾動為 `<name>.previous.log`，兩份合計不超過 2 倍上限。
fn append_rotating(directory:&Path,name:&str,limit:u64,line:&str)->std::io::Result<()> {
    use std::io::Write;
    if line.len()as u64>limit{return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput,"resource record too large"));}
    std::fs::create_dir_all(directory)?;
    let path=directory.join(format!("{name}.log"));
    if std::fs::metadata(&path).map_or(0,|m|m.len())+line.len()as u64>limit {
        let previous=directory.join(format!("{name}.previous.log"));
        if previous.exists(){std::fs::remove_file(&previous)?;}
        std::fs::rename(&path,previous)?;
    }
    std::fs::OpenOptions::new().create(true).append(true).open(path)?.write_all(line.as_bytes())
}

/// 功能診斷日誌（如歌詞查找過程）：每份最多 256KB，寫入失敗不影響功能。
pub fn append_log(directory:&Path,file:&str,line:&str){
    let name=file.strip_suffix(".log").unwrap_or(file);
    let mut line:String=line.trim_end().chars().take(2048).collect();line.push('\n');
    let _=append_rotating(directory,name,256*1024,&line);
}

fn record(value:Snapshot)->String {
    use std::fmt::Write;
    let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    let mut line=format!("time={now} build={} pid={} private={} working={} handles={} threads={} abnormal={} timeouts={} rejected_data={} control_dropped={} textures={} texture_bytes={} galleys={}",
        option_env!("KUIDAO_BUILD_LABEL").or(option_env!("CARGO_PKG_VERSION")).unwrap_or("test"),std::process::id(),value.private_bytes,value.working_bytes,value.handles,value.threads,value.abnormal(),
        TIMEOUTS.load(Ordering::Relaxed),REJECTED_DATA.load(Ordering::Relaxed),CONTROL_DROPPED.load(Ordering::Relaxed),TEXTURES.load(Ordering::Relaxed),TEXTURE_BYTES.load(Ordering::Relaxed),GALLEYS.load(Ordering::Relaxed));
    // 距上次採樣的平均幀率（採樣間隔 5～30 秒不等，按實際間隔換算）。
    static LAST:Mutex<Option<Instant>>=Mutex::new(None);
    let since=LAST.lock().unwrap_or_else(|e|e.into_inner()).replace(Instant::now()).map(|at|at.elapsed().as_secs_f64());
    let frames=FRAMES.swap(0,Ordering::Relaxed);
    let _=write!(line," fps={}",since.filter(|s|*s>0.5).map_or("-".into(),|s|format!("{:.1}",frames as f64/s)));
    for lane in [&POLLER,&ARTWORK,&COVER,&LYRICS,&NATIVE_LYRICS,&CONTROL,&VOLUME,&ASYNC,&ASYNC_ARTWORK,&MONITOR] {
        let _=write!(line," {}={}/{}/{}/{}",lane.name,lane.active(),lane.limit,lane.started.load(Ordering::Relaxed),lane.denied.load(Ordering::Relaxed));
    }
    line.push('\n');line
}

/// 持有期間每 30 秒採樣；銷燬後喚醒退出，不讓界面等待後台線程。
pub struct Monitor { _stop:std::sync::mpsc::Sender<()> }
impl Monitor {
    pub fn start(directory:PathBuf)->Option<Self>{
        let (stop,rx)=std::sync::mpsc::channel();
        MONITOR.spawn(move||{
            loop {
                // 正常時 30 秒採樣一次；一旦異常改為 5 秒，儘早熔斷，避免像線程每秒暴漲時那樣拖到幾十 GB。
                let mut interval=Duration::from_secs(30);
                if let Some(value)=snapshot(){
                    let line=record(value);
                    let _=append(&directory,&line);
                    if value.abnormal(){let _=std::fs::write(directory.join("last-resource-anomaly.log"),&line);interval=Duration::from_secs(5);}
                    if GUARD.observe(&value){let _=std::fs::write(directory.join("resource-guard.log"),format!("已自動暫停音樂資訊、封面與歌詞讀取\n{line}"));}
                }
                if rx.recv_timeout(interval)!=Err(std::sync::mpsc::RecvTimeoutError::Timeout){break;}
            }
        })?;
        Some(Self{_stop:stop})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blocked_jobs_have_a_process_wide_bound_and_release_their_slots(){
        static LANE:Lane=Lane::new("resource-test",2);
        let (tx,rx)=std::sync::mpsc::channel::<()>();let (tx2,rx2)=std::sync::mpsc::channel::<()>();
        let a=LANE.spawn(move||{let _=rx.recv();}).unwrap();let b=LANE.spawn(move||{let _=rx2.recv();}).unwrap();
        for _ in 0..10_000 {assert!(LANE.spawn(||panic!("不得啟動額外任務")).is_none());}
        assert_eq!(LANE.active(),2);drop((tx,tx2));a.join().unwrap();b.join().unwrap();
        assert_eq!(LANE.active(),0);LANE.spawn(||{}).unwrap().join().unwrap();
    }
    #[test]
    fn cancellation_does_not_release_a_still_running_request(){
        static LANE:Lane=Lane::new("async-test",1);
        let pending=PendingRequests::new();let completed=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag=completed.clone();let now=Instant::now();
        pending.retain(LANE.acquire().unwrap(),move||flag.load(Ordering::Relaxed),now+Duration::from_secs(30));
        for second in 0..120 {pending.reap(now+Duration::from_secs(second));assert!(LANE.acquire().is_none());}
        completed.store(true,Ordering::Relaxed);pending.reap(now+Duration::from_secs(120));assert!(LANE.acquire().is_some());
    }
    #[test]
    fn diagnostic_history_rotates_and_rejects_oversized_records(){
        let root=std::env::temp_dir().join(format!("daozai-resource-log-{}",std::process::id()));
        let line="x".repeat(LOG_LIMIT as usize/3);
        for _ in 0..12{append(&root,&line).unwrap();}
        assert!(append(&root,&"x".repeat(LOG_LIMIT as usize+1)).is_err());
        for name in ["resources.log","resources.previous.log"] {let path=root.join(name);assert!(std::fs::metadata(&path).unwrap().len()<=LOG_LIMIT);std::fs::remove_file(path).unwrap();}
        std::fs::remove_dir(root).unwrap();
        let line=record(Snapshot::default());assert!(line.len()<4096);assert!(line.contains("daozai-cover="));
    }
    #[test]
    fn guard_trips_once_after_two_consecutive_severe_samples(){
        let guard=Guard::new();
        let normal=Snapshot{private_bytes:300*1024*1024,threads:30,handles:600,..Default::default()};
        let leaking=Snapshot{threads:9078,handles:58916,..normal};
        assert!(!guard.observe(&leaking),"單次尖峰不熔斷");
        assert!(!guard.observe(&normal));assert!(!guard.observe(&leaking));
        assert!(guard.observe(&leaking),"連續兩次嚴重超標才熔斷");assert!(guard.tripped());
        assert!(!guard.observe(&leaking),"只通知一次");
        for value in [Snapshot{private_bytes:2*1024*1024*1024,..normal},Snapshot{handles:20000,..normal},Snapshot{threads:500,..normal}] {assert!(value.severe());}
        assert!(!normal.severe() && !Snapshot{private_bytes:900*1024*1024,threads:120,handles:4000,..normal}.severe(),"偏高但未失控時不打斷使用者");
    }
    #[test]
    fn feature_logs_rotate_and_cap_each_line(){
        let root=std::env::temp_dir().join(format!("daozai-feature-log-{}",std::process::id()));
        for _ in 0..400 {append_log(&root,"lyrics.log",&"字".repeat(5000));}
        for name in ["lyrics.log","lyrics.previous.log"] {
            let path=root.join(name);let text=std::fs::read_to_string(&path).unwrap();
            assert!(text.len()<=256*1024);assert!(text.lines().all(|line|line.chars().count()==2048));
            std::fs::remove_file(path).unwrap();
        }
        std::fs::remove_dir(root).unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn sampling_does_not_leak_snapshot_handles(){
        // 句柄數是整個測試進程的，並行的其他測試也會開關線程和連線；取三輪中增長最小的一輪判斷。
        // 真正的洩漏每次採樣至少多一個句柄，100 次採樣會多出上百個，三輪都會超標。
        let growth=(0..3).map(|_|{
            let before=snapshot().unwrap();for _ in 0..100{let sample=snapshot().unwrap();assert!(sample.threads>0 && sample.private_bytes>0);}
            snapshot().unwrap().handles.saturating_sub(before.handles)
        }).min().unwrap();
        assert!(growth<=8,"取樣 100 次後控制程式碼增長 {growth} 個");
    }
}

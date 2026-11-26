//! Windows 訊息客戶端的通用生命週期、窗口檢查和後台喚起。
use super::{AccountHistory,Capture,Messenger,OpenError,Target,Unread};
use crate::model::Settings;
use std::{collections::HashMap,sync::mpsc,time::{Duration,Instant}};
use windows::{core::{w,PCWSTR,PWSTR},Win32::{
    Foundation::{CloseHandle,HWND,LPARAM},
    System::{Registry::{RegGetValueW,HKEY,RRF_RT_REG_SZ},Threading::{OpenProcess,QueryFullProcessImageNameW,PROCESS_NAME_WIN32,PROCESS_QUERY_LIMITED_INFORMATION}},
    UI::WindowsAndMessaging::*,
}};

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) struct TaskbarNotice {pub(crate) pid:u32,pub(crate) window:isize,pub(crate) started:Option<u64>}

pub(crate) trait TaskbarSource {
    fn registered(&self)->bool;
    fn take(&self)->Vec<TaskbarNotice>;
    fn forget(&self,pid:u32);
}
pub(crate) struct NativeAdapter {
    pub paths:fn()->Vec<String>,
    pub main_frame:fn(&str,u32,u32)->bool,
    pub accounts:Option<fn(&[String])->Option<Vec<Target>>>,
    pub ready:fn(Target)->bool,
    pub strict_window:bool,
    pub open:fn(Target,&[String],bool)->Result<Target,OpenError>,
    pub taskbar:Option<fn(eframe::egui::Context)->Result<Box<dyn TaskbarSource>,String>>,
}
pub(crate) fn client_paths(source:Messenger)->Vec<String>{(source.adapter().native.paths)()}
pub(crate) fn client_main_frame(source:Messenger,class:&str,style:u32,extended:u32)->bool{(source.adapter().native.main_frame)(class,style,extended)}
pub(crate) fn frame_style(class:&str,style:u32,extended:u32)->bool{
    let frame=(WS_SYSMENU|WS_MINIMIZEBOX).0;let resizable=(WS_CAPTION|WS_THICKFRAME|WS_MINIMIZEBOX).0;
    (style&frame==frame || (class.starts_with("Qt") || class=="Chrome_WidgetWin_1") && style&resizable==resizable)
        && extended&(WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE).0==0
}
pub fn describe_foreground()->String{unsafe{let window=GetForegroundWindow();format!("{:#x} {} 程序 {}",window.0 as usize,class_name(window),window_pid(window))}}
pub fn trace(line:impl AsRef<str>){crate::resources::append_log(&crate::model::config_dir().join("diagnostics"),"messenger-events",line.as_ref());}

// 點島上的提醒喚起客戶端：各客戶端把走過的步驟記在當前線程，結束時與結果合成一行寫入
// messenger-events.log。只記窗口句柄、類名、可見狀態和進程號，不記窗口標題或訊息內容。
thread_local!{static WAKE_STEPS:std::cell::RefCell<Vec<String>>=const{std::cell::RefCell::new(Vec::new())};}
pub(crate) fn wake_step(line:impl Into<String>){WAKE_STEPS.with(|steps|steps.borrow_mut().push(line.into()));}
/// 窗口當時的狀態：是否還在、類名、隱藏 / 最小化、所屬進程。
pub(crate) fn describe_window(window:HWND)->String{unsafe{
    if window.0.is_null() || !IsWindow(window).as_bool(){return format!("{:#x} 已不存在",window.0 as usize);}
    format!("{:#x} {}{}{} 程序 {}",window.0 as usize,class_name(window),
        if IsWindowVisible(window).as_bool(){""}else{" 隱藏"},if IsIconic(window).as_bool(){" 最小化"}else{""},window_pid(window))
}}
fn wake_line(head:&str,steps:&[String],error:Option<&OpenError>,elapsed:Duration)->String{
    let outcome=match error{None=>"已發出打開請求".to_string(),Some(error)=>format!("失敗 {error:?}")};
    let steps=if steps.is_empty(){"-".to_string()}else{steps.join(" → ")};
    format!("{head} → {steps} → 結果:{outcome} 用時{}ms",elapsed.as_millis())
}
#[cfg(test)]
thread_local!{static LAST_WAKE:std::cell::RefCell<String>=const{std::cell::RefCell::new(String::new())};}
#[cfg(test)]
pub(crate) fn take_wake_steps()->Vec<String>{WAKE_STEPS.with(|steps|std::mem::take(&mut *steps.borrow_mut()))}

pub(crate) fn normalized(path: &str) -> String {
    path.trim_start_matches(r"\\?\").replace('/', "\\").to_lowercase()
}

pub(crate) fn registry_string(root:HKEY,key:PCWSTR,name:PCWSTR)->Option<String>{
    let mut data=[0u16;2048];let mut size=(data.len()*2) as u32;
    unsafe {RegGetValueW(root,key,name,RRF_RT_REG_SZ,None,Some(data.as_mut_ptr().cast()),Some(&mut size))}.ok().ok()?;
    let length=data.iter().position(|c|*c==0).unwrap_or(data.len());
    Some(String::from_utf16_lossy(&data[..length]))
}

pub(crate) fn company_name(path:&str)->String{
    use windows::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW,GetFileVersionInfoW,VerQueryValueW};
    static CACHE:std::sync::Mutex<Option<std::collections::HashMap<String,String>>>=std::sync::Mutex::new(None);
    if let Some(name)=CACHE.lock().ok().and_then(|c|c.as_ref().and_then(|c|c.get(path).cloned())){return name;}
    let name=unsafe{(||->Option<String>{
        let wide:Vec<u16>=path.encode_utf16().chain(Some(0)).collect();
        let size=GetFileVersionInfoSizeW(PCWSTR(wide.as_ptr()),None);
        if size==0 {return None;}
        let mut data=vec![0u8;size as usize];
        GetFileVersionInfoW(PCWSTR(wide.as_ptr()),0,size,data.as_mut_ptr().cast()).ok()?;
        let mut pointer=std::ptr::null_mut();let mut length=0u32;
        if !VerQueryValueW(data.as_ptr().cast(),w!("\\VarFileInfo\\Translation"),&mut pointer,&mut length).as_bool() || length<4 {return None;}
        let translation=*(pointer as *const [u16;2]);
        let key:Vec<u16>=format!("\\StringFileInfo\\{:04x}{:04x}\\CompanyName",translation[0],translation[1]).encode_utf16().chain(Some(0)).collect();
        if !VerQueryValueW(data.as_ptr().cast(),PCWSTR(key.as_ptr()),&mut pointer,&mut length).as_bool() || length==0 {return None;}
        let text=std::slice::from_raw_parts(pointer as *const u16,length as usize);
        Some(String::from_utf16_lossy(text).trim_end_matches('\0').to_string())
    })()}.unwrap_or_default();
    if let Ok(mut cache)=CACHE.lock(){cache.get_or_insert_with(Default::default).insert(path.to_string(),name.clone());}
    name
}

pub(crate) fn path_allowed(image:&str,paths:&[String])->bool{
    if paths.iter().any(|p|p==image){return true;}
    let file=image.rsplit('\\').next().unwrap_or(image);
    paths.iter().any(|p|p.strip_prefix('*').is_some_and(|name|name==file)) && publisher_allowed(file,&company_name(image))
}

fn publisher_allowed(file:&str,company:&str)->bool{
    let company=company.to_lowercase();
    Messenger::ALL.into_iter().any(|client|client.adapter().process_names.iter().any(|name|name.eq_ignore_ascii_case(file))
        && client.adapter().publishers.iter().any(|publisher|company.contains(publisher)))
}

pub(crate) unsafe fn matches(hwnd: HWND, paths: &[String]) -> bool {
    if hwnd.0.is_null(){return false;}
    let mut pid=0;GetWindowThreadProcessId(hwnd,Some(&mut pid));
    pid_matches(pid,paths)
}

pub(crate) unsafe fn pid_matches(pid:u32, paths: &[String]) -> bool {
    if pid==0 {return false;}
    let Ok(process)=OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,false,pid) else{return false};
    let mut path=[0u16;32768];let mut length=path.len() as u32;
    let result=QueryFullProcessImageNameW(process,PROCESS_NAME_WIN32,PWSTR(path.as_mut_ptr()),&mut length);
    let _=CloseHandle(process);
    result.is_ok() && path_allowed(&normalized(&String::from_utf16_lossy(&path[..length as usize])),paths)
}

pub(crate) fn window_pid(hwnd:HWND)->u32{
    let mut pid=0;
    if !hwnd.0.is_null(){unsafe{GetWindowThreadProcessId(hwnd,Some(&mut pid));}}
    pid
}

pub(crate) unsafe fn foreground_main_pid(source:Messenger,paths:&[String])->Option<u32>{
    main_pid_at_window(GetForegroundWindow(),source,paths)
}

unsafe fn main_pid_at_window(hwnd:HWND,source:Messenger,paths:&[String])->Option<u32>{
    if IsIconic(hwnd).as_bool() || !visible_main_frame(hwnd,source) || !matches(hwnd,paths){return None;}
    if Messenger::OFFICE.contains(&source) && !super::desktop::main_window(hwnd,source){return None;}
    let pid=window_pid(hwnd);
    Some(if source==Messenger::Wechat{crate::wechat::account_pid(pid,paths)}else{pid})
}

pub(crate) unsafe fn visible_main_frame(hwnd:HWND,source:Messenger)->bool{
    if !IsWindowVisible(hwnd).as_bool() || GetWindow(hwnd,GW_OWNER).is_ok(){return false;}
    let mut class=[0u16;128];
    let n=GetClassNameW(hwnd,&mut class).max(0) as usize;
    client_main_frame(source,&String::from_utf16_lossy(&class[..n]),GetWindowLongW(hwnd,GWL_STYLE) as u32,GetWindowLongW(hwnd,GWL_EXSTYLE) as u32)
}

pub(crate) unsafe fn usable_main_frame(hwnd:HWND,source:Messenger)->bool{
    use windows::Win32::{Foundation::RECT,Graphics::{Dwm::{DwmGetWindowAttribute,DWMWA_CLOAKED},Gdi::{MonitorFromWindow,MONITOR_DEFAULTTONULL}}};
    if !visible_main_frame(hwnd,source){return false;}
    let mut cloaked=0u32;
    if DwmGetWindowAttribute(hwnd,DWMWA_CLOAKED,(&mut cloaked as *mut u32).cast(),4).is_ok() && cloaked!=0 {return false;}
    if IsIconic(hwnd).as_bool(){return true;}
    let mut r=RECT::default();
    GetWindowRect(hwnd,&mut r).is_ok() && r.right-r.left>=300 && r.bottom-r.top>=300 && !MonitorFromWindow(hwnd,MONITOR_DEFAULTTONULL).0.is_null()
}

pub(crate) fn parent_processes()->std::collections::HashMap<u32,u32>{
    use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot,Process32FirstW,Process32NextW,PROCESSENTRY32W,TH32CS_SNAPPROCESS};
    let mut parents=std::collections::HashMap::new();
    unsafe{
        let Ok(snapshot)=CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS,0) else {return parents};
        let mut entry=PROCESSENTRY32W{dwSize:std::mem::size_of::<PROCESSENTRY32W>() as u32,..Default::default()};
        let mut ok=Process32FirstW(snapshot,&mut entry).is_ok();
        while ok {parents.insert(entry.th32ProcessID,entry.th32ParentProcessID);ok=Process32NextW(snapshot,&mut entry).is_ok();}
        let _=CloseHandle(snapshot);
    }
    parents
}

pub(crate) fn ancestors(pid:u32,parents:&std::collections::HashMap<u32,u32>)->Vec<u32>{
    let mut chain=Vec::new();let mut current=pid;
    for _ in 0..8 {
        match parents.get(&current){Some(&parent) if parent!=0 && parent!=current && parent!=pid && !chain.contains(&parent)=>{chain.push(parent);current=parent;},_=>break}
    }
    chain
}

pub(crate) fn class_name(hwnd:HWND)->String{
    let mut class=[0u16;128];
    let n=unsafe{GetClassNameW(hwnd,&mut class)}.max(0) as usize;
    String::from_utf16_lossy(&class[..n])
}

pub(crate) unsafe fn host_icons(hosts:&[(u32,HWND)],pid:u32)->Vec<(HWND,u32)>{
    hosts.iter().filter(|(p,_)|*p==pid).flat_map(|&(_,h)|
        (0..=256).filter(move |id|tray_rect(h,*id).is_some()).map(move |id|(h,id))).collect()
}

pub(crate) fn account_tray_pid(pid:u32,hosts:&[u32],parents:&std::collections::HashMap<u32,u32>)->Option<u32>{
    if pid==0 {return None;}
    if hosts.contains(&pid){return Some(pid);}
    if let Some(ancestor)=ancestors(pid,parents).into_iter().find(|a|hosts.contains(a)){return Some(ancestor);}
    let below:Vec<u32>=hosts.iter().copied().filter(|h|ancestors(*h,parents).contains(&pid)).collect();
    match below.as_slice(){[one]=>Some(*one),_=>None}
}

pub(crate) unsafe fn tray_rect(host:HWND,id:u32)->Option<windows::Win32::Foundation::RECT>{
    use windows::Win32::UI::Shell::{Shell_NotifyIconGetRect,NOTIFYICONIDENTIFIER};
    let icon=NOTIFYICONIDENTIFIER{cbSize:std::mem::size_of::<NOTIFYICONIDENTIFIER>() as u32,hWnd:host,uID:id,..Default::default()};
    Shell_NotifyIconGetRect(&icon).ok()
}

pub(crate) unsafe fn visible_main_window_of(pid:u32,source:Messenger,paths:&[String])->Option<HWND>{
    use windows::Win32::Foundation::{BOOL,RECT};
    struct Find<'a>{pid:u32,source:Messenger,paths:&'a [String],best:Option<(i64,HWND)>}
    unsafe extern "system" fn visit(hwnd:HWND,data:LPARAM)->BOOL{
        let find=&mut *(data.0 as *mut Find);
        if window_pid(hwnd)==find.pid && usable_main_frame(hwnd,find.source) && matches(hwnd,find.paths){
            let mut r=RECT::default();let _=GetWindowRect(hwnd,&mut r);
            let area=if IsIconic(hwnd).as_bool(){0}else{i64::from(r.right-r.left)*i64::from(r.bottom-r.top)};
            if find.best.is_none_or(|(old,_)|area>old){find.best=Some((area,hwnd));}
        }
        true.into()
    }
    let mut find=Find{pid,source,paths,best:None};
    let _=EnumWindows(Some(visit),LPARAM((&mut find as *mut Find) as isize));
    find.best.map(|(_,hwnd)|hwnd)
}

pub(crate) fn process_started(process:windows::Win32::Foundation::HANDLE)->Option<u64>{
    use windows::Win32::{Foundation::FILETIME,System::Threading::GetProcessTimes};
    let (mut created,mut exited,mut kernel,mut user)=(FILETIME::default(),FILETIME::default(),FILETIME::default(),FILETIME::default());
    unsafe{GetProcessTimes(process,&mut created,&mut exited,&mut kernel,&mut user)}.ok()?;
    Some((u64::from(created.dwHighDateTime)<<32)|u64::from(created.dwLowDateTime))
}

pub(crate) fn client_processes(source:Messenger)->Option<Vec<u32>>{
    use windows::Win32::{Foundation::ERROR_NO_MORE_FILES,System::{
        Diagnostics::ToolHelp::{CreateToolhelp32Snapshot,Process32FirstW,Process32NextW,PROCESSENTRY32W,TH32CS_SNAPPROCESS},
        RemoteDesktop::ProcessIdToSessionId,
    }};
    unsafe{
        let mut session=0;ProcessIdToSessionId(std::process::id(),&mut session).ok()?;
        let snapshot=windows::core::Owned::new(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS,0).ok()?);
        let mut entry=PROCESSENTRY32W{dwSize:std::mem::size_of::<PROCESSENTRY32W>() as u32,..Default::default()};
        Process32FirstW(*snapshot,&mut entry).ok()?;
        let mut pids=Vec::new();
        loop{
            let end=entry.szExeFile.iter().position(|c|*c==0).unwrap_or(entry.szExeFile.len());
            let name=String::from_utf16_lossy(&entry.szExeFile[..end]);
            let candidate=source.adapter().process_names.iter().any(|candidate|name.eq_ignore_ascii_case(candidate));
            if candidate{
                let mut client_session=0;ProcessIdToSessionId(entry.th32ProcessID,&mut client_session).ok()?;
                if client_session==session{pids.push(entry.th32ProcessID);}
            }
            match Process32NextW(*snapshot,&mut entry){
                Ok(())=>{},
                Err(error) if error.code()==windows::core::HRESULT::from_win32(ERROR_NO_MORE_FILES.0)=>return Some(pids),
                Err(_)=>return None,
            }
        }
    }
}

pub enum TaskbarEvent {Status(Messenger,String),Message(Unread,Option<Target>)}
struct TaskbarEntry {client:Messenger,source:Option<Box<dyn TaskbarSource>>,retry:Option<Instant>,status:String}
struct ToastEntry {client:Messenger,paths:Vec<String>,history:AccountHistory}
pub struct ToastAccounts {entries:Vec<ToastEntry>}
impl Default for ToastAccounts {
    fn default()->Self{Self{entries:Messenger::ALL.into_iter().filter(|client|client.adapter().capture==Capture::SystemToast && client.adapter().native.accounts.is_some())
        .map(|client|ToastEntry{client,paths:client_paths(client),history:AccountHistory::new(client)}).collect()}}
}
impl ToastAccounts {
    pub fn sample(&mut self){
        for entry in &mut self.entries{
            let accounts=(entry.client.adapter().native.accounts.unwrap())(&entry.paths);
            let at=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()
                .and_then(|time|i64::try_from(time.as_nanos()/100+116444736000000000).ok());
            entry.history.observe(at.unwrap_or(0),if at.is_some(){accounts}else{None});
        }
    }
    pub fn target(&mut self,client:Messenger,id:u32,created:i64)->Option<Target>{
        self.entries.iter_mut().find(|entry|entry.client==client)?.history.target(id,created,client.adapter().native.ready)
    }
    pub fn retain(&mut self,exists:impl Fn(Messenger,u32,i64)->bool){
        for entry in &mut self.entries{let client=entry.client;entry.history.retain(|id,created|exists(client,id,created));}
    }
}

pub struct Runtime {
    processes:HashMap<u32,Option<windows::core::Owned<windows::Win32::Foundation::HANDLE>>>,
    targets:HashMap<Unread,Target>,
    taskbar:Vec<TaskbarEntry>,
}
impl Default for Runtime {
    fn default()->Self{Self{processes:HashMap::new(),targets:HashMap::new(),taskbar:Messenger::ALL.into_iter()
        .filter(|client|client.adapter().native.taskbar.is_some()).map(|client|TaskbarEntry{client,source:None,retry:None,status:String::new()}).collect()}}
}
impl Runtime {
    pub fn foreground(&self,sources:&[Messenger])->Option<(Messenger,u32)>{
        let window=unsafe{GetForegroundWindow()};
        let account=sources.iter().find_map(|&client|{
            self.foreground_at(window,client,&client_paths(client)).map(|pid|(client,pid))
        })?;
        (unsafe{GetForegroundWindow()}==window).then_some(account)
    }
    fn foreground_at(&self,window:HWND,client:Messenger,paths:&[String])->Option<u32>{
        let pid=unsafe{main_pid_at_window(window,client,paths)}?;
        let pid=if client==Messenger::Qq{
            let accounts=(client.adapter().native.accounts?)(paths)?;
            self.qq_account_pid(pid,&accounts,&parent_processes())?
        }else{pid};
        let mut targets=self.targets.values().copied().filter(|target|target.unread.source==client && target.unread.pid==pid).peekable();
        if client.adapter().keep_unidentified && targets.peek().is_none(){return None;}
        let ready=targets.all(|target|if Messenger::OFFICE.contains(&client){
            super::desktop::matches_foreground(target,window,paths)
        }else{(client.adapter().native.ready)(target)});
        ready.then_some(pid)
    }
    fn qq_account_pid(&self,pid:u32,accounts:&[Target],parents:&HashMap<u32,u32>)->Option<u32>{
        let mut pids:Vec<_>=accounts.iter().filter(|target|target.unread.source==Messenger::Qq).map(|target|target.unread.pid).collect();
        pids.sort_unstable();pids.dedup();
        let pid=account_tray_pid(pid,&pids,parents)?;
        self.targets.values().any(|target|target.unread.source==Messenger::Qq && target.unread.pid==pid).then_some(pid)
    }
    pub fn poll_taskbar(&mut self,ctx:&eframe::egui::Context,settings:&Settings)->Vec<TaskbarEvent>{
        let mut events=Vec::new();
        for entry in &mut self.taskbar{
            if entry.source.as_ref().is_some_and(|source|!source.registered()){entry.source=None;}
            let enabled=(entry.client.adapter().enabled)(settings);
            let status=if !enabled || !settings.notifications{
                entry.source=None;entry.retry=None;
                if enabled{"未在監聽 · 提醒總開關已關閉".to_string()}else{String::new()}
            }else{
                if entry.source.is_none() && entry.retry.is_none_or(|at|at.elapsed()>=Duration::from_secs(10)){
                    match (entry.client.adapter().native.taskbar.unwrap())(ctx.clone()){
                        Ok(source)=>{entry.source=Some(source);entry.retry=None;}
                        Err(error)=>{entry.retry=Some(Instant::now());entry.status=format!("{error} · 10 秒後重試");events.push(TaskbarEvent::Status(entry.client,entry.status.clone()));}
                    }
                }
                if entry.source.is_some(){format!("正在監聽 · {}工作列閃動時提醒",entry.client.name())}else{entry.status.clone()}
            };
            if entry.status!=status{entry.status=status.clone();events.push(TaskbarEvent::Status(entry.client,status));}
            if let Some(source)=&entry.source{
                for notice in source.take().into_iter().filter(|notice|notice.pid!=0 || entry.client.adapter().keep_unidentified){
                    let unread=Unread{source:entry.client,pid:notice.pid,window:notice.window};
                    events.push(TaskbarEvent::Message(unread,notice.started.map(|started|Target{unread,started})));
                }
            }
        }
        events
    }
    /// 前台已確認查看該帳號時同步重設監聽端，避免遺漏激活事件後吞掉下一條訊息。
    pub fn viewed_client(&self,client:Messenger,pid:u32){
        if pid==0{return;}
        for entry in &self.taskbar{if entry.client==client{if let Some(source)=&entry.source{source.forget(pid);}}}
    }
    pub fn track(&mut self,target:Target)->Result<(),OpenError>{
        let process=check_process(target)?;
        let unread=target.unread;
        // 辦公客戶端的閃動已提供原進程代際。原窗口隨後關閉時仍可登記，點擊時再核對是否有唯一的替換主窗。
        if unread.source.adapter().native.strict_window && (unread.window==0
            || !Messenger::OFFICE.contains(&unread.source) && !original_window_exists(unread)){return Err(OpenError::Unavailable);}
        self.processes.insert(target.unread.pid,Some(process));self.targets.insert(target.unread,target);Ok(())
    }
    pub fn register_taskbar(&mut self,unread:Unread,target:Option<Target>)->Option<Unread>{
        if unread.pid==0{return Some(unread);}
        if !Messenger::OFFICE.contains(&unread.source){
            return (self.track_unread(unread)!=Err(OpenError::Exited)).then_some(unread);
        }
        // 無法將登記失敗的閃動顯示成已確認帳號，也不能事後按 PID 重新認領一個進程。
        match target.filter(|target|target.unread==unread).map_or(Err(OpenError::Unavailable),|target|self.track(target)){
            Ok(())=>Some(unread),
            Err(OpenError::Exited)=>None,
            Err(error)=>{
                if !cfg!(test){trace(format!("登記 {} 帳號 {} · 窗口 {:#x}：{error:?}",unread.source.name(),unread.pid,unread.window));}
                Some(Unread::of(unread.source))
            }
        }
    }
    pub fn replace_opened(&mut self,old:Unread,opened:Target)->Result<(),OpenError>{
        let previous=self.targets.get(&old).ok_or(OpenError::Unavailable)?;
        if old.source!=opened.unread.source || old.pid!=opened.unread.pid || previous.started!=opened.started
            || original_window_exists(old) || !original_window_exists(opened.unread){return Err(OpenError::Unavailable);}
        self.track(opened)?;
        self.targets.remove(&old);
        Ok(())
    }
    fn track_unread(&mut self,unread:Unread)->Result<(),OpenError>{
        use windows::Win32::System::Threading::PROCESS_SYNCHRONIZE;
        if unread.pid==0{return Err(OpenError::Unidentified);}
        let process=unsafe{OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION|PROCESS_SYNCHRONIZE,false,unread.pid)}.map_err(process_error)?;
        let process=unsafe{windows::core::Owned::new(process)};
        let started=process_started(*process).ok_or(OpenError::Unavailable)?;
        self.track(Target{unread,started})
    }
    pub fn request(&self,unread:Unread,settings:&Settings)->OpenRequest{
        OpenRequest{unread,target:self.targets.get(&unread).copied(),allow_tray:(unread.source.adapter().tray_enabled)(settings)}
    }
    pub fn exited(&mut self,unreads:&[Unread])->Vec<Unread>{
        let exited=self.exited_with(unreads,client_processes);
        for unread in &exited{for entry in &self.taskbar{if entry.client==unread.source{if let Some(source)=&entry.source{source.forget(unread.pid);}}}}
        exited
    }
    fn exited_with(&mut self,unreads:&[Unread],mut discover:impl FnMut(Messenger)->Option<Vec<u32>>)->Vec<Unread>{
        use windows::Win32::{Foundation::{ERROR_INVALID_PARAMETER,WAIT_OBJECT_0},System::Threading::{WaitForSingleObject,PROCESS_SYNCHRONIZE}};
        self.processes.retain(|pid,_|unreads.iter().any(|unread|unread.pid==*pid));self.targets.retain(|unread,_|unreads.contains(unread));
        let mut exited=Vec::new();
        for &unread in unreads{
            if unread.pid==0{if discover(unread.source).is_some_and(|pids|pids.is_empty()){exited.push(unread);}continue;}
            let process=self.processes.entry(unread.pid).or_default();
            if process.is_none(){
                match unsafe{OpenProcess(PROCESS_SYNCHRONIZE,false,unread.pid)}{
                    Ok(handle)=>*process=Some(unsafe{windows::core::Owned::new(handle)}),
                    Err(error)=>{if error.code()==windows::core::HRESULT::from_win32(ERROR_INVALID_PARAMETER.0){exited.push(unread);}continue;}
                }
            }
            if process.as_ref().is_some_and(|handle|unsafe{WaitForSingleObject(**handle,0)==WAIT_OBJECT_0}){exited.push(unread);}
        }
        exited
    }
}
fn process_error(error:windows::core::Error)->OpenError{
    if error.code()==windows::core::HRESULT::from_win32(windows::Win32::Foundation::ERROR_INVALID_PARAMETER.0){OpenError::Exited}else{OpenError::Unavailable}
}
fn check_process(target:Target)->Result<windows::core::Owned<windows::Win32::Foundation::HANDLE>,OpenError>{unsafe{
    use windows::Win32::{Foundation::{WAIT_OBJECT_0,WAIT_TIMEOUT},System::Threading::{WaitForSingleObject,PROCESS_SYNCHRONIZE}};
    let process=windows::core::Owned::new(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION|PROCESS_SYNCHRONIZE,false,target.unread.pid).map_err(process_error)?);
    let started=process_started(*process).ok_or(OpenError::Unavailable)?;
    let state=WaitForSingleObject(*process,0);
    if started!=target.started || state==WAIT_OBJECT_0{return Err(OpenError::Exited);}
    if state!=WAIT_TIMEOUT{return Err(OpenError::Unavailable);}
    Ok(process)
}}
fn original_window_exists(unread:Unread)->bool{
    let window=HWND(unread.window as *mut _);
    unsafe{IsWindow(window).as_bool() && window_pid(window)==unread.pid}
}
fn resolve_open_target(target:Target,paths:&[String])->Result<Target,OpenError>{
    if !target.unread.source.adapter().native.strict_window || original_window_exists(target.unread){return Ok(target);}
    if Messenger::OFFICE.contains(&target.unread.source){
        if let Some(replacement)=super::desktop::rebuild_target(target,paths){
            wake_step(format!("原主窗口已重建 · {:#x} → {:#x}",target.unread.window,replacement.unread.window));
            return Ok(replacement);
        }
    }
    wake_step("原窗口已失效，無法確認唯一的替換主窗");
    Err(OpenError::Unavailable)
}

#[derive(Clone,Copy)]
pub struct OpenRequest {unread:Unread,target:Option<Target>,allow_tray:bool}
static OPEN:crate::resources::Lane=crate::resources::Lane::new("daozai-messenger-open",1);
impl OpenRequest {
    pub fn start(self,ctx:eframe::egui::Context)->Result<mpsc::Receiver<Result<Target,OpenError>>,OpenError>{
        let lease=OPEN.acquire().ok_or(OpenError::Busy)?;
        if self.unread.unidentified(){return Err(OpenError::Unidentified);}
        let (tx,rx)=mpsc::channel();
        lease.spawn(move||{let _=tx.send(self.run());ctx.request_repaint();}).ok_or(OpenError::Unavailable)?;
        Ok(rx)
    }
    fn run(self)->Result<Target,OpenError>{self.run_with_paths(&client_paths(self.unread.source))}
    pub(crate) fn run_with_paths(self,paths:&[String])->Result<Target,OpenError>{
        WAKE_STEPS.with(|steps|steps.borrow_mut().clear());
        let started=Instant::now();
        let head=format!("喚起 {} 帳號 {} · 提醒視窗 {} · 允許系統匣={}",
            self.unread.source.name(),self.unread.pid,describe_window(HWND(self.unread.window as *mut _)),self.allow_tray);
        let result=self.attempt(paths);
        let steps=WAKE_STEPS.with(|steps|std::mem::take(&mut *steps.borrow_mut()));
        let line=wake_line(&head,&steps,result.as_ref().err(),started.elapsed());
        #[cfg(test)] LAST_WAKE.with(|last|*last.borrow_mut()=line.clone());
        // 測試時不寫，免得把模擬數據混進使用者的真實診斷日誌。
        if !cfg!(test){trace(line);}
        result
    }
    fn attempt(self,paths:&[String])->Result<Target,OpenError>{
        use windows::Win32::{Foundation::WAIT_OBJECT_0,System::Threading::WaitForSingleObject};
        let Some(target)=self.target else{
            wake_step("提醒未登記到帳號程序");
            validate_unread_with(self.unread,client_processes)?;
            return Err(if self.unread.pid==0{OpenError::Unidentified}else{OpenError::Unavailable});
        };
        if target.unread!=self.unread{wake_step("登記的帳號與提醒不一致");return Err(OpenError::Unavailable);}
        // 句柄保留到整個喚起結束，不能因提醒被關閉而提前允許進程號複用。
        let process=check_process(target).inspect_err(|error|wake_step(format!("帳號進程檢查未通過 {error:?}")))?;
        let target=resolve_open_target(target,paths)?;
        let adapter=self.unread.source.adapter().native;
        if !(adapter.ready)(target){wake_step("客戶端尚未就緒");return Err(OpenError::Unavailable);}
        let result=(adapter.open)(target,paths,self.allow_tray);
        if unsafe{WaitForSingleObject(*process,0)==WAIT_OBJECT_0}{wake_step("喚起過程中帳號進程已退出");Err(OpenError::Exited)}else{result}
    }
}
fn validate_unread_with(unread:Unread,discover:impl FnMut(Messenger)->Option<Vec<u32>>)->Result<(),OpenError>{
    if Runtime::default().exited_with(&[unread],discover).contains(&unread){return Err(OpenError::Exited);}
    if unread.pid==0{return Err(OpenError::Unidentified);}Ok(())
}

#[cfg(test)]
pub(crate) fn open_unread_with_paths(unread:Unread,paths:&[String])->Result<(),OpenError>{
    let mut runtime=Runtime::default();
    if unread.pid!=0{runtime.track_unread(unread)?;}
    runtime.request(unread,&Settings::default()).run_with_paths(paths).map(|_|())
}

#[cfg(test)]
pub(crate) fn client_in_foreground(source:Messenger)->bool{unsafe{foreground_main_pid(source,&client_paths(source)).is_some()}}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn client_publishers_are_checked_against_the_actual_executable_name(){
        assert!(publisher_allowed("WXWork.exe","Tencent"));
        assert!(publisher_allowed("Feishu.exe","Beijing Feishu Technology Co., Ltd."));
        assert!(publisher_allowed("DingTalk.exe","Alibaba Group."));
        assert!(!publisher_allowed("Feishu.exe","Tencent"));
        assert!(!publisher_allowed("DingTalk.exe","Unknown Publisher"));
        assert!(!publisher_allowed("WXWorkHelper.exe","Tencent"));
    }
    use windows::{core::w,Win32::Foundation::{LRESULT,WPARAM}};
    #[test]
    fn taskbar_sources_follow_settings_and_cannot_supply_another_client_identity(){
        use std::{cell::Cell,rc::Rc};
        struct Source{dropped:Rc<Cell<bool>>,taken:Rc<Cell<usize>>}
        impl Drop for Source{fn drop(&mut self){self.dropped.set(true);}}
        impl TaskbarSource for Source{
            fn registered(&self)->bool{true}
            fn take(&self)->Vec<TaskbarNotice>{self.taken.set(self.taken.get()+1);vec![TaskbarNotice{pid:100,window:11,started:None},TaskbarNotice{pid:0,window:12,started:None}]}
            fn forget(&self,_:u32){}
        }
        let dropped=Rc::new(Cell::new(false));let taken=Rc::new(Cell::new(0));
        let mut runtime=Runtime{processes:HashMap::new(),targets:HashMap::new(),taskbar:vec![TaskbarEntry{
            client:Messenger::Wechat,source:Some(Box::new(Source{dropped:dropped.clone(),taken:taken.clone()})),retry:None,status:String::new(),
        }]};
        let mut settings=Settings::default();settings.notifications=true;settings.wechat_notifications=true;
        let ctx=eframe::egui::Context::default();let events=runtime.poll_taskbar(&ctx,&settings);
        let messages=events.into_iter().filter_map(|event|if let TaskbarEvent::Message(unread,_)=event{Some(unread)}else{None}).collect::<Vec<_>>();
        assert_eq!(messages,[Unread{source:Messenger::Wechat,pid:100,window:11}]);
        settings.notifications=false;let events=runtime.poll_taskbar(&ctx,&settings);
        assert!(dropped.get());assert_eq!(taken.get(),1,"關閉總開關後不能繼續取訊息");
        assert!(events.iter().any(|event|matches!(event,TaskbarEvent::Status(Messenger::Wechat,status) if status.contains("總開關已關閉"))));
        settings.wechat_notifications=false;
        assert!(runtime.poll_taskbar(&ctx,&settings).iter().any(|event|matches!(event,TaskbarEvent::Status(Messenger::Wechat,status) if status.is_empty())));
    }
    #[test]
    fn qq_foreground_children_resolve_to_the_bound_account_without_using_the_remaining_account(){
        let mut runtime=Runtime::default();
        for pid in [100,200,300]{let unread=Unread{source:Messenger::Qq,pid,window:pid as isize};runtime.targets.insert(unread,Target{unread,started:1});}
        let accounts=runtime.targets.values().copied().collect::<Vec<_>>();
        let parents=HashMap::from([(101,100),(201,200),(301,300),(100,10),(200,10),(300,10)]);
        for (child,account) in [(101,100),(201,200),(301,300)]{assert_eq!(runtime.qq_account_pid(child,&accounts,&parents),Some(account));}
        assert_eq!(runtime.qq_account_pid(10,&accounts,&parents),None,"共同的管理程序不能猜成某個帳號");
        let saved=runtime.targets.clone();runtime.targets.retain(|unread,_|unread.pid==100);
        assert_eq!(runtime.qq_account_pid(10,&accounts,&parents),None,"只有一個帳號有島上提醒時，也必須考慮其他正在執行的帳號");runtime.targets=saved;
        runtime.targets.retain(|unread,_|unread.pid!=100);
        assert_eq!(runtime.qq_account_pid(101,&accounts,&parents),None,"原帳號退出後不能轉綁剩餘帳號");
    }
    #[test]
    fn confirming_a_view_only_rearms_that_clients_account(){
        use std::{cell::RefCell,rc::Rc};
        struct Source{client:Messenger,seen:Rc<RefCell<Vec<(Messenger,u32)>>>}
        impl TaskbarSource for Source{
            fn registered(&self)->bool{true}
            fn take(&self)->Vec<TaskbarNotice>{vec![]}
            fn forget(&self,pid:u32){self.seen.borrow_mut().push((self.client,pid));}
        }
        let seen=Rc::new(RefCell::new(vec![]));let mut runtime=Runtime::default();
        for entry in &mut runtime.taskbar{entry.source=Some(Box::new(Source{client:entry.client,seen:seen.clone()}));}
        runtime.viewed_client(Messenger::Wechat,100);runtime.viewed_client(Messenger::DingTalk,0);runtime.viewed_client(Messenger::DingTalk,200);
        assert_eq!(*seen.borrow(),[(Messenger::Wechat,100),(Messenger::DingTalk,200)]);
    }
    #[test]
    fn opening_clients_share_one_worker_slot_until_the_worker_finishes(){
        let (release,wait)=mpsc::channel();let (started,ready)=mpsc::channel();
        let lease=OPEN.acquire().unwrap();let worker=lease.spawn(move||{started.send(()).unwrap();let _=wait.recv();}).unwrap();
        ready.recv_timeout(Duration::from_secs(2)).unwrap();
        for source in Messenger::ALL{
            let result=OpenRequest{unread:Unread::of(source),target:None,allow_tray:true}.start(eframe::egui::Context::default());
            assert!(matches!(result,Err(OpenError::Busy)),"各客戶端必須共用上限");
        }
        release.send(()).unwrap();worker.join().unwrap();assert_eq!(OPEN.active(),0);
    }
    #[test]
    fn cached_targets_keep_client_namespaces_and_reject_mismatched_open_requests(){unsafe{
        use windows::Win32::System::Threading::GetCurrentProcess;
        let window=CreateWindowExW(WS_EX_TOOLWINDOW,w!("STATIC"),w!(""),WS_POPUP,-10000,-10000,10,10,None,None,None,None).unwrap();
        let target=|source|Target{unread:Unread{source,pid:std::process::id(),window:window.0 as isize},started:process_started(GetCurrentProcess()).unwrap()};
        let (wx,qq)=(target(Messenger::Wechat),target(Messenger::Qq));let mut runtime=Runtime::default();
        runtime.track(wx).unwrap();runtime.track(qq).unwrap();
        assert_eq!(runtime.request(wx.unread,&Settings::default()).target,Some(wx));
        assert_eq!(runtime.request(qq.unread,&Settings::default()).target,Some(qq));
        let wrong=OpenRequest{unread:wx.unread,target:Some(qq),allow_tray:true}.run_with_paths(&[]);
        runtime.exited_with(&[wx.unread],|_|None);DestroyWindow(window).unwrap();
        assert_eq!(wrong,Err(OpenError::Unavailable));assert_eq!(runtime.targets.len(),1);assert!(runtime.targets.contains_key(&wx.unread));
    }}
    #[test]
    fn office_registration_keeps_the_verified_process_when_the_original_window_is_destroyed(){unsafe{
        use windows::Win32::System::{LibraryLoader::GetModuleHandleW,Threading::GetCurrentProcess};
        unsafe extern "system" fn proc(h:HWND,m:u32,w:WPARAM,l:LPARAM)->LRESULT{DefWindowProcW(h,m,w,l)}
        let class=w!("QtDaozaiRebuildQWindowIcon");
        let instance=GetModuleHandleW(None).unwrap().into();
        RegisterClassW(&WNDCLASSW{lpfnWndProc:Some(proc),hInstance:instance,lpszClassName:class,..Default::default()});
        let create=||CreateWindowExW(WINDOW_EX_STYLE(0x100),class,w!(""),WINDOW_STYLE(0x860f0000),-10000,-10000,800,600,None,None,instance,None).unwrap();
        let window=create();
        let target=Target{unread:Unread{source:Messenger::DingTalk,pid:std::process::id(),window:window.0 as isize},started:process_started(GetCurrentProcess()).unwrap()};
        DestroyWindow(window).unwrap();
        let window=create();
        let replacement=Target{unread:Unread{window:window.0 as isize,..target.unread},..target};
        let paths=[normalized(&std::env::current_exe().unwrap().to_string_lossy())];
        let mut runtime=Runtime::default();
        assert_eq!(runtime.register_taskbar(target.unread,None),Some(Unread::of(Messenger::DingTalk)),"缺少原閃動的代際證據時，不能把提醒顯示為已確認帳號");
        assert_eq!(runtime.register_taskbar(target.unread,Some(Target{started:target.started+1,..target})),None,"不能把舊進程的提醒交給複用 PID 的進程");
        assert_eq!(runtime.register_taskbar(target.unread,Some(target)),Some(target.unread),"窗口重建後仍可登記原進程代際");
        assert_eq!(runtime.request(target.unread,&Settings::default()).target,Some(target));
        assert_eq!(resolve_open_target(target,&paths),Ok(replacement));
        let ambiguous=create();
        assert_eq!(resolve_open_target(target,&paths),Err(OpenError::Unavailable),"同進程多個候選不能猜替換窗口");
        DestroyWindow(ambiguous).unwrap();
        assert_eq!(runtime.replace_opened(target.unread,Target{started:target.started+1,..replacement}),Err(OpenError::Unavailable));
        runtime.replace_opened(target.unread,replacement).unwrap();
        assert_eq!(runtime.targets.get(&target.unread),None);
        assert_eq!(runtime.targets.get(&replacement.unread),Some(&replacement));
        DestroyWindow(window).unwrap();
    }}
    #[test]
    fn manual_foreground_accepts_only_the_original_office_accounts_rebuilt_main_window(){unsafe{
        use windows::Win32::System::{LibraryLoader::GetModuleHandleW,Threading::GetCurrentProcess};
        unsafe extern "system" fn proc(h:HWND,m:u32,w:WPARAM,l:LPARAM)->LRESULT{DefWindowProcW(h,m,w,l)}
        let instance=GetModuleHandleW(None).unwrap().into();
        let pid=std::process::id();
        let started=process_started(GetCurrentProcess()).unwrap();
        let paths=[normalized(&std::env::current_exe().unwrap().to_string_lossy())];
        for (client,class,title) in [
            (Messenger::Feishu,w!("Chrome_WidgetWin_1"),w!("飛書")),
            (Messenger::DingTalk,w!("StandardFrame_DingTalk"),w!("釘釘")),
            (Messenger::WeCom,w!("WeWorkWindow"),w!("企業微信")),
        ]{
            RegisterClassW(&WNDCLASSW{lpfnWndProc:Some(proc),hInstance:instance,lpszClassName:class,..Default::default()});
            let create=||{
                let window=CreateWindowExW(WINDOW_EX_STYLE(0),class,title,WS_OVERLAPPEDWINDOW,-10000,-10000,800,600,None,None,instance,None).unwrap();
                let _=ShowWindow(window,SW_SHOWNOACTIVATE);
                window
            };
            let old=create();
            let original=Target{unread:Unread{source:client,pid,window:old.0 as isize},started};
            let mut runtime=Runtime::default();
            assert_eq!(runtime.foreground_at(old,client,&paths),None,"沒有原帳號登記時不得認領前台窗口");
            assert_eq!(runtime.register_taskbar(original.unread,Some(original)),Some(original.unread));
            assert_eq!(runtime.foreground_at(old,client,&paths),Some(pid));
            let rebuilt=create();
            assert_eq!(runtime.foreground_at(rebuilt,client,&paths),None,"原窗口仍在時不得把另一個主窗當成重建");
            DestroyWindow(old).unwrap();
            let extra=create();
            assert_eq!(runtime.foreground_at(rebuilt,client,&paths),None,"同進程多個合格候選時保留提醒");
            DestroyWindow(extra).unwrap();
            let popup=CreateWindowExW(WS_EX_TOOLWINDOW,class,title,WS_VISIBLE|WS_POPUP,-10000,-10000,800,600,None,None,instance,None).unwrap();
            assert_eq!(runtime.foreground_at(popup,client,&paths),None,"同進程的輔助窗不能觸發清理");
            DestroyWindow(popup).unwrap();
            let _=ShowWindow(rebuilt,SW_SHOWMINNOACTIVE);
            assert_eq!(runtime.foreground_at(rebuilt,client,&paths),None,"新窗口仍最小化時保留提醒");
            let _=ShowWindow(rebuilt,SW_SHOWNOACTIVATE);
            assert_eq!(runtime.foreground_at(rebuilt,client,&paths),Some(pid),"原帳號唯一的新主窗可確認查看");
            assert_eq!(runtime.request(original.unread,&Settings::default()).target,Some(original),"確認查看不應改寫已登記的原提醒");
            let other=Target{unread:Unread{pid:pid+1,..original.unread},..original};
            runtime.targets.remove(&original.unread);
            runtime.targets.insert(other.unread,other);
            assert_eq!(runtime.foreground_at(rebuilt,client,&paths),None,"前台的新窗口不能清另一個帳號");
            runtime.targets.insert(original.unread,Target{started:started+1,..original});
            assert_eq!(runtime.foreground_at(rebuilt,client,&paths),None,"相同 PID 的新進程不能清舊提醒");
            DestroyWindow(rebuilt).unwrap();
        }
    }}
    fn open_qq_target(target:Target)->Result<(),String>{
        OpenRequest{unread:target.unread,target:Some(target),allow_tray:true}.run().map(|_|()).map_err(|error|error.message(target.unread.source))
    }
    struct ClientProcess(std::process::Child);
    impl ClientProcess {
        fn new()->Self{
            use std::os::windows::process::CommandExt;
            let child=std::process::Command::new("powershell.exe")
                .args(["-NoProfile","-NonInteractive","-WindowStyle","Hidden","-Command","[System.Threading.Thread]::Sleep(60000)"])
                .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
                .creation_flags(0x08000000).spawn().unwrap();
            Self(child)
        }
        fn exit(&mut self){self.0.kill().unwrap();self.0.wait().unwrap();}
    }
    impl Drop for ClientProcess {
        fn drop(&mut self){let _=self.0.kill();let _=self.0.wait();}
    }
    fn qq_target(pid:u32)->Target{Target{unread:crate::app::Unread{source:crate::app::Messenger::Qq,pid,window:pid as isize},started:u64::from(pid)}}
    #[test]
    fn qq_open_regression_unknown_click_after_an_account_exits(){
        let (a,b)=(qq_target(100),qq_target(200));
        let mut history=AccountHistory::new(Messenger::Qq);
        history.observe(10,Some(vec![a,b]));
        history.observe(20,Some(vec![a,b]));
        assert_eq!(history.target(1,15,|_|true),None);
        let unknown=Unread::of(Messenger::Qq);
        let mut runtime=Runtime::default();
        history.observe(30,Some(vec![b]));
        assert!(runtime.exited_with(&[unknown],|_|Some(vec![b.unread.pid])).is_empty());
        let click=runtime.request(unknown,&Settings::default()).start(Default::default());
        assert!(matches!(click,Err(OpenError::Unidentified)),"舊提醒不能啟動喚起剩餘帳號的後台任務");
    }
    #[test]
    fn qq_notifications_bind_only_to_the_stable_account_at_creation(){
        let a=qq_target(100);let mut accounts=AccountHistory::new(Messenger::Qq);
        accounts.observe(10_000_000,Some(vec![a]));accounts.observe(20_000_000,Some(vec![a]));
        assert_eq!(accounts.target(1,15_000_000,|_|true),Some(a));
        assert_eq!(accounts.target(2,9_000_000,|_|true),None,"啟動前歷史通知不能繫結當前帳號");
        assert_eq!(accounts.target(3,21_000_000,|_|true),None,"超出觀察時間的通知不能猜帳號");
        accounts.observe(60_000_000,Some(vec![a]));
        assert_eq!(accounts.target(4,30_000_000,|_|true),None,"長時間暫停後不能繫結遺漏期間的通知");
    }
    #[test]
    fn qq_notifications_never_rebind_after_other_accounts_exit(){
        let (a,b,c)=(qq_target(100),qq_target(200),qq_target(300));let mut accounts=AccountHistory::new(Messenger::Qq);
        accounts.observe(10_000_000,Some(vec![a,b,c]));accounts.observe(20_000_000,Some(vec![b,c]));
        assert_eq!(accounts.target(1,15_000_000,|_|true),None);
        accounts.observe(30_000_000,Some(vec![c]));accounts.observe(40_000_000,Some(vec![c]));
        assert_eq!(accounts.target(1,15_000_000,|_|panic!("舊通知不能重新識別")),None);
        assert_eq!(accounts.target(2,15_000_000,|_|true),None,"延遲送達的舊通知不能繫結剩餘帳號");
        assert_eq!(accounts.target(3,35_000_000,|_|true),Some(c),"剩餘帳號的新通知可以正常繫結");
    }
    #[test]
    fn qq_notification_updates_never_rebind_after_account_changes(){
        let (a,b)=(qq_target(100),qq_target(200));let mut accounts=AccountHistory::new(Messenger::Qq);
        accounts.observe(10_000_000,Some(vec![a]));accounts.observe(20_000_000,Some(vec![a]));
        assert_eq!(accounts.target(1,15_000_000,|_|true),Some(a));
        accounts.observe(30_000_000,Some(vec![b]));accounts.observe(40_000_000,Some(vec![b]));
        assert_eq!(accounts.target(1,15_000_000,|_|panic!("正文更新不能改綁")),None,"帳號變化後保留未識別訊息，不能錯開或丟棄新帳號的更新");
        assert_eq!(accounts.target(1,35_000_000,|_|true),Some(b),"通知編號複用時必須重新匹配建立時間");
        accounts.retain(|_,_|false);assert!(accounts.targets.is_empty());
        accounts.observe(50_000_000,Some(vec![Target{started:b.started+1,..b}]));
        assert_eq!(accounts.target(2,45_000_000,|_|true),None,"程序號和視窗號相同也不能忽略程序重啟");
    }
    #[test]
    fn qq_failed_observations_and_ambiguous_tray_hosts_remain_unidentified(){
        let a=qq_target(100);let mut accounts=AccountHistory::new(Messenger::Qq);
        accounts.observe(10_000_000,Some(vec![a]));accounts.observe(20_000_000,None);
        assert_eq!(accounts.target(1,15_000_000,|_|true),None);
        accounts.observe(30_000_000,Some(vec![a]));accounts.observe(40_000_000,Some(vec![a]));
        assert_eq!(accounts.target(2,35_000_000,|_|false),None,"同宿主有多個圖示時不能繫結");
        assert_eq!(accounts.target(2,35_000_000,|_|panic!("圖示減少後也不能改綁舊通知")),None);
    }
    #[test]
    fn qq_account_observation_tracks_the_original_native_window(){unsafe{
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        unsafe extern "system" fn proc(h:HWND,m:u32,w:WPARAM,l:LPARAM)->LRESULT{DefWindowProcW(h,m,w,l)}
        let instance=GetModuleHandleW(None).unwrap().into();
        RegisterClassW(&WNDCLASSW{lpfnWndProc:Some(proc),hInstance:instance,lpszClassName:w!("Chrome_WidgetWin_1"),..Default::default()});
        let window=CreateWindowExW(WINDOW_EX_STYLE(0),w!("Chrome_WidgetWin_1"),w!(""),WS_OVERLAPPEDWINDOW,0,0,400,500,None,None,instance,None).unwrap();
        let paths=[normalized(&std::env::current_exe().unwrap().to_string_lossy())];
        let found=crate::messenger::qq::accounts(&paths).unwrap();let mut monitor=Runtime::default();
        let target=found[0];let tracked=monitor.track(target).is_ok();
        let reused=monitor.track(Target{started:target.started+1,..target}).is_ok();
        let restarted=open_qq_target(Target{started:target.started+1,..target});
        let scan_at=Instant::now();let icons=host_icons(&[(target.unread.pid,window)],target.unread.pid);
        println!("未註冊圖示的完整系統匣掃描耗時：{:?}",scan_at.elapsed());
        DestroyWindow(window).unwrap();
        assert_eq!(found.len(),1);assert_eq!(target.unread.window,window.0 as isize);assert!(tracked);
        assert!(!reused,"不同代程序不能沿用舊通知");assert!(!monitor.track(target).is_ok(),"原視窗銷燬後不能改選其他視窗");
        assert_eq!(restarted,Err("QQ 帳號已退出".into()));assert!(icons.is_empty());
        monitor.exited(&[]);assert!(monitor.targets.is_empty());
    }}
    #[test]
    fn unidentified_qq_reminders_never_choose_remaining_accounts(){
        let unread=crate::app::Unread::of(crate::app::Messenger::Qq);
        for pids in [Some(vec![100,200,300]),Some(vec![200,300]),Some(vec![300]),None]{
            assert_eq!(validate_unread_with(unread,|_|pids.clone()).map_err(|error|error.message(unread.source)),Err("暫時無法確認此 QQ 訊息的帳號".into()));
        }
        assert_eq!(validate_unread_with(unread,|_|Some(Vec::new())).map_err(|error|error.message(unread.source)),Err("QQ 帳號已退出".into()));
    }
    #[test]
    fn exited_accounts_report_their_status_even_when_other_accounts_remain(){
        use crate::app::{Messenger,Unread};
        for source in Messenger::ALL{
            let mut clients=[ClientProcess::new(),ClientProcess::new(),ClientProcess::new()];
            let unread=Unread{source,pid:clients[0].0.id(),window:0};
            assert!(validate_unread_with(unread,|_|panic!("不能改選其他帳號")).is_ok());
            clients[0].exit();
            assert_eq!(validate_unread_with(unread,|_|panic!("不能改選其他帳號")),Err(OpenError::Exited));
            for client in &mut clients[1..]{assert!(client.0.try_wait().unwrap().is_none());}
        }
    }
    #[test]
    fn qq_unreads_never_show_hidden_helpers(){unsafe{
        use crate::app::{Messenger,Unread};
        let window=CreateWindowExW(WS_EX_TOOLWINDOW,w!("STATIC"),w!(""),WS_POPUP,0,0,10,10,None,None,None,None).unwrap();
        let paths=[normalized(&std::env::current_exe().unwrap().to_string_lossy())];
        let results=[0,std::process::id()].map(|pid|open_unread_with_paths(Unread{source:Messenger::Qq,pid,window:window.0 as isize},&paths));
        let visible=IsWindowVisible(window).as_bool();DestroyWindow(window).unwrap();
        assert!(!visible && results.iter().all(Result::is_err),"不能顯示輔助視窗或改為喚起其他 QQ");
    }}
    #[test]
    fn wake_attempts_record_the_target_route_and_result_without_titles(){unsafe{
        use crate::app::{Messenger,Unread};
        let window=CreateWindowExW(WS_EX_TOOLWINDOW,w!("STATIC"),w!("聊天物件的名字"),WS_POPUP,0,0,10,10,None,None,None,None).unwrap();
        let paths=[normalized(&std::env::current_exe().unwrap().to_string_lossy())];
        let pid=std::process::id();
        assert!(open_unread_with_paths(Unread{source:Messenger::Qq,pid,window:window.0 as isize},&paths).is_err());
        let line=LAST_WAKE.with(|last|last.borrow().clone());
        DestroyWindow(window).unwrap();
        assert!(line.starts_with(&format!("喚起 QQ 帳號 {pid} · 提醒視窗 {:#x} Static 隱藏",window.0 as usize)),"記錄目標帳號與提醒視窗當時的狀態：{line}");
        assert!(line.contains("→ 客戶端尚未就緒 →"),"記錄停在哪一步：{line}");
        assert!(line.contains("結果:失敗 Unavailable") && line.contains("用時"),"記錄結果與耗時：{line}");
        assert!(!line.contains("聊天物件的名字"),"不能記錄視窗標題");
        assert_eq!(WAKE_STEPS.with(|steps|steps.borrow().len()),0,"每次喚起結束後清空步驟，不串到下一次");
    }}
    #[test]
    fn wake_line_reports_success_and_steps_in_order(){
        let ok=wake_line("喚起 釘釘 帳號 7",&["切前台 0x1 StandardFrame_DingTalk 進程 7".into(),"系統已接受".into()],None,Duration::from_millis(12));
        assert_eq!(ok,"喚起 釘釘 帳號 7 → 切前台 0x1 StandardFrame_DingTalk 進程 7 → 系統已接受 → 結果:已發出打開請求 用時12ms");
        let failed=wake_line("喚起 微信 帳號 9",&[],Some(&OpenError::Failed("未能喚起微信（T3）".into())),Duration::ZERO);
        assert!(failed.contains("→ - →") && failed.contains("失敗 Failed(\"未能喚起微信（T3）\")"));
    }
    #[test]
    fn exited_messenger_accounts_are_removed_individually(){
        use crate::app::{Messenger,Unread};
        for source in Messenger::ALL{
            let mut clients=[ClientProcess::new(),ClientProcess::new(),ClientProcess::new()];
            let mut unreads=clients.iter().map(|client|Unread{source,pid:client.0.id(),window:0}).collect::<Vec<_>>();
            let mut monitor=Runtime::default();
            assert!(monitor.exited_with(&unreads,|_|panic!("已知帳號只檢查對應程序")).is_empty());
            for index in [1,0,2]{
                let closed=Unread{source,pid:clients[index].0.id(),window:0};clients[index].exit();
                assert_eq!(monitor.exited_with(&unreads,|_|panic!("不能依靠其他帳號是否存在來清理")),[closed]);
                unreads.retain(|unread|*unread!=closed);
                assert!(monitor.exited_with(&unreads,|_|None).is_empty());
            }
            assert!(monitor.processes.is_empty(),"未讀清空後應釋放程序控制程式碼");
        }
    }
    #[test]
    fn reminders_clear_when_the_client_exits_before_the_first_check(){
        use crate::app::{Messenger,Unread};
        for source in Messenger::ALL{
            let mut client=ClientProcess::new();
            let unread=Unread{source,pid:client.0.id(),window:0};
            client.exit();drop(client);
            let mut monitor=Runtime::default();
            assert_eq!(monitor.exited(&[unread]),[unread]);
        }
    }
    #[test]
    fn unknown_messenger_accounts_clear_only_after_the_whole_client_exits(){
        use crate::app::{Messenger,Unread};
        for source in Messenger::ALL{
            let unread=Unread::of(source);let mut monitor=Runtime::default();
            assert!(monitor.exited_with(&[unread],|_|Some(vec![100,200])).is_empty());
            assert!(monitor.exited_with(&[unread],|_|Some(vec![200])).is_empty());
            assert!(monitor.exited_with(&[unread],|_|None).is_empty(),"無法確認客戶端狀態時保留提醒");
            assert_eq!(monitor.exited_with(&[unread],|_|Some(Vec::new())),[unread]);
        }
    }
    #[test]
    fn messenger_reminders_survive_hidden_or_destroyed_windows(){unsafe{
        use crate::app::{Messenger,Unread};
        let mut monitor=Runtime::default();
        for source in Messenger::ALL{
            let window=CreateWindowExW(WS_EX_TOOLWINDOW,w!("STATIC"),w!(""),WS_POPUP,-10000,-10000,10,10,None,None,None,None).unwrap();
            let unread=Unread{source,pid:std::process::id(),window:window.0 as isize};
            let hidden=monitor.exited(&[unread]);DestroyWindow(window).unwrap();
            assert!(hidden.is_empty(),"縮到系統匣時應保留未讀");
            assert!(monitor.exited(&[unread]).is_empty(),"視窗關閉、程序仍在時應保留未讀");
        }
    }}

}

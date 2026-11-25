//! 辦公客戶端只監聽任務欄事件；喚起時僅處理已確認的原窗口。
use super::{Messenger,OpenError,Target,Unread};
use super::windows::*;
use windows::{core::Owned,Win32::{Foundation::{BOOL,HWND,LPARAM},System::Threading::{OpenProcess,PROCESS_QUERY_LIMITED_INFORMATION},UI::WindowsAndMessaging::*}};

pub(crate) const WECOM:NativeAdapter=NativeAdapter{
    paths:||paths(Messenger::WeCom),main_frame:|class,style,extended|main_frame(Messenger::WeCom,class,style,extended),
    accounts:None,ready,strict_window:true,open,taskbar:Some(|ctx|start(Messenger::WeCom,ctx)),
};
pub(crate) const FEISHU:NativeAdapter=NativeAdapter{
    paths:||paths(Messenger::Feishu),main_frame:|class,style,extended|main_frame(Messenger::Feishu,class,style,extended),
    accounts:None,ready,strict_window:true,open,taskbar:Some(|ctx|start(Messenger::Feishu,ctx)),
};
pub(crate) const DINGTALK:NativeAdapter=NativeAdapter{
    paths:||paths(Messenger::DingTalk),main_frame:|class,style,extended|main_frame(Messenger::DingTalk,class,style,extended),
    accounts:None,ready,strict_window:true,open,taskbar:Some(|ctx|start(Messenger::DingTalk,ctx)),
};
fn paths(client:Messenger)->Vec<String>{client.adapter().process_names.iter().map(|name|format!("*{}",name.to_ascii_lowercase())).collect()}
fn start(client:Messenger,ctx:eframe::egui::Context)->Result<Box<dyn TaskbarSource>,String>{
    Ok(Box::new(crate::wechat::Listener::for_client(client,ctx)?))
}
fn main_frame(client:Messenger,class:&str,style:u32,extended:u32)->bool{
    if style&WS_CHILD.0!=0 || extended&(WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE).0!=0{return false;}
    match client{
        Messenger::WeCom=>class=="WeWorkWindow",
        Messenger::Feishu=>class=="Chrome_WidgetWin_1" && frame_style(class,style,extended),
        Messenger::DingTalk=>(class=="StandardFrame_DingTalk" || class.starts_with("Qt") && class.ends_with("QWindowIcon")) && frame_style(class,style,extended),
        _=>false,
    }
}
pub(crate) unsafe fn main_window(window:HWND,client:Messenger)->bool{
    if !IsWindow(window).as_bool() || GetWindow(window,GW_OWNER).is_ok(){return false;}
    let class=class_name(window);
    let style=GetWindowLongW(window,GWL_STYLE)as u32;
    if !client_main_frame(client,&class,style,GetWindowLongW(window,GWL_EXSTYLE)as u32){return false;}
    if client==Messenger::DingTalk && class!=DINGTALK_FRAME{
        let pid=window_pid(window);
        if has_dingtalk_frame(pid) || !IsWindowVisible(window).as_bool()
            && has_preferred_dingtalk_qt(pid,dingtalk_frameless_main_style(style)){return false;}
    }
    if client==Messenger::Feishu && !feishu_title(&window_title(window)) && feishu_main_frame(window_pid(window)).is_some(){return false;}
    window_sized(window)
}
/// 最小化時按恢復尺寸檢查，排除任何狀態下不足 300×300 的小彈窗。
unsafe fn window_sized(window:HWND)->bool{
    use windows::Win32::Foundation::RECT;
    let mut rect=RECT::default();
    if IsIconic(window).as_bool(){
        let mut placement=WINDOWPLACEMENT{length:std::mem::size_of::<WINDOWPLACEMENT>() as u32,..Default::default()};
        if GetWindowPlacement(window,&mut placement).is_err(){return false;}
        rect=placement.rcNormalPosition;
    }else if GetWindowRect(window,&mut rect).is_err(){return false;}
    rect.right-rect.left>=300 && rect.bottom-rect.top>=300
}
const DINGTALK_FRAME:&str="StandardFrame_DingTalk";
/// 釘釘 9.0.5 的 Qt 主窗口沒有系統標題欄，但有完整的窗口控制樣式。同進程的隱藏對話框帶標題欄。
fn dingtalk_frameless_main_style(style:u32)->bool{
    let frame=(WS_POPUP|WS_SYSMENU|WS_THICKFRAME|WS_MINIMIZEBOX|WS_MAXIMIZEBOX).0;
    style&(frame|WS_CAPTION.0)==frame
}
/// 可見窗口優先。全隱藏時，仍要用閃動窗口和同進程唯一候選同時確認主窗口。
unsafe fn has_preferred_dingtalk_qt(pid:u32,visible_only:bool)->bool{
    struct Search{pid:u32,visible_only:bool,found:bool}
    unsafe extern "system" fn visit(window:HWND,data:LPARAM)->BOOL{
        let search=&mut *(data.0 as *mut Search);
        if window_pid(window)==search.pid && GetWindow(window,GW_OWNER).is_err(){
            let class=class_name(window);
            let style=GetWindowLongW(window,GWL_STYLE)as u32;
            if class.starts_with("Qt") && class.ends_with("QWindowIcon")
                && (IsWindowVisible(window).as_bool() || !search.visible_only && dingtalk_frameless_main_style(style))
                && main_frame(Messenger::DingTalk,&class,style,GetWindowLongW(window,GWL_EXSTYLE)as u32)
                && window_sized(window){search.found=true;return false.into();}
        }
        true.into()
    }
    let mut search=Search{pid,visible_only,found:false};
    let _=EnumWindows(Some(visit),LPARAM((&mut search as *mut Search)as isize));
    search.found
}
/// 新版釘釘的主窗口是 StandardFrame_DingTalk；同進程裡的 Qt 窗口只是對話框（常為隱藏狀態），
/// 不能算作第二個主窗口。主窗口本身隱藏到托盤時仍然認它。只有舊版沒有該窗口時才按 Qt 主窗口識別。
unsafe fn has_dingtalk_frame(pid:u32)->bool{
    struct Search{pid:u32,found:bool}
    unsafe extern "system" fn visit(window:HWND,data:LPARAM)->BOOL{
        let search=&mut *(data.0 as *mut Search);
        if window_pid(window)==search.pid && class_name(window)==DINGTALK_FRAME && GetWindow(window,GW_OWNER).is_err()
            && main_frame(Messenger::DingTalk,DINGTALK_FRAME,GetWindowLongW(window,GWL_STYLE)as u32,GetWindowLongW(window,GWL_EXSTYLE)as u32)
            && window_sized(window){
            search.found=true;return false.into();
        }
        true.into()
    }
    let mut search=Search{pid,found:false};
    let _=EnumWindows(Some(visit),LPARAM((&mut search as *mut Search)as isize));
    search.found
}
/// 閃動沒能對應到帳號時寫進診斷日誌：同進程所有無所屬窗口的頂層窗口，
/// 列出類名、樣式、尺寸、可見狀態和是否算主窗口，不讀標題。最多列 12 個。
pub(crate) fn describe_process_windows(client:Messenger,pid:u32)->String{unsafe{
    use windows::Win32::Foundation::RECT;
    struct Search{pid:u32,windows:Vec<HWND>}
    unsafe extern "system" fn visit(window:HWND,data:LPARAM)->BOOL{
        let search=&mut *(data.0 as *mut Search);
        if window_pid(window)==search.pid && GetWindow(window,GW_OWNER).is_err(){search.windows.push(window);}
        true.into()
    }
    let mut search=Search{pid,windows:Vec::new()};
    let _=EnumWindows(Some(visit),LPARAM((&mut search as *mut Search)as isize));
    let total=search.windows.len();
    let mut lines=search.windows.iter().take(12).map(|&window|{
        let mut rect=RECT::default();let _=GetWindowRect(window,&mut rect);
        format!("{:#x} {} 樣式 {:#010x}/{:#x} {}×{}{}{}{}",window.0 as usize,class_name(window),
            GetWindowLongW(window,GWL_STYLE)as u32,GetWindowLongW(window,GWL_EXSTYLE)as u32,rect.right-rect.left,rect.bottom-rect.top,
            if IsWindowVisible(window).as_bool(){""}else{" 隱藏"},if IsIconic(window).as_bool(){" 最小化"}else{""},
            if main_window(window,client){" 主窗口"}else{""})
    }).collect::<Vec<_>>();
    if total>12{lines.push(format!("另有 {} 個",total-12));}
    format!("進程 {pid} 頂層窗口 {total} 個：{}",lines.join("；"))
}}
/// 飛書主窗口的標題固定為應用名（切換消息、雲文檔等頁面不變，關到托盤後仍保留）；
/// 「在獨立窗口打開」的會話、文檔窗口與主窗口同進程、同類名、同樣式，只有標題是會話或文檔名。
const FEISHU_TITLES:[&str;3]=["飛書","Feishu","Lark"];
const FEISHU_FRAME:&str="Chrome_WidgetWin_1";
fn feishu_title(title:&str)->bool{FEISHU_TITLES.iter().any(|name|name.eq_ignore_ascii_case(title.trim()))}
fn window_title(window:HWND)->String{
    let mut text=[0u16;256];
    let n=unsafe{GetWindowTextW(window,&mut text)}.max(0) as usize;
    String::from_utf16_lossy(&text[..n])
}
unsafe fn feishu_frame(window:HWND)->bool{
    class_name(window)==FEISHU_FRAME && GetWindow(window,GW_OWNER).is_err()
        && main_frame(Messenger::Feishu,FEISHU_FRAME,GetWindowLongW(window,GWL_STYLE)as u32,GetWindowLongW(window,GWL_EXSTYLE)as u32)
}
/// 同進程裡唯一一個以應用名為標題的飛書主窗口；沒有或不止一個（比如有會話恰好叫「飛書」）時不猜。
unsafe fn feishu_main_frame(pid:u32)->Option<HWND>{
    struct Search{pid:u32,found:Vec<HWND>}
    unsafe extern "system" fn visit(window:HWND,data:LPARAM)->BOOL{
        let search=&mut *(data.0 as *mut Search);
        if window_pid(window)==search.pid && feishu_frame(window) && feishu_title(&window_title(window))
            && window_sized(window){search.found.push(window);}
        true.into()
    }
    let mut search=Search{pid,found:Vec::new()};
    let _=EnumWindows(Some(visit),LPARAM((&mut search as *mut Search)as isize));
    match search.found.as_slice(){[one]=>Some(*one),_=>None}
}
/// 飛書獨立會話 / 文檔窗口閃動時，歸到同進程的主窗口：同一進程就是同一個飛書實例，不會串到其他帳號。
unsafe fn feishu_owner(window:HWND)->Option<HWND>{
    if !feishu_frame(window) || feishu_title(&window_title(window)){return None;}
    feishu_main_frame(window_pid(window))
}
fn candidates(client:Messenger,pid:u32)->Option<Vec<Target>>{unsafe{
    let process=Owned::new(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,false,pid).ok()?);
    struct Search{client:Messenger,pid:u32,windows:Vec<Unread>}
    unsafe extern "system" fn visit(window:HWND,data:LPARAM)->BOOL{
        let search=&mut *(data.0 as *mut Search);
        if window_pid(window)==search.pid && main_window(window,search.client){
            search.windows.push(Unread{source:search.client,pid:search.pid,window:window.0 as isize});
        }
        true.into()
    }
    let mut search=Search{client,pid,windows:Vec::new()};
    EnumWindows(Some(visit),LPARAM((&mut search as *mut Search)as isize)).ok()?;
    snapshot_account(pid,search.windows,|_|process_started(*process))
}}
fn snapshot_account(pid:u32,windows:impl IntoIterator<Item=Unread>,started_at:impl FnOnce(u32)->Option<u64>)->Option<Vec<Target>>{
    let started=started_at(pid)?;
    Some(windows.into_iter().filter(|unread|unread.pid==pid).map(|unread|Target{unread,started}).collect())
}
/// 只接受原事件窗口或其明確的所屬主窗口，同進程的其他窗口不能借用。
fn select_target(client:Messenger,window:isize,accounts:&[Target])->Option<Target>{
    let target=accounts.iter().find(|target|target.unread.source==client && target.unread.window==window)?;
    (accounts.iter().filter(|other|other.unread.source==client && other.unread.pid==target.unread.pid).count()==1).then_some(*target)
}
pub(crate) fn flash_account(client:Messenger,window:HWND,paths:&[String])->Option<Option<Target>>{unsafe{
    let source_pid=window_pid(window);
    if !pid_matches(source_pid,paths){return None;}
    let mut root=GetAncestor(window,GA_ROOTOWNER);
    if client==Messenger::Feishu {if let Some(main)=feishu_owner(root){root=main;}}
    let pid=window_pid(root);
    if pid!=source_pid && !pid_matches(pid,paths){return Some(None);}
    let Some(accounts)=candidates(client,pid)else{return Some(None);};
    if client==Messenger::DingTalk && root==window && accounts.is_empty() && dingtalk_login_window(window){return None;}
    Some(select_target(client,root.0 as isize,&accounts))
}}
#[cfg(test)]
pub(crate) fn flash_target(client:Messenger,window:HWND,paths:&[String])->Option<Unread>{
    flash_account(client,window,paths).map(|target|target.map_or_else(||Unread::of(client),|t|t.unread))
}
/// 釘釘 9.0.5 掃碼前會閃動豎版登錄窗口，登錄成功後在同一進程中另建消息主窗口。
unsafe fn dingtalk_login_window(window:HWND)->bool{
    let class=class_name(window);
    let style=GetWindowLongW(window,GWL_STYLE) as u32;
    let fixed_popup=(WS_POPUP|WS_CLIPSIBLINGS|WS_CLIPCHILDREN|WS_MINIMIZEBOX).0;
    if !class.starts_with("Qt") || !class.ends_with("QWindowIcon") || GetWindowLongW(window,GWL_EXSTYLE)!=0
        || style & !(WS_VISIBLE|WS_MINIMIZE).0 != fixed_popup{return false;}
    let mut placement=WINDOWPLACEMENT{length:std::mem::size_of::<WINDOWPLACEMENT>() as u32,..Default::default()};
    if GetWindowPlacement(window,&mut placement).is_err(){return false;}
    let rect=placement.rcNormalPosition;
    let (width,height)=(i64::from(rect.right-rect.left),i64::from(rect.bottom-rect.top));
    // 實測寬高比 600:798，允許不同顯示縮放帶來的兩個像素取整差。
    width>0 && (height*600-width*798).abs()<=1200
}
fn ready(target:Target)->bool{
    ready_with_paths(target,&client_paths(target.unread.source))
}
fn ready_with_paths(target:Target,paths:&[String])->bool{
    let pid=target.unread.pid;
    (unsafe{pid_matches(pid,paths)}) && matches!(candidates(target.unread.source,pid).as_deref(),Some([one]) if *one==target)
}
pub(super) fn matches_foreground(target:Target,window:HWND,paths:&[String])->bool{
    let window=window.0 as isize;
    if target.unread.window==window{return ready_with_paths(target,paths);}
    // 手動打開客戶端時也只確認原進程中唯一合格的前台新主窗。
    rebuild_target(target,paths).is_some_and(|rebuilt|rebuilt.unread.window==window)
}
/// 重建時沒有新閃動可校驗，必須有該客戶端明確的主窗口特徵。
pub(crate) fn rebuild_target(target:Target,paths:&[String])->Option<Target>{unsafe{
    let pid=target.unread.pid;
    let window=HWND(target.unread.window as *mut _);
    if window_pid(window)==pid || !pid_matches(pid,paths){return None;}
    let accounts=candidates(target.unread.source,pid)?;
    let [one]=accounts.as_slice() else{return None;};
    if one.started!=target.started{return None;}
    let replacement=HWND(one.unread.window as *mut _);
    let trusted=match target.unread.source{
        Messenger::Feishu=>feishu_title(&window_title(replacement)),
        Messenger::DingTalk=>class_name(replacement)==DINGTALK_FRAME
            || dingtalk_frameless_main_style(GetWindowLongW(replacement,GWL_STYLE) as u32),
        Messenger::WeCom=>true,
        _=>false,
    };
    trusted.then_some(*one)
}}
fn open(target:Target,paths:&[String],_:bool)->Result<Target,OpenError>{unsafe{
    let unread=target.unread;
    let window=HWND(unread.window as *mut _);
    if unread.pid==0 || window_pid(window)!=unread.pid {wake_step("提醒視窗已不屬於該帳號");return Err(OpenError::Unavailable);}
    if !main_window(window,unread.source){wake_step(format!("提醒視窗不是主視窗（{}）",describe_window(window)));return Err(OpenError::Unavailable);}
    if !matches(window,paths){wake_step("提醒視窗所屬程式不在已知安裝路徑中");return Err(OpenError::Unavailable);}
    // 不執行協議、托盤回調或鼠標鍵盤模擬，只恢復該消息保存的主窗口。
    wake_step(format!("切前台 {}",describe_window(window)));
    let show_queued=match wake_route(unread.source,IsWindowVisible(window).as_bool(),IsIconic(window).as_bool()) {
        WakeRoute::Restore=>Some(ShowWindowAsync(window,SW_RESTORE).as_bool()),
        WakeRoute::Show=>Some(ShowWindowAsync(window,SW_SHOW).as_bool()),
        WakeRoute::Relaunch=>return relaunch(target,paths),
        WakeRoute::Focus=>None,
    };
    finish_open(show_queued,||SetForegroundWindow(window).as_bool()).map(|_|target)
}}

fn finish_open(show_queued:Option<bool>,focus:impl FnOnce()->bool)->Result<(),OpenError>{
    if show_queued==Some(false){wake_step("系統未接受窗口恢復請求");return Err(OpenError::Unavailable);}
    if focus(){wake_step("系統已接受");return Ok(());}
    if show_queued==Some(true){
        wake_step("恢復請求已提交，等待原窗口到前台");
        Ok(())
    }else{
        wake_step("系統拒絕切前台");
        Err(OpenError::Unavailable)
    }
}

#[derive(Debug,PartialEq,Eq)]
enum WakeRoute {Focus,Restore,Show,Relaunch}
/// 怎樣把主窗口帶到前台。飛書關到托盤後窗口被隱藏，若由鬼島直接 `ShowWindow`，飛書不會恢復渲染，
/// 頁面停在加載骨架、點擊無響應（#181，本機飛書 8.6 復現）；改為再啟動一次飛書主程序，由飛書自己顯示原窗口。
/// 最小化的窗口照常還原（實測正常）；其他客戶端未驗證，保持原做法。
fn wake_route(source:Messenger,visible:bool,iconic:bool)->WakeRoute {
    if !visible && source==Messenger::Feishu {WakeRoute::Relaunch}
    else if iconic {WakeRoute::Restore}
    else if visible {WakeRoute::Focus}
    else {WakeRoute::Show}
}

/// 啟動提醒窗口所屬進程自己的程序（單實例：不會多開，飛書把已有主窗口顯示出來），再等窗口出現。
unsafe fn relaunch(target:Target,paths:&[String])->Result<Target,OpenError>{
    let pid=target.unread.pid;
    let Some(path)=crate::smtc::player_process::image_path(pid) else{wake_step("無法讀取客戶端程序路徑");return Err(OpenError::Unavailable);};
    if !path_allowed(&normalized(&path),paths) {wake_step("客戶端程序不在已知安裝路徑中");return Err(OpenError::Unavailable);}
    super::trust::launch_feishu(std::path::Path::new(&path),pid).map_err(|reason|{
        wake_step(reason);OpenError::Failed(reason.into())
    })?;
    wake_step("窗口在托盤中，由客戶端自己顯示主窗口");
    let until=std::time::Instant::now()+std::time::Duration::from_secs(3);
    while std::time::Instant::now()<until {
        if let Some(shown)=visible_relaunched_target(target,paths){
            if shown==target{wake_step("客戶端已顯示原主窗口");}
            else{wake_step(format!("客戶端已顯示同一帳號的新主窗口 · {:#x} → {:#x}",target.unread.window,shown.unread.window));}
            let window=HWND(shown.unread.window as *mut _);
            return finish_open(Some(true),||SetForegroundWindow(window).as_bool()).map(|_|shown);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    wake_step("客戶端 3 秒內沒有顯示主視窗");Err(OpenError::Unavailable)
}

unsafe fn visible_relaunched_target(original:Target,paths:&[String])->Option<Target>{
    let window=HWND(original.unread.window as *mut _);
    let target=if window_pid(window)==original.unread.pid {original}else{rebuild_target(original,paths)?};
    let window=HWND(target.unread.window as *mut _);
    (window_pid(window)==target.unread.pid && IsWindowVisible(window).as_bool()
        && !IsIconic(window).as_bool() && main_window(window,target.unread.source)).then_some(target)
}

#[cfg(test)]
mod tests{
    use super::*;
    use windows::{core::w,Win32::{Foundation::{LRESULT,WPARAM},System::{LibraryLoader::GetModuleHandleW,Threading::GetCurrentProcess}}};
    fn target(client:Messenger,pid:u32,window:isize)->Target{Target{unread:Unread{source:client,pid,window},started:1}}
    #[test]
    fn three_or_more_instances_never_rebind_to_another_or_the_last_instance(){
        for client in Messenger::OFFICE{
            let (a,b,c)=(target(client,100,11),target(client,200,22),target(client,300,33));
            assert_eq!(select_target(client,11,&[a,b,c]),Some(a));
            assert_eq!(select_target(client,22,&[a,b,c]),Some(b));
            assert_eq!(select_target(client,11,&[b,c]),None);
            assert_eq!(select_target(client,11,&[c]),None);
            assert_eq!(select_target(client,11,&[a,Target{unread:Unread{window:44,..a.unread},..a}]),None,"同進程多窗口不能當作唯一帳號");
        }
    }
    #[test]
    fn another_instance_exit_does_not_invalidate_the_original_candidate(){
        for client in [Messenger::DingTalk,Messenger::Feishu]{
            let original=target(client,100,11);
            let exited=target(client,200,22);
            let mut queried=Vec::new();
            let snapshot=snapshot_account(100,[exited.unread,original.unread],|pid|{
                queried.push(pid);
                (pid==100).then_some(original.started)
            });
            assert_eq!(snapshot,Some(vec![original]));
            assert_eq!(queried,[100],"另一實例退出時不應查詢它的進程元數據");
        }
    }
    #[test]
    fn feishu_in_tray_is_shown_by_feishu_itself(){
        assert_eq!(wake_route(Messenger::Feishu,false,false),WakeRoute::Relaunch,"關到托盤的飛書不能由島仔直接顯示（#181 卡死）");
        assert_eq!(wake_route(Messenger::Feishu,false,true),WakeRoute::Relaunch,"隱藏優先於殘留的最小化標誌");
        assert_eq!(wake_route(Messenger::Feishu,true,true),WakeRoute::Restore,"最小化的飛書照常還原");
        assert_eq!(wake_route(Messenger::Feishu,true,false),WakeRoute::Focus);
        for client in [Messenger::DingTalk,Messenger::WeCom] {assert_eq!(wake_route(client,false,false),WakeRoute::Show,"其他客戶端未驗證，保持原做法");}
    }
    #[test]
    fn a_queued_window_restore_waits_for_foreground_confirmation(){
        assert_eq!(finish_open(Some(true),||false),Ok(()),"恢復請求已進入窗口隊列，應繼續等原窗口切到前台");
        assert_eq!(finish_open(Some(false),||panic!("恢復請求被拒後無需再嘗試置前")),Err(OpenError::Unavailable));
        assert_eq!(finish_open(None,||false),Err(OpenError::Unavailable),"僅置前的請求被拒後沒有待確認的恢復操作");
    }
    #[test]
    fn feishu_rebuilt_during_relaunch_waits_for_the_same_accounts_visible_main_window(){unsafe{
        unsafe extern "system" fn proc(h:HWND,m:u32,w:WPARAM,l:LPARAM)->LRESULT{DefWindowProcW(h,m,w,l)}
        let instance=GetModuleHandleW(None).unwrap().into();
        let class=w!("Chrome_WidgetWin_1");
        RegisterClassW(&WNDCLASSW{lpfnWndProc:Some(proc),hInstance:instance,lpszClassName:class,..Default::default()});
        let create=|title|CreateWindowExW(WINDOW_EX_STYLE(0),class,title,WS_OVERLAPPEDWINDOW,-10000,-10000,500,500,None,None,instance,None).unwrap();
        let hidden=create(w!("飛書"));
        let original=Target{unread:Unread{source:Messenger::Feishu,pid:std::process::id(),window:hidden.0 as isize},
            started:process_started(GetCurrentProcess()).unwrap()};
        let paths=[normalized(&std::env::current_exe().unwrap().to_string_lossy())];
        assert_eq!(visible_relaunched_target(original,&paths),None);
        DestroyWindow(hidden).unwrap();
        assert_eq!(visible_relaunched_target(original,&paths),None,"新主窗口尚未創建時應繼續等待");
        let helper=create(w!("獨立會話"));
        let _=ShowWindow(helper,SW_SHOWNOACTIVATE);
        let unrelated=visible_relaunched_target(original,&paths);
        DestroyWindow(helper).unwrap();
        assert_eq!(unrelated,None,"同進程的獨立窗口不能接管這次喚起");
        let replacement=create(w!("飛書"));
        assert_eq!(visible_relaunched_target(original,&paths),None,"新窗口隱藏時應繼續等待");
        let _=ShowWindow(replacement,SW_SHOWNOACTIVATE);
        let shown=visible_relaunched_target(original,&paths);
        DestroyWindow(replacement).unwrap();
        assert_eq!(shown,Some(Target{unread:Unread{window:replacement.0 as isize,..original.unread},..original}));
    }}
    /// 本機實測：先把飛書關到托盤，再運行 `cargo test live_feishu -- --ignored`，走島仔點提醒時的同一條喚起路徑。
    #[test]
    #[ignore="需要本機飛書已登入並關到系統匣"]
    fn live_feishu_tray_window_is_shown_by_feishu(){unsafe{
        let paths=client_paths(Messenger::Feishu);
        let accounts:Vec<_>=client_processes(Messenger::Feishu).expect("應找到飛書進程").into_iter()
            .filter_map(|pid|candidates(Messenger::Feishu,pid)).flatten().collect();
        let [target]=accounts.as_slice() else{panic!("應只有一個飛書主窗口：{}",accounts.len())};
        let window=HWND(target.unread.window as *mut _);
        assert!(!IsWindowVisible(window).as_bool(),"請先把飛書關到系統匣");
        crate::messenger::windows::take_wake_steps();
        let result=open(*target,&paths,true);
        let steps=crate::messenger::windows::take_wake_steps();
        eprintln!("{steps:?}");
        assert_eq!(result,Ok(*target));
        let until=std::time::Instant::now()+std::time::Duration::from_secs(4);
        while GetForegroundWindow()!=window && std::time::Instant::now()<until{
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert_eq!(GetForegroundWindow(),window,"應將原飛書主窗口切到前台");
        assert!(IsWindowVisible(window).as_bool() && !IsIconic(window).as_bool());
        assert!(steps.iter().any(|s|s.contains("由客戶端自己顯示")));
    }}
    #[test]
    fn main_window_rules_exclude_login_popups_and_cross_client_windows(){
        let frame=WS_OVERLAPPEDWINDOW.0;
        assert!(main_frame(Messenger::WeCom,"WeWorkWindow",WS_POPUP.0,0));
        assert!(main_frame(Messenger::Feishu,"Chrome_WidgetWin_1",frame,0));
        assert!(main_frame(Messenger::DingTalk,"Qt51511QWindowIcon",frame,0));
        assert!(main_frame(Messenger::DingTalk,"StandardFrame_DingTalk",0x360f0000,0x100),"支援實際日誌中的已登入釘釘主視窗和最小化狀態");
        for client in Messenger::OFFICE{
            assert!(!main_frame(client,"WeChatLogin",frame,0));
            assert!(!main_frame(client,"Chrome_WidgetWin_0",frame,0));
            assert!(!main_frame(client,"PerryShadowWnd",frame,0));
        }
        assert!(!main_frame(Messenger::Feishu,"Chrome_WidgetWin_1",WS_POPUP.0,0));
        assert!(!main_frame(Messenger::DingTalk,"Qt51511QWindowIcon",0x96020000,0));
        assert!(!main_frame(Messenger::DingTalk,"StandardFrame_DingTalk",WS_POPUP.0,0));
        assert!(!main_frame(Messenger::DingTalk,"StandardFrame_DingTalk",frame,WS_EX_TOOLWINDOW.0));
        assert!(!main_frame(Messenger::DingTalk,"StandardFrame_DingTalk",frame|WS_CHILD.0,0));
        assert!(!main_frame(Messenger::Feishu,"StandardFrame_DingTalk",frame,0));
        assert!(!main_frame(Messenger::WeCom,"WeWorkWindow",WS_POPUP.0,WS_EX_TOOLWINDOW.0));
    }
    #[test]
    fn synthetic_native_windows_keep_original_handles_and_reject_ambiguity(){unsafe{
        unsafe extern "system" fn proc(h:HWND,m:u32,w:WPARAM,l:LPARAM)->LRESULT{DefWindowProcW(h,m,w,l)}
        let instance=GetModuleHandleW(None).unwrap().into();let pid=std::process::id();
        let paths=[normalized(&std::env::current_exe().unwrap().to_string_lossy())];
        let started=process_started(GetCurrentProcess()).unwrap();
        for (client,class) in [(Messenger::WeCom,w!("WeWorkWindow")),(Messenger::Feishu,w!("Chrome_WidgetWin_1")),(Messenger::DingTalk,w!("Qt51511QWindowIcon")),(Messenger::DingTalk,w!("StandardFrame_DingTalk"))]{
            RegisterClassW(&WNDCLASSW{lpfnWndProc:Some(proc),hInstance:instance,lpszClassName:class,..Default::default()});
            let create=||CreateWindowExW(WINDOW_EX_STYLE(0),class,w!("Kuidao synthetic client"),WS_OVERLAPPEDWINDOW,-10000,-10000,400,500,None,None,instance,None).unwrap();
            let first=create();let original=Target{unread:Unread{source:client,pid,window:first.0 as isize},started};
            assert!(ready_with_paths(original,&paths),"唯一原視窗必須可識別");
            assert_eq!(flash_target(client,first,&paths),Some(original.unread));
            let helper=CreateWindowExW(WS_EX_TOOLWINDOW,w!("STATIC"),w!(""),WS_POPUP,-10000,-10000,400,500,None,None,instance,None).unwrap();
            let ambiguous=flash_target(client,helper,&paths);DestroyWindow(helper).unwrap();
            assert_eq!(ambiguous,Some(Unread::of(client)),"同程序但沒有視窗歸屬關係的輔助視窗不能借用僅剩的主視窗");
            let owned=CreateWindowExW(WS_EX_TOOLWINDOW,w!("STATIC"),w!(""),WS_POPUP,-10000,-10000,400,500,first,None,instance,None).unwrap();
            let bound=flash_target(client,owned,&paths);DestroyWindow(owned).unwrap();
            assert_eq!(bound,Some(original.unread),"視窗明確屬於原主視窗時可以識別");
            let second=create();
            assert!(!ready_with_paths(original,&paths),"同程序新增主視窗後不得繼續按程序猜帳號");
            assert_eq!(flash_target(client,first,&paths),Some(Unread::of(client)));
            DestroyWindow(first).unwrap();
            assert!(!ready_with_paths(original,&paths),"不能換成仍存活的其他窗口");
            if client==Messenger::Feishu || client==Messenger::DingTalk && class_name(second)!=DINGTALK_FRAME {
                assert_eq!(rebuild_target(original,&paths),None,"沒有主窗口特徵的獨立窗口不能作為重建窗口");
            }
            assert_eq!(open(original,&paths,true),Err(OpenError::Unavailable));
            assert!(!IsWindowVisible(second).as_bool(),"不能打開其他窗口");
            DestroyWindow(second).unwrap();
        }
        // 不足主窗口尺寸的 StandardFrame 不應擋住同進程真正的 Qt 主窗口。
        let small=CreateWindowExW(WINDOW_EX_STYLE(0x100),w!("StandardFrame_DingTalk"),w!(""),WINDOW_STYLE(0x060f0000),-10000,-10000,220,220,None,None,instance,None).unwrap();
        let qt=CreateWindowExW(WINDOW_EX_STYLE(0x100),w!("Qt51511QWindowIcon"),w!(""),WINDOW_STYLE(0x860f0000),-10000,-10000,1200,800,None,None,instance,None).unwrap();
        let original=Target{unread:Unread{source:Messenger::DingTalk,pid,window:qt.0 as isize},started};
        let _=ShowWindow(small,SW_SHOWMINNOACTIVE);
        assert!(IsIconic(small).as_bool());
        assert!(!main_window(small,Messenger::DingTalk));
        assert_eq!(flash_target(Messenger::DingTalk,qt,&paths),Some(original.unread));
        assert!(ready_with_paths(original,&paths));
        let new_main=CreateWindowExW(WINDOW_EX_STYLE(0x100),w!("Qt51511QWindowIcon"),w!(""),WINDOW_STYLE(0x860f0000),-10000,-10000,1200,800,None,None,instance,None).unwrap();
        assert_eq!(rebuild_target(original,&paths),None,"原窗口存活時不能重綁");
        DestroyWindow(qt).unwrap();
        let rebuilt=Target{unread:Unread{window:new_main.0 as isize,..original.unread},..original};
        assert_eq!(rebuild_target(original,&paths),Some(rebuilt),"原窗口銷燬後允許同一進程中唯一可信的新主窗口");
        assert_eq!(rebuild_target(Target{started:started.wrapping_add(1),..original},&paths),None,"禁止重綁到複用 PID 的新進程");
        DestroyWindow(small).unwrap();DestroyWindow(new_main).unwrap();
        // 用戶診斷包裡釘釘 9.0.5 的窗口組合：一個 StandardFrame_DingTalk 主窗口和三個隱藏的 Qt 對話框。
        // 主窗口最小化到任務欄和隱藏到托盤時，都必須對應到原進程。
        for (state,frame_style) in [("最小化",0x260f0000u32),("隱藏到系統匣",0x060f0000)]{
            let frame=CreateWindowExW(WINDOW_EX_STYLE(0x100),w!("StandardFrame_DingTalk"),w!("Kuidao synthetic client"),WINDOW_STYLE(frame_style),-10000,-10000,1200,800,None,None,instance,None).unwrap();
            let dialogs=[0;3].map(|_|CreateWindowExW(WINDOW_EX_STYLE(0x100),w!("Qt51511QWindowIcon"),w!(""),WINDOW_STYLE(0x86cf0000),-10000,-10000,655,517,None,None,instance,None).unwrap());
            let original=Target{unread:Unread{source:Messenger::DingTalk,pid,window:frame.0 as isize},started};
            assert!(dialogs.iter().all(|dialog|!main_window(*dialog,Messenger::DingTalk)),"{state}：隱藏的 Qt 對話方塊不能算主視窗");
            assert!(ready_with_paths(original,&paths),"{state}：主視窗必須可識別");
            assert_eq!(flash_target(Messenger::DingTalk,frame,&paths),Some(original.unread),"{state}：閃動必須對應到原程序");
            DestroyWindow(frame).unwrap();
            assert!(dialogs.iter().all(|dialog|main_window(*dialog,Messenger::DingTalk)),"沒有新版主視窗時仍按舊版 Qt 主視窗識別");
            assert_eq!(flash_target(Messenger::DingTalk,dialogs[0],&paths),Some(Unread::of(Messenger::DingTalk)),"舊版多個 Qt 主視窗仍不能猜帳號");
            for dialog in dialogs{DestroyWindow(dialog).unwrap();}
        }
        // 釘釘 9.0.5 的 Qt 主窗口最小化時樣式是 0xB60F0000。收進托盤後，此處模擬它清除可見和最小化標誌。
        for (state,main_style) in [("顯示",0x960f0000u32),("最小化",0xb60f0000),("關到托盤",0x860f0000)]{
            let main=CreateWindowExW(WINDOW_EX_STYLE(0x100),w!("Qt51511QWindowIcon"),w!("Daozai synthetic client"),WINDOW_STYLE(main_style),-10000,-10000,1200,800,None,None,instance,None).unwrap();
            let dialogs=[0;3].map(|_|CreateWindowExW(WINDOW_EX_STYLE(0x100),w!("Qt51511QWindowIcon"),w!(""),WINDOW_STYLE(0x86cf0000),-10000,-10000,655,517,None,None,instance,None).unwrap());
            let original=Target{unread:Unread{source:Messenger::DingTalk,pid,window:main.0 as isize},started};
            assert_eq!(flash_target(Messenger::DingTalk,main,&paths),Some(original.unread),"{state}：閃動必須對應到原主窗口");
            assert!(ready_with_paths(original,&paths),"{state}：點擊時仍能確認原主窗口");
            assert_eq!(flash_target(Messenger::DingTalk,dialogs[0],&paths),Some(Unread::of(Messenger::DingTalk)),"{state}：無歸屬關係的對話框不能代替主窗口");
            let log=describe_process_windows(Messenger::DingTalk,pid);
            assert!(log.contains("Qt51511QWindowIcon") && log.contains("655×517") && !log.contains("Daozai"),"{state}：診斷只記類名和尺寸，不記標題：{log}");
            DestroyWindow(main).unwrap();for dialog in dialogs{DestroyWindow(dialog).unwrap();}
        }
        // 兩個同樣式的 Qt 主窗口無論顯示還是隱藏，都不能憑閃動猜帳號。
        for style in [0x16cf0000,0x860f0000]{
            let pair=[0;2].map(|_|CreateWindowExW(WINDOW_EX_STYLE(0x100),w!("Qt51511QWindowIcon"),w!(""),WINDOW_STYLE(style),-10000,-10000,1200,800,None,None,instance,None).unwrap());
            assert_eq!(flash_target(Messenger::DingTalk,pair[0],&paths),Some(Unread::of(Messenger::DingTalk)));
            for window in pair{DestroyWindow(window).unwrap();}
        }
        // 本機飛書 8.6 實測：主窗口標題「飛書」，「在獨立窗口打開」的會話窗口與它同進程、同類名、同樣式，標題是會話名。
        // 主窗口正常顯示和關到托盤（隱藏）兩種狀態都要能對應到原進程，獨立窗口閃動歸到主窗口。
        for (state,main_style) in [("顯示",0x16cf0000u32),("關到系統匣",0x06cf0000)]{
            let frame=|title,style|CreateWindowExW(WINDOW_EX_STYLE(0x00200100),w!("Chrome_WidgetWin_1"),title,WINDOW_STYLE(style),-10000,-10000,1200,800,None,None,instance,None).unwrap();
            let main=frame(w!("飛書"),main_style);let chat=frame(w!("少爺"),0x16cf0000);
            let original=Target{unread:Unread{source:Messenger::Feishu,pid,window:main.0 as isize},started};
            assert!(!main_window(chat,Messenger::Feishu),"{state}：獨立會話視窗不能算第二個主視窗");
            assert!(ready_with_paths(original,&paths),"{state}：有獨立會話視窗時主視窗仍可識別");
            assert_eq!(flash_target(Messenger::Feishu,main,&paths),Some(original.unread),"{state}：主視窗閃動對應原程序");
            assert_eq!(flash_target(Messenger::Feishu,chat,&paths),Some(original.unread),"{state}：獨立會話視窗閃動歸到同程序主視窗");
            let lookalike=frame(w!("Feishu"),0x16cf0000);
            assert_eq!(flash_target(Messenger::Feishu,chat,&paths),Some(Unread::of(Messenger::Feishu)),"{state}：兩個視窗都像主視窗時不猜");
            DestroyWindow(lookalike).unwrap();DestroyWindow(main).unwrap();
            assert!(main_window(chat,Messenger::Feishu),"{state}：找不到主視窗時沿用原規則");
            DestroyWindow(chat).unwrap();
        }
        // 同名小窗與主窗口最小化後的系統矩形相似，應以恢復尺寸區分。
        let create=|title,width,height|CreateWindowExW(WINDOW_EX_STYLE(0),w!("Chrome_WidgetWin_1"),title,WS_OVERLAPPEDWINDOW,-10000,-10000,width,height,None,None,instance,None).unwrap();
        let main=create(w!("飛書"),998,726);
        let small=create(w!("Feishu"),220,220);
        let chat=create(w!("獨立會話"),400,500);
        for window in [main,small]{
            let _=ShowWindow(window,SW_SHOWMINNOACTIVE);
            assert!(IsIconic(window).as_bool(),"測試窗口必須進入最小化狀態");
        }
        let original=Target{unread:Unread{source:Messenger::Feishu,pid,window:main.0 as isize},started};
        assert!(!main_window(small,Messenger::Feishu),"真正的小窗最小化後仍應被排除");
        assert!(ready_with_paths(original,&paths),"正常的主窗口最小化後仍可識別");
        assert_eq!(flash_target(Messenger::Feishu,main,&paths),Some(original.unread));
        assert_eq!(flash_target(Messenger::Feishu,chat,&paths),Some(original.unread),"同名小窗不影響獨立會話的歸屬");
        assert_eq!(flash_target(Messenger::Feishu,small,&paths),Some(Unread::of(Messenger::Feishu)),"同名小窗閃動不應借用主窗口");
        for window in [chat,small,main]{DestroyWindow(window).unwrap();}
    }}
    #[test]
    fn hidden_helpers_are_never_shown(){unsafe{
        let paths=[normalized(&std::env::current_exe().unwrap().to_string_lossy())];
        let window=CreateWindowExW(WS_EX_TOOLWINDOW,w!("STATIC"),w!(""),WS_POPUP,-10000,-10000,400,500,None,None,None,None).unwrap();
        for source in Messenger::OFFICE{
            assert_eq!(open(target(source,std::process::id(),window.0 as isize),&paths,true),Err(OpenError::Unavailable));
            assert!(!IsWindowVisible(window).as_bool());
        }
        DestroyWindow(window).unwrap();
    }}
}

//! 仅在通知缺少应用标识时，按编号和创建时间核对 Windows 记录的发送方；不查询通知正文。
use std::{ffi::{c_char,c_int,c_void,CStr,CString},path::Path,ptr};
type RowCallback=unsafe extern "C" fn(*mut c_void,c_int,*mut *mut c_char,*mut *mut c_char)->c_int;
unsafe extern "C" fn capture_text(data:*mut c_void,count:c_int,values:*mut *mut c_char,_:*mut *mut c_char)->c_int{
    if count>0 && !values.is_null(){unsafe{if !(*values).is_null(){let text=CStr::from_ptr(*values).to_string_lossy().into_owned();*data.cast::<Option<String>>()=Some(text);}}}0
}
#[link(name="kernel32")]
extern "system" {
    fn LoadLibraryW(name:*const u16)->*mut c_void;
    fn GetProcAddress(module:*mut c_void,name:*const u8)->*mut c_void;
    fn FreeLibrary(module:*mut c_void)->i32;
}
struct PlainDb{module:*mut c_void,db:*mut c_void}
impl Drop for PlainDb{
    fn drop(&mut self){
        unsafe{
            let proc=GetProcAddress(self.module,b"sqlite3_close\0".as_ptr());
            if !proc.is_null(){let close:unsafe extern "C" fn(*mut c_void)->c_int=std::mem::transmute(proc);close(self.db);}
            FreeLibrary(self.module);
        }
    }
}
fn open_plain(name:*const c_char)->Option<PlainDb>{
    let dll:Vec<u16>="winsqlite3.dll".encode_utf16().chain(Some(0)).collect();
    unsafe{
        let module=LoadLibraryW(dll.as_ptr());
        if module.is_null(){return None;}
        let proc=GetProcAddress(module,b"sqlite3_open\0".as_ptr());
        if proc.is_null(){FreeLibrary(module);return None;}
        let open:unsafe extern "C" fn(*const c_char,*mut *mut c_void)->c_int=std::mem::transmute(proc);
        let mut db=ptr::null_mut();
        if open(name,&mut db)==0{Some(PlainDb{module,db})}else{FreeLibrary(module);None}
    }
}
fn exec_plain(module:*mut c_void,db:*mut c_void,sql:*const c_char,data:*mut c_void,error:*mut *mut c_char)->c_int{
    unsafe{
        let proc=GetProcAddress(module,b"sqlite3_exec\0".as_ptr());
        if proc.is_null(){return 1;}
        let exec:unsafe extern "C" fn(*mut c_void,*const c_char,Option<RowCallback>,*mut c_void,*mut *mut c_char)->c_int=std::mem::transmute(proc);
        exec(db,sql,Some(capture_text),data,error)
    }
}
#[link(name="winsqlite3")]
extern "C" {
    fn sqlite3_open_v2(name:*const c_char,db:*mut *mut c_void,flags:c_int,vfs:*const c_char)->c_int;
    fn sqlite3_exec(db:*mut c_void,sql:*const c_char,callback:Option<RowCallback>,data:*mut c_void,error:*mut *mut c_char)->c_int;
    fn sqlite3_close(db:*mut c_void)->c_int;
}
pub(super) fn sender(id:u32,created:i64)->Option<String>{
    let local=std::env::var_os("LOCALAPPDATA")?;
    sender_in(&Path::new(&local).join("Microsoft/Windows/Notifications/wpndatabase.db"),id,created)
}

/// 通知原始的 `launch` 參數：LINE 這類程式會把特定聊天室放進 `line://…`，
/// 直接開啟就能回到同一個對話，不必只把主視窗叫到前面。
pub(super) fn launch(id:u32,created:i64)->Option<String>{
    let local=std::env::var_os("LOCALAPPDATA")?;
    let db=Path::new(&local).join("Microsoft/Windows/Notifications/wpndatabase.db");
    launch_in(&db,id,created).or_else(||{
        // 通知服務有時會短暫鎖住資料庫；退一步讀一份當下快照，避免點擊時沒有深層連結。
        let copy=snapshot(&db)?;
        let found=launch_in(&copy,id,created);
        let _=std::fs::remove_file(&copy);
        found
    })
}
fn copy_shared(source:&Path,target:&Path)->Option<()>{
    // 通知服務會用共享讀取鎖住資料庫；std::fs::copy 在這種情況下會被拒絕，
    // 以一般 File 開啟（Rust 會帶 FILE_SHARE_READ/WRITE/DELETE）再逐段複製。
    let mut input=std::fs::File::open(source).ok()?;
    let mut output=std::fs::File::create(target).ok()?;
    std::io::copy(&mut input,&mut output).ok()?;
    Some(())
}
fn snapshot(source:&Path)->Option<std::path::PathBuf>{
    let stamp=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos();
    let copy=std::env::temp_dir().join(format!("kuidao-wpn-{}-{stamp}.db",std::process::id()));
    copy_shared(source,&copy)?;
    Some(copy)
}

fn read_text(path:&Path,sql:&str)->Option<String>{
    let display=path.to_string_lossy().replace("/","\\");
    let name=CString::new(display.as_bytes()).ok()?;
    let sql=CString::new(sql).ok()?;
    let mut db=ptr::null_mut();let mut value=None;let mut plain=None;
    unsafe{
        let ro=sqlite3_open_v2(name.as_ptr(),&mut db,1,ptr::null());
        let mut opened=ro==0;
        if !opened && !db.is_null(){sqlite3_close(db);db=ptr::null_mut();}
        // 通知服務鎖住本體時，先以共享讀取複製一份再查；動態開檔不帶 CREATE，避免誤建空資料庫。
        if !opened && path.exists(){
            plain=open_plain(name.as_ptr());
            opened=plain.is_some();
        }
        if !opened{
            let uri=format!("file:{}?mode=ro&immutable=1",display.replace('\\',"/"));
            if let Ok(uri)=CString::new(uri){opened=sqlite3_open_v2(uri.as_ptr(),&mut db,1|0x40,ptr::null())==0;}
        }
        let mut err:*mut c_char=ptr::null_mut();
        let rc=if let Some(p)=&plain{exec_plain(p.module,p.db,sql.as_ptr(),(&mut value as *mut Option<String>).cast(),&mut err)}
            else if opened{sqlite3_exec(db,sql.as_ptr(),Some(capture_text),(&mut value as *mut Option<String>).cast(),&mut err)}
            else{-99};
        if plain.is_none() && !db.is_null(){sqlite3_close(db);}
        if rc==0{value}else{None}
    }
}

fn launch_in(path:&Path,id:u32,created:i64)->Option<String>{
    let sql=format!("SELECT CAST(Payload AS TEXT) FROM Notification WHERE Id={id} AND ArrivalTime={created} AND Type='toast' LIMIT 1");
    parse_launch(&read_text(path,&sql)?)
}
fn parse_launch(payload:&str)->Option<String>{
    let start=payload.find("<toast")?;
    let attr=payload[start..].find("launch=\"")?+start+8;
    let end=payload[attr..].find('"')?+attr;
    let launch=payload[attr..end].trim();
    if launch.is_empty(){return None;}
    Some(xml_unescape(launch))
}

fn xml_unescape(value:&str)->String{
    value.replace("&quot;","\"").replace("&apos;","'").replace("&lt;","<").replace("&gt;",">").replace("&amp;","&")
}
fn sender_in(path:&Path,id:u32,created:i64)->Option<String>{
    // 参数均为数值，创建时间必须同时匹配，避免通知编号复用时混淆应用。
    let sql=format!("SELECT h.PrimaryId FROM Notification n JOIN NotificationHandler h ON h.RecordId=n.HandlerId WHERE n.Id={id} AND n.ArrivalTime={created} AND n.Type='toast' LIMIT 1");
    let found=read_text(path,&sql)?;
    (!found.trim().is_empty()).then_some(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore="winsqlite3 在此測試環境無法建立新資料庫，需以實機通知中心驗證"]
    fn sender_lookup_matches_creation_time_and_never_modifies_the_database(){
        let path=std::env::temp_dir().join(format!("daozai-toast-source-{}.db",std::process::id()));
        let _=std::fs::remove_file(&path);
        let _=std::fs::remove_file(format!("{}-wal",path.display()));
        let _=std::fs::remove_file(format!("{}-shm",path.display()));
        let name=CString::new(path.to_string_lossy().as_bytes()).unwrap();let mut db=ptr::null_mut();
        let sql=CString::new("CREATE TABLE NotificationHandler (RecordId INTEGER, PrimaryId TEXT); CREATE TABLE Notification (Id INTEGER, ArrivalTime INTEGER, HandlerId INTEGER, Type TEXT); INSERT INTO NotificationHandler VALUES (1,'QQ'),(2,'OtherApp'); INSERT INTO Notification VALUES (10,100,1,'toast'),(11,101,2,'toast'),(12,102,1,'badge');").unwrap();
        unsafe{
            assert_eq!(sqlite3_open_v2(name.as_ptr(),&mut db,6,ptr::null()),0);
            let result=sqlite3_exec(db,sql.as_ptr(),None,ptr::null_mut(),ptr::null_mut());
            sqlite3_close(db);assert_eq!(result,0);
        }
        let before=std::fs::read(&path).unwrap();
        let found=[sender_in(&path,10,100),sender_in(&path,11,101),sender_in(&path,10,101),sender_in(&path,12,102)];
        let after=std::fs::read(&path).unwrap();std::fs::remove_file(&path).unwrap();
        assert_eq!(found,[Some("QQ".into()),Some("OtherApp".into()),None,None]);assert_eq!(before,after);
        assert_eq!(sender_in(&path,10,100),None);assert!(!path.exists(),"只讀查詢不能新建資料庫");
    }
    #[test]
    fn launch_reads_the_raw_toast_and_keeps_protocol_arguments(){
        let line=r#"<toast launch="line://chat/?mid=abc&amp;id=1" activationType="protocol"><visual/></toast>"#;
        let phone=r#"<?xml version="1.0"?><toast launch="ms-phone:phonenotifications?deviceid=9&amp;notificationId=8%7c1" activationType="protocol"></toast>"#;
        assert_eq!(parse_launch(line).as_deref(),Some("line://chat/?mid=abc&id=1"));
        assert_eq!(parse_launch(phone).as_deref(),Some("ms-phone:phonenotifications?deviceid=9&notificationId=8%7c1"));
        assert_eq!(parse_launch(r#"<toast launch="" activationType="protocol"/>"#),None);
        assert_eq!(parse_launch(r#"<badge value="3"/>"#),None);
        assert_eq!(parse_launch(""),None);
    }
    #[test]
    #[ignore="需要本機通知中心留有指定測試通知"]
    fn phone_link_launch_local(){
        let id=std::env::var("KUIDAO_TEST_TOAST_ID").unwrap().parse::<u32>().unwrap();
        let created=std::env::var("KUIDAO_TEST_TOAST_TIME").unwrap().parse().unwrap();
        let launch=launch(id,created).expect("讀不到通知的 launch");
        assert!(launch.starts_with("ms-phone:phonenotifications?"),"實際 {launch}");
    }
    #[test]
    #[ignore="需要本機通知中心留有指定測試通知"]
    fn qq_notification_local_identity(){
        let id=std::env::var("KUIDAO_QQ_TEST_ID").unwrap().parse().unwrap();
        use windows::UI::Notifications::{Management::UserNotificationListener,NotificationKinds};
        use windows::Win32::System::WinRT::{RoInitialize,RoUninitialize,RO_INIT_MULTITHREADED};
        unsafe{RoInitialize(RO_INIT_MULTITHREADED).unwrap();}
        let items=UserNotificationListener::Current().unwrap().GetNotificationsAsync(NotificationKinds::Toast).unwrap().get().unwrap();
        let n=(0..items.Size().unwrap()).filter_map(|i|items.GetAt(i).ok()).find(|n|n.Id().ok()==Some(id)).unwrap();
        let created=n.CreationTime().unwrap().UniversalTime;
        assert_eq!(super::super::notification_identity(&n,id,created,None),("QQ".into(),"QQ".into()));
        assert_eq!(sender(id,created).as_deref(),Some("QQ"));assert!(sender(id,created+1).is_none());
        drop(n);drop(items);unsafe{RoUninitialize();}
    }
}

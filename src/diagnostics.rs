//! 啟動錯誤與 Rust 異常儲存在本機，不自動上傳帳號或應用數據。

use std::path::{Path, PathBuf};

fn write_report(directory: &Path, title: &str, details: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(directory)?;
    let path = directory.join("last-error.log");
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    std::fs::write(&path, format!(
        "鬼島 {}\n系統：{} / {}\n時間（Unix 秒）：{timestamp}\n{title}\n{details}\n",
        env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH,
    ))?;
    Ok(path)
}

fn message(directory: &Path, title: &str, details: &str) -> String {
    match write_report(directory, title, details) {
        Ok(path) => format!("{title}\n\n請將以下日誌發給開發者，幫助定位問題：\n{}", path.display()),
        Err(error) => format!("{title}\n\n{details}\n\n日誌無法儲存：{error}\n請截圖回饋。"),
    }
}

pub fn report(title: &str, details: &str) {
    let text = message(&daozai::model::config_dir().join("diagnostics"), title, details);
    #[cfg(windows)]
    {
        use windows::core::{w, PCWSTR};
        use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
        let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
        unsafe { MessageBoxW(None, PCWSTR(wide.as_ptr()), w!("鬼島"), MB_OK | MB_ICONERROR); }
    }
    #[cfg(not(windows))]
    eprintln!("{text}");
}

/// 僅記錄最近一次啟動停留的階段，不記錄帳號或用戶內容。
pub fn startup(stage:&str) {
    let directory=daozai::model::config_dir().join("diagnostics");
    if std::fs::create_dir_all(&directory).is_ok() {
        let _=std::fs::write(directory.join("last-startup.log"),format!(
            "鬼島 {}\n程序：{}\n啟動階段：{stage}\n",env!("CARGO_PKG_VERSION"),std::process::id()));
    }
}

/// 記錄實際使用的圖形裝置，避免把電腦裡的顯卡列表誤當作當前渲染裝置。
pub fn graphics(cc:&eframe::CreationContext<'_>) {
    use eframe::glow::HasContext;
    let details=if let Some(gl)=&cc.gl {unsafe{format!("廠商：{}\n渲染器：{}\nOpenGL：{}\n",
        gl.get_parameter_string(eframe::glow::VENDOR),gl.get_parameter_string(eframe::glow::RENDERER),gl.get_parameter_string(eframe::glow::VERSION))}}
    else if let Some(state)=&cc.wgpu_render_state {
        let info=state.adapter.get_info();format!("渲染器：{}\n圖形後端：{:?}\n裝置型別：{:?}\n",info.name,info.backend,info.device_type)
    }else{return;};
    let directory=daozai::model::config_dir().join("diagnostics");
    if std::fs::create_dir_all(&directory).is_ok(){let _=std::fs::write(directory.join("last-graphics.log"),details);}
}

static STARTED: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

fn diagnostics_dir() -> PathBuf { daozai::model::config_dir().join("diagnostics") }

/// 自動重啟的頻率限制：本次至少運行 30 秒（避免啟動即崩的死循環），且一小時內最多 3 次。
fn restart_allowed(history: &str, now: u64, uptime: std::time::Duration) -> bool {
    if uptime < std::time::Duration::from_secs(30) { return false; }
    let recent = history.lines().filter_map(|line| line.split_whitespace().next()?.parse::<u64>().ok())
        .filter(|at| now.saturating_sub(*at) < 3600).count();
    recent < 3
}

/// 啟動一個新的鬼島：它會先等當前進程退出，再走正常的單實例啟動流程。
pub fn spawn_replacement() -> bool {
    spawn_replacement_with_args(&[])
}

pub fn spawn_replacement_with_args(args: &[&str]) -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    let mut command = std::process::Command::new(exe);
    command.arg("--restart-after").arg(std::process::id().to_string()).args(args).env_remove("DAOZAI_TEST_CRASH");
    if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from)) { command.current_dir(dir); }
    command.spawn().is_ok()
}

/// 異常退出後自動重新拉起（受頻率限制）；返回是否已安排重啟。
fn relaunch_after_failure(reason: &str) -> bool {
    let uptime = STARTED.get().map(|at| at.elapsed()).unwrap_or_default();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    let path = diagnostics_dir().join("auto-restart.log");
    let history = std::fs::read_to_string(&path).unwrap_or_default();
    if !restart_allowed(&history, now, uptime) { return false; }
    // 只保留最近 20 條，避免常駐程序無限累積。
    let mut lines: Vec<&str> = history.lines().collect();
    if lines.len() > 19 { lines.drain(..lines.len() - 19); }
    let reason: String = reason.chars().filter(|c| !c.is_control()).take(160).collect();
    let entry = format!("{now} 執行 {} 秒後{reason}，已自動重啟", uptime.as_secs());
    lines.push(&entry);
    let _ = std::fs::create_dir_all(diagnostics_dir());
    let _ = std::fs::write(&path, lines.join("\n") + "\n");
    spawn_replacement()
}

/// 自動重啟的新進程：等上一個鬼島退出（最多 15 秒）再繼續，避免單實例檢查把自己擋住。
pub fn wait_for_previous_instance() {
    let mut args = std::env::args();
    while let Some(arg) = args.next() {
        if arg != "--restart-after" { continue; }
        let Some(pid) = args.next().and_then(|v| v.parse::<u32>().ok()) else { return };
        #[cfg(windows)]
        unsafe {
            use windows::Win32::{Foundation::CloseHandle, System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE}};
            if let Ok(process) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
                WaitForSingleObject(process, 15_000);
                let _ = CloseHandle(process);
            }
        }
        #[cfg(not(windows))]
        let _ = pid;
        return;
    }
}

/// 進程級崩潰（例如顯卡驅動裡訪問了無效內存）：Rust 的 panic 記錄覆蓋不到，這裡記下崩在哪個模塊；
/// 崩在顯卡的 OpenGL 驅動裡時，下次啟動改用相容渲染；隨後按頻率限制自動重啟。
#[cfg(windows)]
mod native_crash {
    use std::sync::atomic::{AtomicBool, Ordering};
    use windows::Win32::{Foundation::HMODULE, System::{Diagnostics::Debug::{SetUnhandledExceptionFilter, EXCEPTION_POINTERS},
        LibraryLoader::{GetModuleFileNameW, GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT}}};
    const EXCEPTION_CONTINUE_SEARCH: i32 = 0;
    static HANDLING: AtomicBool = AtomicBool::new(false);

    /// 異常地址所在的模塊檔案名與模塊內偏移。
    unsafe fn module_at(address: usize) -> (String, usize) {
        let mut module = HMODULE::default();
        let flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT;
        if GetModuleHandleExW(flags, windows::core::PCWSTR(address as *const u16), &mut module).is_err() { return ("未知模組".into(), address); }
        let mut name = [0u16; 520];
        let length = GetModuleFileNameW(module, &mut name) as usize;
        let path = String::from_utf16_lossy(&name[..length.min(name.len())]);
        let file = path.rsplit('\\').next().unwrap_or(&path).to_string();
        (file, address.wrapping_sub(module.0 as usize))
    }

    unsafe extern "system" fn filter(info: *const EXCEPTION_POINTERS) -> i32 {
        if HANDLING.swap(true, Ordering::SeqCst) || info.is_null() || (*info).ExceptionRecord.is_null() { return EXCEPTION_CONTINUE_SEARCH; }
        let record = &*(*info).ExceptionRecord;
        let code = record.ExceptionCode.0 as u32;
        let (module, offset) = module_at(record.ExceptionAddress as usize);
        let uptime = super::STARTED.get().map(|at| at.elapsed().as_secs()).unwrap_or(0);
        let gl_driver = daozai::renderer::is_gl_driver(&module);
        let renderer = if daozai::renderer::compat_active() { "相容渲染（Direct3D 12）" } else { "OpenGL" };
        let mut details = format!("異常程式碼：{code:#010x}\n崩潰模組：{module}\n模組內偏移：{offset:#x}\n已執行：{uptime} 秒\n渲染方式：{renderer}\n");
        if gl_driver && !daozai::renderer::compat_active() {
            daozai::renderer::request_fallback(&module);
            details.push_str("判斷：崩在顯示卡的 OpenGL 驅動裡，下次啟動自動改用相容渲染（Direct3D 12）。\n");
        }
        let restarted = super::relaunch_after_failure(&format!("程序崩潰（{module}）"));
        details.push_str(if restarted { "已自動重新啟動鬼島。\n" } else { "未自動重啟（執行不足 30 秒或一小時內已重啟 3 次）。\n" });
        let _ = super::write_report(&super::diagnostics_dir(), "鬼島意外退出（程序崩潰）", &details);
        // 交給系統繼續處理，Windows 事件日誌裡仍會留下崩潰記錄。
        EXCEPTION_CONTINUE_SEARCH
    }

    pub fn install() { unsafe { SetUnhandledExceptionFilter(Some(filter)); } }

    /// 驗收用：`KUIDAO_TEST_CRASH=native` 時，運行指定秒數後在鬼島自身觸發一次訪問違規。
    pub fn test_crash_hook() {
        let Ok(value) = std::env::var("KUIDAO_TEST_CRASH") else { return };
        let Some(seconds) = value.strip_prefix("native:").and_then(|s| s.parse::<u64>().ok()) else { return };
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(seconds));
            unsafe { std::ptr::write_volatile(std::ptr::null_mut::<u32>(), 1); }
        });
    }
}

pub fn install() {
    let _ = STARTED.set(std::time::Instant::now());
    #[cfg(windows)]
    { native_crash::install(); native_crash::test_crash_hook(); }
    std::panic::set_hook(Box::new(|info| {
        // 不寫入 panic 的數據載荷，避免把郵件、憑據或登錄資訊帶進日誌。
        let location = info.location().map(|p| format!("{}:{}:{}", p.file(), p.line(), p.column()))
            .unwrap_or_else(|| "未知".into());
        let details = format!("程式碼位置：{location}\n呼叫棧：\n{}", std::backtrace::Backtrace::force_capture());
        // 主線程出錯會讓整個鬼島退出：記下原因後直接自動重啟，不彈窗打斷。後台線程出錯不影響主界面，仍按原樣提示。
        if std::thread::current().name() == Some("main") && relaunch_after_failure("程式異常") {
            let _ = write_report(&diagnostics_dir(), "鬼島發生異常，已自動重新啟動。", &details);
            return;
        }
        report("鬼島發生異常，請重新啟動。", &details);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directory(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("daozai-diagnostics-{}-{name}", std::process::id()))
    }

    #[test]
    fn failure_creates_a_report_and_exposes_its_location() {
        let directory = directory("report");
        let text = message(&directory, "啟動失敗", "OpenGL 初始化失敗");
        let path = directory.join("last-error.log");
        assert!(text.contains(&path.display().to_string()));
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains(env!("CARGO_PKG_VERSION")));
        assert!(content.contains("OpenGL 初始化失敗"));
        // 重複失敗只保留最後一份，避免常駐程序無限累積日誌。
        write_report(&directory, "第二次失敗", "新的錯誤").unwrap();
        assert!(!std::fs::read_to_string(&path).unwrap().contains("OpenGL"));
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn auto_restart_is_rate_limited() {
        use std::time::Duration;
        let now = 10_000;
        assert!(restart_allowed("", now, Duration::from_secs(60)));
        assert!(!restart_allowed("", now, Duration::from_secs(10)), "啟動不到 30 秒就崩潰不自動重啟，避免死迴圈");
        let three = "9000 a\n9500 b\n9900 c\n";
        assert!(!restart_allowed(three, now, Duration::from_secs(600)), "一小時內最多 3 次");
        let old = "1000 a\n2000 b\n9900 c\n";
        assert!(restart_allowed(old, now, Duration::from_secs(600)), "一小時前的重啟不計入");
        assert!(restart_allowed("亂碼\n\n", now, Duration::from_secs(600)), "記錄損壞時仍可重啟");
    }

    #[test]
    fn unwritable_log_keeps_the_error_visible() {
        let path = directory("blocked");
        std::fs::write(&path, b"test").unwrap();
        let text = message(&path, "啟動失敗", "OpenGL 初始化失敗");
        assert!(text.contains("OpenGL 初始化失敗"));
        assert!(text.contains("日誌無法儲存"));
        std::fs::remove_file(path).unwrap();
    }
}

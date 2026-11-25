#![cfg_attr(windows, windows_subsystem = "windows")]
#![allow(unused_must_use)]
//! 極小安裝啟動器：安裝包本體不含主程式。
//!
//! 第一次開啟時查詢 GitHub 最新 Release，下載發佈 zip、驗證 SHA-256、
//! 解壓到安裝目錄後啟動主程式（正式版 `kuidao.exe`／開發版 `鬼島 Kuidao 開發版.exe`）。

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::Duration;

const REPOSITORY: &str = "https://github.com/JCnTEA/kuidao-tw";
const API: &str = "https://api.github.com/repos/JCnTEA/kuidao-tw/releases/latest";
/// 開發版走另一條發布線：最新一個帶 `-tw-dev` 的 prerelease，每次啟動都比版本、自動換新版。
const API_DEV: &str = "https://api.github.com/repos/JCnTEA/kuidao-tw/releases?per_page=20";
const APP_EXE: &str = "kuidao.exe";
/// 同一份啟動器原始碼，靠建置時的 `KUIDAO_BOOTSTRAP_CHANNEL=dev` 切成開發版啟動器。
fn is_dev() -> bool {
    option_env!("KUIDAO_BOOTSTRAP_CHANNEL") == Some("dev")
}
/// 開發版主程式在發佈包裡的檔名（中文名，跟正式版的 kuidao.exe 分開）。
const APP_EXE_DEV: &str = "鬼島 Kuidao 開發版.exe";

fn app_name() -> &'static str {
    if is_dev() {
        APP_EXE_DEV
    } else {
        APP_EXE
    }
}

fn channel_title() -> &'static str {
    if is_dev() {
        "鬼島 · 開發版"
    } else {
        "鬼島 · 首次啟動"
    }
}

static PERCENT: AtomicU32 = AtomicU32::new(0);
static QUIT: AtomicBool = AtomicBool::new(false);
static STATUS: Mutex<String> = Mutex::new(String::new());
static DETAIL: Mutex<String> = Mutex::new(String::new());
static FAILED: AtomicBool = AtomicBool::new(false);

fn set_status(status: impl Into<String>, detail: impl Into<String>, percent: u32) {
    if let Ok(mut text) = STATUS.lock() {
        *text = status.into();
    }
    if let Ok(mut text) = DETAIL.lock() {
        *text = detail.into();
    }
    PERCENT.store(percent.min(100), Ordering::Relaxed);
    repaint();
}

#[cfg(windows)]
mod gui {
    use super::{channel_title, is_dev, DETAIL, FAILED, PERCENT, QUIT, STATUS};
    use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect,
        SetBkMode, SetTextColor, UpdateWindow, DRAW_TEXT_FORMAT, HDC, PAINTSTRUCT, TRANSPARENT,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForSystem};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
        GetSystemMetrics, IsDialogMessageW, PeekMessageW, PostQuitMessage, RegisterClassW,
        SetForegroundWindow, ShowWindow, TranslateMessage, CS_HREDRAW, CS_VREDRAW, MSG, PM_REMOVE,
        SM_CXSCREEN, SM_CYSCREEN, SW_SHOW, WINDOW_EX_STYLE, WM_CLOSE, WM_DESTROY, WM_PAINT,
        WM_QUIT, WNDCLASSW, WS_CAPTION, WS_OVERLAPPED, WS_SYSMENU,
    };

    const W: i32 = 600;
    const H: i32 = 286;

    pub fn scale() -> f32 {
        (unsafe { GetDpiForSystem() } as f32 / 96.0).clamp(1.0, 3.0)
    }

    unsafe extern "system" fn wndproc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_CLOSE => {
                QUIT.store(true, std::sync::atomic::Ordering::Relaxed);
                let _ = unsafe { DestroyWindow(hwnd) };
                LRESULT(0)
            }
            WM_DESTROY => {
                unsafe { PostQuitMessage(0) };
                LRESULT(0)
            }
            WM_PAINT => {
                unsafe { paint(hwnd) };
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }

    unsafe fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
        COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16)
    }

    unsafe fn text(
        hdc: HDC,
        value: &str,
        rect: &mut RECT,
        size: i32,
        bold: bool,
        color: COLORREF,
        center: bool,
    ) {
        let s = scale();
        let font = unsafe {
            CreateFontW(
                -size * s as i32,
                0,
                0,
                0,
                if bold { 700 } else { 400 },
                0,
                0,
                0,
                1,
                0,
                0,
                5,
                0,
                windows::core::w!("Microsoft JhengHei UI"),
            )
        };
        let old = unsafe { windows::Win32::Graphics::Gdi::SelectObject(hdc, font) };
        unsafe {
            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, color);
            let mut wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
            DrawTextW(
                hdc,
                &mut wide,
                rect,
                DRAW_TEXT_FORMAT(if center { 0x0001 } else { 0x0000 }),
            );
            windows::Win32::Graphics::Gdi::SelectObject(hdc, old);
            DeleteObject(font);
        }
    }

    unsafe fn paint(hwnd: HWND) {
        let mut ps = PAINTSTRUCT::default();
        let hdc = unsafe { BeginPaint(hwnd, &mut ps) };
        let mut client = RECT::default();
        unsafe { windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut client) };
        let s = scale();
        let bg = unsafe { CreateSolidBrush(rgb(12, 17, 31)) };
        unsafe { FillRect(hdc, &client, bg) };
        unsafe { DeleteObject(bg) };

        // 頂部細線與品牌色。
        let bar = RECT {
            left: 0,
            top: 0,
            right: client.right,
            bottom: (3.0 * s) as i32,
        };
        let accent = unsafe { CreateSolidBrush(rgb(74, 222, 164)) };
        unsafe { FillRect(hdc, &bar, accent) };
        unsafe { DeleteObject(accent) };

        let pad = (34.0 * s) as i32;
        let mut title = RECT {
            left: pad,
            top: (30.0 * s) as i32,
            right: client.right - pad,
            bottom: (72.0 * s) as i32,
        };
        unsafe { text(hdc, "鬼島", &mut title, 30, true, rgb(244, 248, 255), false) };

        let mut subtitle = RECT {
            left: pad,
            top: (76.0 * s) as i32,
            right: client.right - pad,
            bottom: (102.0 * s) as i32,
        };
        let status = STATUS.lock().map(|v| v.clone()).unwrap_or_default();
        unsafe {
            text(
                hdc,
                &status,
                &mut subtitle,
                16,
                true,
                rgb(150, 235, 200),
                false,
            )
        };

        let mut note = RECT {
            left: pad,
            top: (106.0 * s) as i32,
            right: client.right - pad,
            bottom: (128.0 * s) as i32,
        };
        unsafe {
            text(
                hdc,
                if is_dev() {
                    "開發版啟動器：每次開啟自動更新到最新開發版。"
                } else {
                    "安裝包不含主程式，下載完成後會自動開啟。"
                },
                &mut note,
                13,
                false,
                rgb(146, 158, 181),
                false,
            )
        };

        // 進度槽。
        let track = RECT {
            left: pad,
            top: (152.0 * s) as i32,
            right: client.right - pad,
            bottom: (162.0 * s) as i32,
        };
        let track_brush = unsafe { CreateSolidBrush(rgb(31, 40, 61)) };
        unsafe { FillRect(hdc, &track, track_brush) };
        unsafe { DeleteObject(track_brush) };

        let percent = PERCENT.load(std::sync::atomic::Ordering::Relaxed).min(100);
        let fill = RECT {
            left: track.left,
            top: track.top,
            right: track.left + ((track.right - track.left) as u32 * percent / 100) as i32,
            bottom: track.bottom,
        };
        let color = if FAILED.load(std::sync::atomic::Ordering::Relaxed) {
            rgb(244, 114, 114)
        } else {
            rgb(74, 222, 164)
        };
        let fill_brush = unsafe { CreateSolidBrush(color) };
        unsafe { FillRect(hdc, &fill, fill_brush) };
        unsafe { DeleteObject(fill_brush) };

        let mut percent_text = RECT {
            left: pad,
            top: (170.0 * s) as i32,
            right: client.right - pad,
            bottom: (190.0 * s) as i32,
        };
        unsafe {
            text(
                hdc,
                &format!("{percent}%"),
                &mut percent_text,
                12,
                true,
                rgb(203, 213, 225),
                false,
            )
        };

        let mut detail = RECT {
            left: pad,
            top: (198.0 * s) as i32,
            right: client.right - pad,
            bottom: (220.0 * s) as i32,
        };
        let detail_text = DETAIL.lock().map(|v| v.clone()).unwrap_or_default();
        unsafe {
            text(
                hdc,
                &detail_text,
                &mut detail,
                12,
                false,
                rgb(128, 142, 166),
                false,
            )
        };

        let mut footer = RECT {
            left: pad,
            top: (244.0 * s) as i32,
            right: client.right - pad,
            bottom: (268.0 * s) as i32,
        };
        unsafe {
            text(
                hdc,
                "下載自 GitHub · JCnTEA/kuidao-tw",
                &mut footer,
                11,
                false,
                rgb(94, 108, 132),
                false,
            )
        };

        unsafe { EndPaint(hwnd, &ps) };
    }

    pub fn create() -> Option<HWND> {
        let instance = unsafe { GetModuleHandleW(None) }.ok()?;
        let class_name = windows::core::w!("KuidaoBootstrapWindow");
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            lpszClassName: class_name,
            ..Default::default()
        };
        unsafe { RegisterClassW(&wc) };
        let s = scale();
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: (W as f32 * s) as i32,
            bottom: (H as f32 * s) as i32,
        };
        unsafe {
            AdjustWindowRectExForDpi(
                &mut rect,
                WS_CAPTION | WS_OVERLAPPED | WS_SYSMENU,
                false,
                WINDOW_EX_STYLE(0),
                (s * 96.0) as u32,
            )
        };
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        let x = (unsafe { GetSystemMetrics(SM_CXSCREEN) } - width) / 2;
        let y = (unsafe { GetSystemMetrics(SM_CYSCREEN) } - height) / 2 - (24.0 * s) as i32;
        let title = windows::core::HSTRING::from(channel_title());
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class_name,
                windows::core::PCWSTR(title.as_ptr()),
                WS_CAPTION | WS_OVERLAPPED | WS_SYSMENU,
                x,
                y,
                width,
                height,
                None,
                None,
                instance,
                None,
            )
        }
        .ok()?;
        unsafe {
            ShowWindow(hwnd, SW_SHOW);
            UpdateWindow(hwnd);
            SetForegroundWindow(hwnd);
        }
        Some(hwnd)
    }

    pub fn pump() {
        let mut msg = MSG::default();
        loop {
            let found = unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) };
            if !found.as_bool() {
                break;
            }
            if msg.message == WM_QUIT {
                QUIT.store(true, std::sync::atomic::Ordering::Relaxed);
                break;
            }
            unsafe {
                if !IsDialogMessageW(msg.hwnd, &msg).as_bool() {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
    }

    pub fn repaint() {
        if let Some(hwnd) = window() {
            unsafe { windows::Win32::Graphics::Gdi::InvalidateRect(hwnd, None, false) };
            pump();
        }
    }

    static HWND_ATOM: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

    pub fn set_window(hwnd: HWND) {
        HWND_ATOM.store(hwnd.0 as isize, std::sync::atomic::Ordering::Relaxed);
    }

    fn window() -> Option<HWND> {
        let raw = HWND_ATOM.load(std::sync::atomic::Ordering::Relaxed);
        (raw != 0).then(|| HWND(raw as *mut std::ffi::c_void))
    }

    pub fn message(text: &str, title: &str, flags: u32) -> u32 {
        let text = windows::core::HSTRING::from(text);
        let title = windows::core::HSTRING::from(title);
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
                window().unwrap_or_default(),
                &text,
                &title,
                windows::Win32::UI::WindowsAndMessaging::MESSAGEBOX_STYLE(flags),
            )
            .0 as u32
        }
    }

    pub fn run_loop<F: FnOnce()>(work: F) {
        work();
        let mut msg = MSG::default();
        while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
            unsafe {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("鬼島啟動器僅支援 Windows。");
}

#[cfg(windows)]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let force_update = args
        .iter()
        .any(|arg| arg == "--update" || arg == "--repair");
    let no_launch = args.iter().any(|arg| arg == "--no-launch");
    let forwarded: Vec<String> = args
        .into_iter()
        .filter(|arg| !matches!(arg.as_str(), "--update" | "--repair" | "--no-launch"))
        .collect();
    let install_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    let app = install_dir.join(app_name());

    if app.is_file() && !force_update {
        // 正式版：主程式存在就直接開，日常開啟不經過網路。
        // 開發版：每次啟動都先問一次最新開發版，版本不同就自動換掉全部檔案（程式＋素材）。
        let needs_update = is_dev() && dev_update_needed(&install_dir);
        if !needs_update {
            if !no_launch {
                launch(&app, &install_dir, &forwarded);
            }
            return;
        }
        // 開發版要換整包檔案前，先把自己正在跑的舊開發版關掉，避免 exe 被佔用換不掉。
        close_running_app();
    }

    let Some(hwnd) = gui::create() else {
        return;
    };
    gui::set_window(hwnd);
    set_status(
        "正在準備下載",
        "安裝包只包含啟動器，主程式會在背景補齊。",
        0,
    );
    gui::run_loop(|| {
        loop {
            match update(&install_dir, &app) {
                Ok(()) => {
                    set_status("下載完成", "正在開啟鬼島…", 100);
                    gui::repaint();
                    if !no_launch {
                        launch(&app, &install_dir, &forwarded);
                    }
                    unsafe { windows::Win32::UI::WindowsAndMessaging::DestroyWindow(hwnd) };
                    break;
                }
                Err(error) => {
                    if QUIT.load(Ordering::Relaxed) {
                        break;
                    }
                    FAILED.store(true, Ordering::Relaxed);
                    set_status("下載失敗", &error, PERCENT.load(Ordering::Relaxed));
                    gui::repaint();
                    if app.is_file() {
                        // MB_YESNO | MB_ICONERROR = 0x00000014，IDYES = 6
                        let choice = gui::message(
                            &format!("{error}\n\n目前已安裝的版本仍可開啟。要改用已安裝版本嗎？"),
                            channel_title(),
                            0x0000_0014,
                        );
                        if choice == 6 {
                            FAILED.store(false, Ordering::Relaxed);
                            launch(&app, &install_dir, &forwarded);
                            unsafe { windows::Win32::UI::WindowsAndMessaging::DestroyWindow(hwnd) };
                            break;
                        }
                    } else {
                        // MB_RETRYCANCEL | MB_ICONERROR = 0x00000015，IDRETRY = 4
                        let choice = gui::message(
                            &format!("{error}\n\n請確認網路連線後再試一次。"),
                            channel_title(),
                            0x0000_0015,
                        );
                        if choice == 4 {
                            continue;
                        }
                    }
                    unsafe { windows::Win32::UI::WindowsAndMessaging::DestroyWindow(hwnd) };
                    break;
                }
            }
        }
    });
}

fn launch(app: &Path, install_dir: &Path, args: &[String]) {
    let mut command = std::process::Command::new(app);
    command.current_dir(install_dir).args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use windows::Win32::System::Threading::CREATE_NEW_PROCESS_GROUP;
        command.creation_flags(CREATE_NEW_PROCESS_GROUP.0);
    }
    let _ = command.spawn();
}

fn update(install_dir: &Path, app: &Path) -> Result<(), String> {
    set_status("正在查詢最新版本", "連線 GitHub Release…", 1);
    let release = fetch_release()?;
    if QUIT.load(Ordering::Relaxed) {
        return Err("已取消".into());
    }
    let work = cache_dir()?;
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|_| "無法建立下載暫存資料夾".to_owned())?;
    let result = (|| {
        set_status(
            format!("正在下載 {}", release.version),
            format!("{} MB · 下載後會自動解壓", release.size / 1024 / 1024),
            2,
        );
        let zip = work.join("update.zip");
        download(&release, &zip)?;
        if QUIT.load(Ordering::Relaxed) {
            return Err("已取消".into());
        }
        set_status("正在驗證檔案", "確認下載內容完整且未被替換…", 92);
        verify(&zip, &release)?;
        set_status("正在解壓縮", "展開主程式與素材…", 94);
        let stage = work.join("stage");
        extract(&zip, &stage)?;
        let source = find_app(&stage).ok_or_else(|| format!("下載內容中找不到 {}", app_name()))?;
        set_status("正在安裝", "寫入安裝資料夾…", 97);
        copy_tree(source.parent().unwrap_or(&stage), install_dir)?;
        if !app.is_file() {
            return Err("安裝後仍找不到主程式，請重新安裝".into());
        }
        set_status("安裝完成", "正在開啟鬼島…", 100);
        if is_dev() {
            let _ = std::fs::write(install_dir.join("dev-version.txt"), &release.version);
        }
        update_uninstall_version(&release.version);
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&work);
    }
    result
}

struct Release {
    version: String,
    url: String,
    size: u64,
    hash: String,
}

fn fetch_release() -> Result<Release, String> {
    if is_dev() {
        return fetch_dev_release();
    }
    let response = agent(20)?
        .get(API)
        .set(
            "User-Agent",
            concat!("KuidaoBootstrap/", env!("CARGO_PKG_VERSION")),
        )
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|error| match error {
            ureq::Error::Status(403 | 429, _) => "更新服務暫時限流，請稍後再試".to_owned(),
            ureq::Error::Status(404, _) => "找不到可用的正式版本".to_owned(),
            _ => "無法連線更新服務，請檢查網路或代理".to_owned(),
        })?;
    let value: serde_json::Value = serde_json::from_reader(response.into_reader())
        .map_err(|_| "更新資訊格式異常".to_owned())?;
    if value["draft"].as_bool() == Some(true) || value["prerelease"].as_bool() == Some(true) {
        return Err("最新版本尚未正式發布".into());
    }
    let tag = value["tag_name"]
        .as_str()
        .ok_or_else(|| "更新資訊缺少版本號".to_owned())?;
    let version = tag.trim_start_matches('v').to_owned();
    let name = format!("Kuidao-v{version}-windows-x64.zip");
    let expected = format!("{REPOSITORY}/releases/download/{tag}/{name}");
    let asset = value["assets"]
        .as_array()
        .and_then(|assets| {
            assets
                .iter()
                .find(|asset| asset["name"].as_str() == Some(&name))
        })
        .ok_or_else(|| "最新版本沒有提供 Windows 下載包".to_owned())?;
    let url = asset["browser_download_url"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let size = asset["size"].as_u64().unwrap_or_default();
    let hash = asset["digest"]
        .as_str()
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .unwrap_or_default()
        .to_ascii_lowercase();
    if url != expected || !url.starts_with("https://") || size == 0 || size > 200 * 1024 * 1024 {
        return Err("下載網址或檔案大小不符".into());
    }
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("更新資訊缺少校驗碼".into());
    }
    Ok(Release {
        version: tag.to_owned(),
        url,
        size,
        hash,
    })
}

/// 開發版發布：`releases` 清單裡最新一個 prerelease、標籤結尾是 `-tw-dev`，
/// 資產固定叫 `Kuidao-Dev-v<版本>-tw-windows-x64.zip`，下載後整包換上去（程式＋素材）。
fn fetch_dev_release() -> Result<Release, String> {
    let response = agent(20)?
        .get(API_DEV)
        .set(
            "User-Agent",
            concat!("KuidaoDevBootstrap/", env!("CARGO_PKG_VERSION")),
        )
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|error| match error {
            ureq::Error::Status(403 | 429, _) => "更新服務暫時限流，請稍後再試".to_owned(),
            _ => "無法連線更新服務，請檢查網路或代理".to_owned(),
        })?;
    let list: serde_json::Value = serde_json::from_reader(response.into_reader())
        .map_err(|_| "更新資訊格式異常".to_owned())?;
    let releases = list
        .as_array()
        .ok_or_else(|| "更新資訊格式異常".to_owned())?;
    for value in releases {
        if value["draft"].as_bool() == Some(true) {
            continue;
        }
        let Some(tag) = value["tag_name"].as_str() else {
            continue;
        };
        if !tag.ends_with("-tw-dev") {
            continue;
        }
        let stem = tag.trim_start_matches('v').trim_end_matches("-tw-dev");
        if stem.is_empty() {
            continue;
        }
        let name = format!("Kuidao-Dev-v{stem}-tw-windows-x64.zip");
        let expected = format!("{REPOSITORY}/releases/download/{tag}/{name}");
        let Some(asset) = value["assets"].as_array().and_then(|assets| {
            assets
                .iter()
                .find(|asset| asset["name"].as_str() == Some(&name))
        }) else {
            continue;
        };
        let url = asset["browser_download_url"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let size = asset["size"].as_u64().unwrap_or_default();
        let hash = asset["digest"]
            .as_str()
            .and_then(|digest| digest.strip_prefix("sha256:"))
            .unwrap_or_default()
            .to_ascii_lowercase();
        if url != expected || !url.starts_with("https://") || size == 0 || size > 200 * 1024 * 1024
        {
            return Err("開發版下載網址或檔案大小不符".into());
        }
        if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("開發版缺少校驗碼".into());
        }
        return Ok(Release {
            version: tag.to_owned(),
            url,
            size,
            hash,
        });
    }
    Err("目前沒有可用的開發版".into())
}

/// 換版前先結束目前正在執行的開發版（只關程式，不動設定與歌詞庫）。
fn close_running_app() {
    let _ = std::process::Command::new("taskkill")
        .args(["/F", "/T", "/IM", app_name()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    std::thread::sleep(std::time::Duration::from_millis(500));
}

/// 開發版版本記在這台電腦的安裝資料夾；跟線上最新開發版不同就換新版。
fn dev_update_needed(install_dir: &Path) -> bool {
    let install_dir_ref = install_dir.to_path_buf();
    let local = std::fs::read_to_string(install_dir_ref.join("dev-version.txt"))
        .ok()
        .map(|text| text.trim().to_owned())
        .unwrap_or_default();
    if local.is_empty() {
        return true;
    }
    match fetch_dev_release() {
        Ok(release) => release.version != local,
        // 離線或服務掛掉時照常開目前版本，不要卡在啟動器。
        Err(_) => false,
    }
}

fn agent(timeout: u64) -> Result<ureq::Agent, String> {
    let mut builder = ureq::AgentBuilder::new()
        .try_proxy_from_env(true)
        .timeout_connect(Duration::from_secs(20))
        .timeout_read(Duration::from_secs(timeout))
        .timeout_write(Duration::from_secs(timeout));
    let proxy = [
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
        "HTTP_PROXY",
        "http_proxy",
    ]
    .iter()
    .find_map(|key| {
        std::env::var(key)
            .ok()
            .filter(|value| !value.trim().is_empty())
    });
    if let Some(proxy) = proxy {
        let proxy = ureq::Proxy::new(proxy.trim()).map_err(|_| "代理設定格式不正確".to_owned())?;
        builder = builder.proxy(proxy);
    }
    Ok(builder.build())
}

fn download(release: &Release, target: &Path) -> Result<(), String> {
    let response = agent(30)?
        .get(&release.url)
        .call()
        .map_err(|_| "下載連線中斷，請重試".to_owned())?;
    if !response.get_url().starts_with("https://") {
        return Err("下載未使用安全連線".into());
    }
    let mut input = response.into_reader();
    let mut output = std::fs::File::create(target).map_err(|_| "無法寫入下載暫存檔".to_owned())?;
    let mut hash = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buffer = [0u8; 65536];
    let mut total = 0u64;
    loop {
        if QUIT.load(Ordering::Relaxed) {
            return Err("已取消".into());
        }
        let length = input
            .read(&mut buffer)
            .map_err(|_| "下載逾時或中斷，請重試".to_owned())?;
        if length == 0 {
            break;
        }
        total += length as u64;
        if total > release.size {
            return Err("下載檔案大小不符".into());
        }
        output
            .write_all(&buffer[..length])
            .map_err(|_| "無法寫入下載檔，請確認磁碟空間".to_owned())?;
        hash.update(&buffer[..length]);
        let percent = 2 + (total as f32 / release.size as f32 * 88.0) as u32;
        set_status(
            format!("正在下載 {}", release.version),
            format!(
                "{:.1} MB / {:.1} MB",
                total as f64 / 1048576.0,
                release.size as f64 / 1048576.0
            ),
            percent,
        );
    }
    if total != release.size {
        return Err("下載檔案不完整，請重試".into());
    }
    if QUIT.load(Ordering::Relaxed) {
        return Err("已取消".into());
    }
    Ok(())
}

fn verify(path: &Path, release: &Release) -> Result<(), String> {
    let mut file = std::fs::File::open(path).map_err(|_| "無法讀取下載檔".to_owned())?;
    let mut hash = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buffer = [0u8; 65536];
    loop {
        let length = file
            .read(&mut buffer)
            .map_err(|_| "無法讀取下載檔".to_owned())?;
        if length == 0 {
            break;
        }
        hash.update(&buffer[..length]);
    }
    let digest: String = hash
        .finish()
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if digest != release.hash {
        return Err("下載內容校驗失敗，請重試".into());
    }
    Ok(())
}

/// 背景解壓時不要閃出主控台視窗。
fn hidden_command(program: &Path) -> std::process::Command {
    let mut command = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

fn extract(zip: &Path, stage: &Path) -> Result<(), String> {
    std::fs::create_dir_all(stage).map_err(|_| "無法建立解壓資料夾".to_owned())?;
    let system_root = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    // Windows 內建 tar 對部分 ZIP（尤其含中文檔名）會回報失敗；
    // Expand-Archive 在部分 .NET 版本會呼叫不存在的三參數多載。
    // 因此直接用 .NET ZipArchive 逐筆解壓，保留 Unicode 檔名並自行檢查路徑。
    let powershell = system_root.join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let script = r#"$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip=[System.IO.Compression.ZipFile]::OpenRead($env:KUIDAO_ZIP)
try {
  $root=[System.IO.Path]::GetFullPath($env:KUIDAO_STAGE)
  foreach($entry in $zip.Entries){
    $name=$entry.FullName.Replace('\','/')
    if($name -match '(^/|^[A-Za-z]:|(^|/)\.\.(/|$))'){throw 'ZIP 內含異常路徑'}
    $dest=[System.IO.Path]::GetFullPath([System.IO.Path]::Combine($root,$name))
    if(-not $dest.StartsWith($root,[System.StringComparison]::OrdinalIgnoreCase)){throw 'ZIP 路徑超出解壓資料夾'}
    if($entry.Name -eq ''){
      New-Item -ItemType Directory -Force -Path $dest | Out-Null
    } else {
      New-Item -ItemType Directory -Force -Path (Split-Path -Parent $dest) | Out-Null
      if(Test-Path -LiteralPath $dest){Remove-Item -LiteralPath $dest -Force}
      [System.IO.Compression.ZipFileExtensions]::ExtractToFile($entry,$dest)
    }
  }
} finally {$zip.Dispose()}"#;
    let status = hidden_command(&powershell)
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .env("KUIDAO_ZIP", zip)
        .env("KUIDAO_STAGE", stage)
        .status()
        .map_err(|_| "無法啟動 Windows 解壓工具".to_owned())?;
    if status.success() {
        return Ok(());
    }
    let tar = system_root.join("System32/tar.exe");
    let status = hidden_command(&tar)
        .arg("-xf")
        .arg(zip)
        .arg("-C")
        .arg(stage)
        .status()
        .map_err(|_| "無法啟動 Windows 解壓工具".to_owned())?;
    if !status.success() {
        return Err("解壓失敗，下載檔可能損壞或系統解壓元件無法使用".into());
    }
    Ok(())
}

fn find_app(root: &Path) -> Option<PathBuf> {
    let mut stack = vec![root.to_path_buf()];
    let mut found = Vec::new();
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                stack.push(path);
            } else if path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case(app_name()))
            {
                found.push(path);
            }
        }
    }
    (found.len() == 1).then(|| found.remove(0))
}

fn copy_tree(source: &Path, target: &Path) -> Result<(), String> {
    std::fs::create_dir_all(target).map_err(|_| "無法建立安裝資料夾".to_owned())?;
    let mut stack = vec![source.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|_| "無法讀取解壓內容".to_owned())?;
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if meta.file_type().is_symlink() {
                continue;
            }
            let relative = path
                .strip_prefix(source)
                .map_err(|_| "解壓路徑異常".to_owned())?;
            let destination = target.join(relative);
            if meta.is_dir() {
                std::fs::create_dir_all(&destination)
                    .map_err(|_| "無法建立安裝子資料夾".to_owned())?;
                stack.push(path);
            } else if meta.is_file() {
                if let Some(parent) = destination.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|_| "無法建立安裝子資料夾".to_owned())?;
                }
                std::fs::copy(&path, &destination)
                    .map_err(|_| format!("無法寫入 {}", destination.display()))?;
            }
        }
    }
    Ok(())
}

fn cache_dir() -> Result<PathBuf, String> {
    let root = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("TEMP").map(PathBuf::from))
        .ok_or_else(|| "找不到暫存資料夾".to_owned())?
        .join(if is_dev() {
            "KuidaoDev/Bootstrap"
        } else {
            "Kuidao/Bootstrap"
        });
    std::fs::create_dir_all(&root).map_err(|_| "無法建立暫存資料夾".to_owned())?;
    Ok(root.join(std::process::id().to_string()))
}

#[cfg(windows)]
fn update_uninstall_version(version: &str) {
    use windows::core::w;
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE,
        KEY_SET_VALUE, REG_SZ,
    };
    let key_path = if is_dev() {
        w!("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\KuidaoDev")
    } else {
        w!("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Kuidao")
    };
    let root = if is_dev() {
        HKEY_CURRENT_USER
    } else {
        HKEY_LOCAL_MACHINE
    };
    let mut key = HKEY::default();
    if unsafe { RegOpenKeyExW(root, key_path, 0, KEY_SET_VALUE, &mut key) } != ERROR_SUCCESS {
        return;
    }
    let mut value: Vec<u8> = version
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();
    value.extend_from_slice(&[0, 0]);
    unsafe {
        let _ = RegSetValueExW(key, w!("DisplayVersion"), 0, REG_SZ, Some(&value));
        let _ = RegCloseKey(key);
    }
}

#[cfg(windows)]
fn repaint() {
    gui::repaint();
}

#[cfg(not(windows))]
fn repaint() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn bootstrap_extracts_unicode_zip_without_tar() {
        if !cfg!(windows) {
            return;
        }
        let root = std::env::temp_dir().join(format!(
            "kuidao-bootstrap-extract-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let payload = root.join("payload");
        std::fs::create_dir_all(&payload).unwrap();
        std::fs::write(payload.join(APP_EXE), b"app").unwrap();
        std::fs::write(payload.join("收集岛仔诊断.cmd"), b"diag").unwrap();
        let zip = root.join("update.zip");
        let system_root = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let powershell = system_root.join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let status = Command::new(powershell)
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                "Compress-Archive -Path \"$env:KUIDAO_SRC\\*\" -DestinationPath $env:KUIDAO_ZIP -Force",
            ])
            .env("KUIDAO_SRC", &payload)
            .env("KUIDAO_ZIP", &zip)
            .status()
            .unwrap();
        assert!(status.success(), "Compress-Archive 失敗");

        let stage = root.join("stage");
        extract(&zip, &stage).expect("ZIP 應該可以直接解壓");
        assert_eq!(std::fs::read(stage.join(APP_EXE)).unwrap(), b"app".to_vec());
        assert!(stage.join("收集岛仔诊断.cmd").is_file());
        let _ = std::fs::remove_dir_all(&root);
    }
}

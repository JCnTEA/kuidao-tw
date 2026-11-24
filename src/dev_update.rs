//! 開發版自動更新：主線每次推送就出開發版包，這裡在背景確認並靜默換版，不打擾使用者。
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const RELEASES_API: &str = "https://api.github.com/repos/JCnTEA/kuidao-tw/releases?per_page=10";
const ASSET_NAME: &str = "Kuidao-Dev.exe";
const START_DELAY: Duration = Duration::from_secs(20);
const MAX_PACKAGE: u64 = 200 * 1024 * 1024;

static STARTED: AtomicBool = AtomicBool::new(false);

struct Asset { url: String, size: u64, hash: String }

/// 只有開發版（建置時帶 KUIDAO_CHANNEL=dev）才自動更新；正式版仍走原本需確認的流程。
pub(crate) fn enabled() -> bool {
    if cfg!(test) { return false; }
    matches!(option_env!("KUIDAO_CHANNEL"), Some("dev"))
}

/// 啟動後延遲一次檢查；有新版就下載、換版、自動重開。
pub(crate) fn tick(ctx: &eframe::egui::Context) {
    if !enabled() || STARTED.swap(true, Ordering::Relaxed) { return; }
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        std::thread::sleep(START_DELAY);
        if let Ok(true) = refresh() {
            ctx.send_viewport_cmd_to(eframe::egui::ViewportId::ROOT, eframe::egui::ViewportCommand::Close);
        }
    });
}

/// 跟正式版更新一樣支援系統代理，避免開發版在代理環境下抓不到包。
fn agent() -> Result<ureq::Agent, String> {
    let proxy = ["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy", "HTTP_PROXY", "http_proxy"]
        .iter().find_map(|key| std::env::var(key).ok().filter(|value| !value.trim().is_empty()))
        .or_else(crate::quota::system_proxy);
    let mut builder = ureq::AgentBuilder::new().try_proxy_from_env(false)
        .timeout_connect(Duration::from_secs(20))
        .timeout_read(Duration::from_secs(30)).timeout_write(Duration::from_secs(30));
    if let Some(proxy) = proxy {
        builder = builder.proxy(ureq::Proxy::new(proxy).map_err(|_| "代理設定無效".to_owned())?);
    }
    Ok(builder.build())
}

fn refresh() -> Result<bool, String> {
    let agent = agent()?;
    let response = agent.get(RELEASES_API)
        .set("User-Agent", concat!("Kuidao/", env!("CARGO_PKG_VERSION")))
        .set("Accept", "application/vnd.github+json")
        .call().map_err(|_| "開發版更新檢查失敗".to_owned())?;
    let list: serde_json::Value = serde_json::from_reader(response.into_reader().take(512 * 1024))
        .map_err(|_| "開發版更新資訊格式異常".to_owned())?;
    let Some(asset) = dev_asset(&list) else { return Ok(false) };
    if asset.hash == local_hash()? { return Ok(false); }
    let work = work_dir()?;
    let file = work.join("update.exe");
    if let Err(error) = download(&agent, &asset, &file) {
        let _ = std::fs::remove_dir_all(&work);
        return Err(error);
    }
    let target = std::env::current_exe().map_err(|_| "找不到目前程式".to_owned())?;
    std::fs::write(work.join("apply.ps1"), concat!("\u{feff}", include_str!("../installer/dev-update.ps1")))
        .map_err(|_| "無法準備更新程式".to_owned())?;
    launch(&work, &target)?;
    Ok(true)
}

/// 從發布清單挑出最新的開發版，並確認它帶著可下載、可校驗的單檔程式。
fn dev_asset(list: &serde_json::Value) -> Option<Asset> {
    let releases = list.as_array()?;
    for release in releases {
        if release["draft"].as_bool() != Some(false) || release["prerelease"].as_bool() != Some(true) { continue; }
        let tag = release["tag_name"].as_str()?;
        if !tag.contains("-dev") { continue; }
        let asset = release["assets"].as_array()?.iter().find(|asset| asset["name"] == ASSET_NAME)?;
        let url = asset["browser_download_url"].as_str()?;
        if !url.starts_with("https://") { continue; }
        let size = asset["size"].as_u64()?;
        if size == 0 || size > MAX_PACKAGE { continue; }
        let hash = asset["digest"].as_str()?.strip_prefix("sha256:")?.to_ascii_lowercase();
        if hash.len() != 64 || !hash.bytes().all(|c| c.is_ascii_hexdigit()) { continue; }
        return Some(Asset { url: url.to_owned(), size, hash });
    }
    None
}

fn local_hash() -> Result<String, String> {
    let bytes = std::fs::read(std::env::current_exe().map_err(|_| "找不到目前程式".to_owned())?)
        .map_err(|_| "無法讀取目前程式".to_owned())?;
    let digest = ring::digest::digest(&ring::digest::SHA256, &bytes);
    Ok(digest.as_ref().iter().map(|byte| format!("{byte:02x}")).collect())
}

fn work_dir() -> Result<PathBuf, String> {
    let root = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
        .ok_or("無法找到更新快取目錄")?.join("Kuidao/DevUpdate");
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default().as_nanos();
    let work = root.join(format!("{}-{stamp}", std::process::id()));
    std::fs::create_dir_all(&work).map_err(|_| "無法建立更新快取".to_owned())?;
    Ok(work)
}

fn download(agent: &ureq::Agent, asset: &Asset, path: &Path) -> Result<(), String> {
    let response = agent.get(&asset.url).call().map_err(|_| "連線下載伺服器失敗".to_owned())?;
    if !response.get_url().starts_with("https://") { return Err("更新下載未使用安全連線".into()); }
    let mut input = response.into_reader();
    let mut output = std::fs::File::create(path).map_err(|_| "無法儲存更新檔".to_owned())?;
    let mut hash = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buffer = [0u8; 65536];
    let mut total = 0u64;
    loop {
        let length = input.read(&mut buffer).map_err(|_| "下載中斷，請稍後重試".to_owned())?;
        if length == 0 { break; }
        total += length as u64;
        if total > asset.size { return Err("更新檔大小不符".into()); }
        std::io::Write::write_all(&mut output, &buffer[..length]).map_err(|_| "無法儲存更新檔".to_owned())?;
        hash.update(&buffer[..length]);
    }
    let digest: String = hash.finish().as_ref().iter().map(|byte| format!("{byte:02x}")).collect();
    if total != asset.size || digest != asset.hash { return Err("更新檔校驗失敗".into()); }
    Ok(())
}

#[cfg(windows)]
fn launch(work: &Path, target: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let shell = std::env::var_os("SystemRoot").map(PathBuf::from).ok_or("無法找到 Windows 目錄")?
        .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let mut child = std::process::Command::new(shell)
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(work.join("apply.ps1"))
        .arg("-WorkPath").arg(work)
        .arg("-TargetExe").arg(target)
        .arg("-ParentProcessId").arg(std::process::id().to_string())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn().map_err(|_| "無法啟動更新程式".to_owned())?;
    // 腳本先確認快取與目標可用，才讓主程序退出。
    for _ in 0..100 {
        if work.join("ready").exists() { return Ok(()); }
        if child.try_wait().ok().flatten().is_some() { break; }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    let message = std::fs::read_to_string(work.join("error.txt")).unwrap_or_else(|_| "無法準備更新，請稍後重試".into());
    let _ = std::fs::remove_dir_all(work);
    Err(message)
}
#[cfg(not(windows))]
fn launch(_: &Path, _: &Path) -> Result<(), String> { Err("僅支援 Windows".into()) }

//! 開發版啟動前的 6 位數一次性通行碼檢查。
//! 通行碼由「存取碼」產生器寫入 `%APPDATA%\KuidaoGuard\code.json`，30 秒後失效且只能用一次。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Default, serde::Deserialize, serde::Serialize)]
struct Payload {
    #[serde(rename = "Code", default)]
    code: String,
    #[serde(rename = "Expires", default)]
    expires: i64,
    #[serde(rename = "Used", default)]
    used: bool,
}

fn code_path() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("APPDATA")?;
    Some(std::path::PathBuf::from(base).join("KuidaoGuard").join("code.json"))
}

fn verify(input: &str) -> Result<(), String> {
    let path = code_path().ok_or("找不到 APPDATA，無法讀取通行碼")?;
    let text = std::fs::read_to_string(&path).map_err(|_| "尚未產生通行碼，請先開啟「存取碼」".to_owned())?;
    let mut payload: Payload = serde_json::from_str(&text).map_err(|_| "通行碼檔案異常".to_owned())?;
    let now = chrono::Utc::now().timestamp();
    if payload.used { return Err("此通行碼已使用過，請重新產生".into()); }
    if now > payload.expires { return Err("通行碼已過期，請重新產生".into()); }
    if payload.code != input.trim() { return Err("通行碼不正確".into()); }
    payload.used = true;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(&payload).map_err(|_| "無法寫入通行碼".to_owned())?)
        .map_err(|_| "無法寫入通行碼".to_owned())?;
    std::fs::rename(&tmp, &path).map_err(|_| "無法寫入通行碼".to_owned())?;
    Ok(())
}

/// 顯示解鎖視窗；通過回傳 true，關閉視窗視為放棄。
#[allow(dead_code)]
pub fn require_code() -> bool {
    let passed = Arc::new(AtomicBool::new(false));
    let flag = passed.clone();
    let mut input = String::new();
    let mut message = String::new();
    let opts = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("鬼島 存取碼")
            .with_inner_size([320.0, 210.0])
            .with_resizable(false),
        ..Default::default()
    };
    let _ = eframe::run_simple_native("鬼島 存取碼", opts, move |ctx, _frame| {
        eframe::egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(6.0);
            ui.vertical_centered(|ui| {
                ui.heading("輸入 6 位數通行碼");
                ui.label("請在「存取碼」產生器按下產生，30 秒內輸入");
                ui.add_space(8.0);
                ui.add(eframe::egui::TextEdit::singleline(&mut input).desired_width(160.0).hint_text("000000"));
                ui.add_space(8.0);
                if ui.button("解鎖").clicked() {
                    match verify(&input) {
                        Ok(()) => { flag.store(true, Ordering::Relaxed); ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Close); }
                        Err(error) => { message = error; input.clear(); }
                    }
                }
                if !message.is_empty() { ui.colored_label(eframe::egui::Color32::from_rgb(255, 138, 138), &message); }
            });
        });
    });
    passed.load(Ordering::Relaxed)
}

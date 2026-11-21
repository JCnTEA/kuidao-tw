//! 可外掛的吉祥物動畫包。
//!
//! 每個形象都可以有自己的動畫，不再共用原始版的表情序列：
//! `%APPDATA%\Kuidao\mascot-packs\<形象>\pack.json` 放外掛包，
//! 沒有外掛時才使用程式內建的該形象動畫。石虎內建「待機」呼吸包。
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eframe::egui::{self, Color32, TextureHandle};

use crate::paint::Card;
use crate::pet::PetState;
use crate::skin_paint::Palette;

const FRAME_W: usize = 144;
const FRAME_H: usize = 90;

#[derive(serde::Deserialize, Clone)]
struct Manifest {
    name: String,
    #[serde(default = "default_frame_size")]
    frame_size: [u32; 2],
    animations: HashMap<String, Animation>,
}

fn default_frame_size() -> [u32; 2] { [FRAME_W as u32, FRAME_H as u32] }

#[derive(serde::Deserialize, Clone)]
struct Animation {
    frames: Vec<String>,
    #[serde(default = "default_fps")]
    fps: f32,
    #[serde(default, rename = "loop")]
    repeat: bool,
}

fn default_fps() -> f32 { 8.0 }

#[derive(Clone)]
struct FrameBytes {
    bytes: &'static [u8],
    file: Option<PathBuf>,
}

impl FrameBytes {
    fn read(&self) -> Option<Vec<u8>> {
        if let Some(path) = &self.file { std::fs::read(path).ok() } else { Some(self.bytes.to_vec()) }
    }
}

struct LoadedPack {
    name: String,
    animations: HashMap<String, (Vec<FrameBytes>, f32, bool)>,
}

/// 單一形象的動畫播放器；只有目前選中的形象會載入紋理。
pub(crate) struct MascotPack {
    key: Option<String>,
    pack: Option<LoadedPack>,
    palette: Option<Palette>,
    textures: HashMap<(String, usize), TextureHandle>,
    failed: bool,
}

impl Default for MascotPack {
    fn default() -> Self {
        Self { key: None, pack: None, palette: None, textures: HashMap::new(), failed: false }
    }
}

impl MascotPack {
    pub fn reload(&mut self) {
        self.key = None;
        self.pack = None;
        self.textures.clear();
        self.failed = false;
    }

    pub fn name(&self) -> &str {
        self.pack.as_ref().map_or("", |pack| pack.name.as_str())
    }

    pub fn is_external(&self) -> bool {
        self.key.as_deref().is_some_and(|key| external_dir(key).join("pack.json").is_file())
    }

    fn ensure(&mut self, key: &str) {
        if self.key.as_deref() == Some(key) { return; }
        self.reload();
        self.key = Some(key.to_string());
        self.pack = load_pack(key);
        self.failed = self.pack.is_none();
    }

    /// 是否由動畫包接手這個形象；沒有動畫包時回傳 false，沿用原本流程。
    pub fn active_for(&mut self, key: &str) -> bool {
        self.ensure(key);
        self.pack.is_some()
    }

    pub fn draw(&mut self, ctx: &egui::Context, c: &Card, cx: f32, state: PetState, time: f64, palette: Option<Palette>) -> bool {
        let Some(key) = self.key.clone() else { return false; };
        if self.palette != palette { self.textures.clear(); self.palette = palette; }
        let Some(pack) = &self.pack else { return false; };
        let name = state_key(state);
        let Some((frames, fps, repeat)) = pack.animations.get(name) else { return false; };
        if frames.is_empty() { return false; }
        let frames = frames.clone();
        let fps = *fps;
        let repeat = *repeat;
        let index = if repeat {
            ((time.max(0.0) * f64::from(fps)) as usize) % frames.len()
        } else {
            ((time.max(0.0) * f64::from(fps)) as usize).min(frames.len() - 1)
        };
        let texture = self.texture(ctx, &key, &frames, index, palette);
        // 位置固定在槽位內；動作只來自逐格動物影格，不再整張圖上下左右位移。
        let rect = c.rect(cx - 24.0, 0.0, 48.0, 30.0);
        c.painter.image(texture.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
        true
    }

    fn texture(&mut self, ctx: &egui::Context, key: &str, frames: &[FrameBytes], index: usize, palette: Option<Palette>) -> &TextureHandle {
        let slot = (key.to_string(), index);
        if !self.textures.contains_key(&slot) {
            let bytes = frames[index].read().unwrap_or_default();
            let mut image = image::load_from_memory(&bytes)
                .map(|image| image.into_rgba8())
                .map(|image| egui::ColorImage::from_rgba_unmultiplied([image.width() as usize, image.height() as usize], image.as_raw()))
                .unwrap_or_else(|_| egui::ColorImage::new([FRAME_W, FRAME_H], Color32::TRANSPARENT));
            if let Some(palette) = palette {
                for pixel in &mut image.pixels {
                    if pixel.a() > 0 { *pixel = crate::skin_paint::map_color(*pixel, &palette); }
                }
            }
            let texture = ctx.load_texture(format!("mascot-pack-{key}-{index}"), image, egui::TextureOptions::LINEAR);
            crate::skin_paint::mark_colored(texture.id());
            self.textures.insert(slot.clone(), texture);
        }
        self.textures.get(&slot).expect("剛載入的動畫紋理")
    }
}

fn state_key(state: PetState) -> &'static str {
    match state {
        PetState::Awake => "awake",
        PetState::Curious => "curious",
        PetState::Waking => "waking",
        PetState::Happy => "happy",
        PetState::Satisfied => "satisfied",
        PetState::Music => "music",
        PetState::Worried => "worried",
        PetState::Focused => "focused",
        PetState::Dozing => "dozing",
        PetState::Sleeping => "sleeping",
    }
}

/// 每個狀態只換動畫影格；影格內的動物自己動，畫面不再整體位移。

pub fn external_dir(key: &str) -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
        .join("Kuidao").join("mascot-packs").join(key)
}

pub fn open_folder(key: &str) {
    let dir = external_dir(key);
    let _ = std::fs::create_dir_all(&dir);
    let readme = dir.join("說明.txt");
    if !readme.exists() {
        let _ = std::fs::write(&readme, "吉祥物動畫包\n\n把 pack.json 與影格 PNG 放在這個資料夾，重新載入後就會取代內建動畫。\n每格建議 144×90，透明背景。animations 可用的鍵：awake、curious、waking、happy、satisfied、music、worried、focused、dozing、sleeping。\n");
    }
    // 第一次打開時放一份可替換的範例，讓外掛包真的能獨立修改。
    if !dir.join("pack.json").exists() {
        if let Some(pack) = builtin_tiger(key) {
            if let Some((frames, _, _)) = pack.animations.get("awake") {
                for (index, frame) in frames.iter().enumerate() {
                    if let Some(bytes) = frame.read() { let _ = std::fs::write(dir.join(format!("idle-{index:02}.png")), bytes); }
                }
            }
            let names: Vec<_> = (0..16).map(|index| format!("idle-{index:02}.png")).collect();
            let manifest = serde_json::json!({
                "name": "石虎・待機（外掛範例）",
                "frame_size": [FRAME_W, FRAME_H],
                "fps": 8,
                "loops": 1,
                "animations": {
                    "awake": {"frames": names, "fps": 8, "loop": true},
                }
            });
            let _ = std::fs::write(dir.join("pack.json"), serde_json::to_vec_pretty(&manifest).unwrap_or_default());
        }
    }
    #[cfg(windows)]
    { let _ = std::process::Command::new("explorer").arg(&dir).spawn(); }
    #[cfg(not(windows))]
    { let _ = &dir; }
}

fn load_pack(key: &str) -> Option<LoadedPack> {
    let dir = external_dir(key);
    let manifest = dir.join("pack.json");
    if manifest.is_file() {
        let text = std::fs::read_to_string(&manifest).ok()?;
        let parsed: Manifest = serde_json::from_str(&text).ok()?;
        // 舊版「開啟資料夾」會自動放一份只有 awake 的範例包；那不是使用者自製動畫，
        // 直接忽略，讓石虎改用內建向量，才看得到各狀態的動作差異。
        if parsed.name.contains("外掛範例") && parsed.animations.len() == 1 { return None; }
        let mut animations = HashMap::new();
        for (name, animation) in parsed.animations {
            if animation.frames.is_empty() { continue; }
            let frames: Vec<_> = animation.frames.iter().map(|name| {
                let file = safe_join(&dir, name);
                FrameBytes { bytes: &[], file }
            }).collect();
            animations.insert(name, (frames, animation.fps.max(1.0), animation.repeat));
        }
        return (!animations.is_empty()).then_some(LoadedPack { name: parsed.name, animations });
    }
    // 內建形象改用向量單線條，不再載入會糊掉的點陣影格；
    // 只有使用者放在 `%APPDATA%\Kuidao\mascot-packs` 的外掛包才走逐格動畫。
    let _ = key;
    None
}

/// 只允許動畫包資料夾內的相對路徑，避免 pack.json 讀到其他位置。
fn safe_join(dir: &Path, name: &str) -> Option<PathBuf> {
    let path = Path::new(name);
    if path.is_absolute() || name.split(['/', '\\']).any(|part| part == "..") { return None; }
    Some(dir.join(path))
}

fn builtin_tiger(key: &str) -> Option<LoadedPack> {
    if key != "tiger" { return None; }
    const IDLE: [&[u8]; 16] = [
        include_bytes!("../assets/pet-species/tiger/idle-00.png"),
        include_bytes!("../assets/pet-species/tiger/idle-01.png"),
        include_bytes!("../assets/pet-species/tiger/idle-02.png"),
        include_bytes!("../assets/pet-species/tiger/idle-03.png"),
        include_bytes!("../assets/pet-species/tiger/idle-04.png"),
        include_bytes!("../assets/pet-species/tiger/idle-05.png"),
        include_bytes!("../assets/pet-species/tiger/idle-06.png"),
        include_bytes!("../assets/pet-species/tiger/idle-07.png"),
        include_bytes!("../assets/pet-species/tiger/idle-08.png"),
        include_bytes!("../assets/pet-species/tiger/idle-09.png"),
        include_bytes!("../assets/pet-species/tiger/idle-10.png"),
        include_bytes!("../assets/pet-species/tiger/idle-11.png"),
        include_bytes!("../assets/pet-species/tiger/idle-12.png"),
        include_bytes!("../assets/pet-species/tiger/idle-13.png"),
        include_bytes!("../assets/pet-species/tiger/idle-14.png"),
        include_bytes!("../assets/pet-species/tiger/idle-15.png"),
    ];
    let frames = || IDLE.iter().map(|bytes| FrameBytes { bytes, file: None }).collect::<Vec<_>>();
    let mut animations = HashMap::new();
    animations.insert("awake".to_string(), (frames(), 8.0, true));
    animations.insert("curious".to_string(), (frames(), 10.0, true));
    animations.insert("waking".to_string(), (frames(), 12.0, false));
    animations.insert("happy".to_string(), (frames(), 14.0, true));
    animations.insert("satisfied".to_string(), (frames(), 12.0, true));
    animations.insert("music".to_string(), (frames(), 10.0, true));
    animations.insert("worried".to_string(), (frames(), 7.0, true));
    animations.insert("focused".to_string(), (frames(), 6.0, true));
    animations.insert("dozing".to_string(), (frames(), 5.0, true));
    animations.insert("sleeping".to_string(), (frames(), 4.0, true));
    Some(LoadedPack { name: "石虎・待機（內建）".into(), animations })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiger_has_its_own_pack_and_others_fall_back() {
        assert!(builtin_tiger("tiger").is_some(), "石虎必須有自己的動畫包");
        for key in ["shiba", "cat", "husky", "golden"] {
            assert!(builtin_tiger(key).is_none(), "其他形象目前走原本流程");
        }
    }

    #[test]
    fn tiger_idle_keeps_the_body_still_and_only_the_tail_moves() {
        let pack = builtin_tiger("tiger").unwrap();
        let (frames, _, _) = &pack.animations["awake"];
        let image = |index: usize| image::load_from_memory(frames[index].bytes).unwrap().into_rgba8();
        let crop = |index: usize, x: u32, y: u32, w: u32, h: u32| image::imageops::crop_imm(&image(index), x, y, w, h).to_image();
        let first = image(0);
        for (x, y, w, h, label) in [(36u32, 8u32, 74u32, 38u32, "頭"), (70u32, 42u32, 42u32, 46u32, "身體")] {
            assert_eq!(crop(0, x, y, w, h).as_raw(), crop(4, x, y, w, h).as_raw(), "{label}不能跟著整張圖位移");
        }
        assert_ne!(crop(0, 25, 45, 47, 43).as_raw(), crop(4, 25, 45, 47, 43).as_raw(), "尾巴必須自己擺動");
        assert_eq!(first.dimensions(), (144, 90));
    }

    #[test]
    fn pack_frames_are_opaque_line_art_on_transparent_background() {
        let pack = builtin_tiger("tiger").unwrap();
        let (frames, _, _) = &pack.animations["awake"];
        assert_eq!(frames.len(), 16);
        for frame in frames {
            let image = image::load_from_memory(frame.bytes).unwrap().into_rgba8();
            assert_eq!(image.dimensions(), (144, 90));
            assert_eq!(image.get_pixel(0, 0).0[3], 0, "背景必須透明");
            assert!(image.pixels().any(|pixel| pixel.0[3] > 200), "線稿必須可見");
        }
    }

    #[test]
    fn safe_join_rejects_escaping_paths() {
        let dir = Path::new("packs/tiger");
        assert!(safe_join(dir, "idle-00.png").is_some());
        assert!(safe_join(dir, "../secret.png").is_none());
        assert!(safe_join(dir, "C:\\secret.png").is_none());
    }
}

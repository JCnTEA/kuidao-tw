//! 配色、字體與字號。
//!
//! 字體：拉丁/數字用 Segoe UI（Windows 原生 UI 字體，與原版 DirectWrite 渲染同源），
//! 中文回退到微軟雅黑 UI，韓文回退到 Malgun Gothic。三檔字重：常規 / 半粗 / 粗。
//! 非 Windows（開發機、錄屏）按 `KUIDAO_FONT_DIR` → 常見系統路徑依次查找
//! 度量兼容的 Selawik 與 Noto Sans SC。

use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, FontId};
use std::path::{Path, PathBuf};
use std::sync::Arc;

// ---------- 顏色（原版逐像素取色，少量為審美微調） ----------
pub const RED: Color32 = Color32::from_rgb(242, 48, 70);
pub const RED_TOP: Color32 = Color32::from_rgb(253, 104, 92);
pub const BLUE: Color32 = Color32::from_rgb(62, 172, 248);
/// “已連線”藍：藍牙連線提醒和無電量裝置的連線環共用，不在電量的綠黃橙紅裡。
pub const LINK: Color32 = Color32::from_rgb(10, 132, 255);
pub const LINK_SOFT: Color32 = Color32::from_rgb(90, 169, 255);
pub const BLUE_DEEP: Color32 = Color32::from_rgb(13, 69, 128);
pub const CYAN: Color32 = Color32::from_rgb(102, 219, 255);
pub const SOFT_GREEN: Color32 = Color32::from_rgb(118, 196, 151);
pub const GREEN: Color32 = Color32::from_rgb(48, 209, 88);
/// 健康頁「久坐」模式的主色（與喝水的藍成對）。
pub const SIT_GREEN: Color32 = Color32::from_rgb(70, 200, 125);
pub const CHARGE: Color32 = Color32::from_rgb(33, 235, 84);
pub const WHITE: Color32 = Color32::WHITE;
pub const GRAY: Color32 = Color32::from_rgb(178, 178, 186);
pub const DIM: Color32 = Color32::from_rgb(142, 142, 153);
pub const BTN: Color32 = Color32::from_rgb(40, 40, 43);
pub const BTN_HOVER: Color32 = Color32::from_rgb(56, 56, 60);
pub const CARD: Color32 = Color32::from_rgb(22, 22, 24);
pub const TRACK: Color32 = Color32::from_rgb(36, 36, 39);
pub const ARROW_BG: Color32 = Color32::from_rgb(48, 48, 51);
pub const WATER_TRACK: Color32 = Color32::from_rgb(15, 38, 64);
pub const YELLOW: Color32 = Color32::from_rgb(255, 196, 61);
/// 無線裝置電量 10–19%：介於黃色“一般”和紅色“即將耗盡”之間。
pub const ORANGE: Color32 = Color32::from_rgb(255, 138, 61);

pub fn alpha_blue(a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(BLUE.r(), BLUE.g(), BLUE.b(), (255.0 * a) as u8)
}

/// 字重檔位。
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum W {
    Regular,
    Semibold,
    Bold,
}

pub fn font(size: f32, w: W) -> FontId {
    match w {
        W::Regular => FontId::proportional(size),
        W::Semibold => FontId::new(size, FontFamily::Name("semibold".into())),
        W::Bold => FontId::new(size, FontFamily::Name("bold".into())),
    }
}

/// 字號表（邏輯點）。漢字天然比拉丁字母「顯大」，中文字號單獨一套，
/// 讓兩種文字的視覺重量一致，而不是硬把墨高壓到拉丁大寫高度。
pub mod size {
    pub const TITLE: f32 = 14.5; // 頁標題（中文）
    pub const DATE: f32 = 11.0; // 頂欄日期
    pub const CLOCK: f32 = 12.5; // 頂欄時鐘
    pub const PILL_CLOCK: f32 = 15.5; // 藥丸時鐘
    pub const HERO: f32 = 29.0; // 大數字（25:00 / 0.1 / 0/0）
    pub const BODY: f32 = 11.5; // 正文、按鈕
    pub const SMALL: f32 = 11.0; // 說明
    pub const TINY: f32 = 10.0; // 腳註、座標軸
}

struct Face {
    path: PathBuf,
    index: u32,
}

fn find(cands: &[(&str, u32)], dirs: &[PathBuf]) -> Option<Face> {
    for (name, index) in cands {
        let p = Path::new(name);
        if p.is_absolute() && p.exists() {
            return Some(Face { path: p.to_path_buf(), index: *index });
        }
        for d in dirs {
            let q = d.join(name);
            if q.exists() {
                return Some(Face { path: q, index: *index });
            }
        }
    }
    None
}

fn font_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Ok(d) = std::env::var("KUIDAO_FONT_DIR") {
        v.push(PathBuf::from(d));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            v.push(d.join("fonts"));
        }
    }
    v.push(PathBuf::from("assets/fonts"));
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
    v.push(PathBuf::from(windir).join("Fonts"));
    for d in [
        "/usr/share/fonts/google-noto-cjk",
        "/usr/share/fonts/opentype/noto",
        "/usr/share/fonts/noto-cjk",
        "/usr/share/fonts/truetype",
    ] {
        v.push(PathBuf::from(d));
    }
    v
}

/// 裝載三檔字重。每檔 = 拉丁字體 + 中文回退 + 韓文回退；缺哪個就跳過，最後兜底 egui 內置字體。
pub fn load_fonts(ctx: &egui::Context) {
    ctx.set_fonts(shared_fonts().clone());
}

#[cfg(feature = "sdk-js")]
pub(crate) fn sync_canvas_fonts(
    ctx: &egui::Context,
    active: std::collections::BTreeMap<String, Arc<FontData>>,
) {
    let changed = ctx.data_mut(|data| {
        let previous = data.get_temp_mut_or_default::<std::collections::BTreeMap<String, Arc<FontData>>>(
            egui::Id::new("canvas-fonts-active"),
        );
        if previous.len() == active.len() && active.iter().all(|(name, font)| {
            previous.get(name).is_some_and(|installed| Arc::ptr_eq(installed, font))
        }) {
            return false;
        }
        *previous = active.clone();
        true
    });
    if changed {
        let mut definitions = ctx.fonts(|fonts| fonts.lock().fonts.definitions().clone());
        definitions.font_data.retain(|name, _| !name.starts_with("plugin:"));
        definitions.families.retain(|family, _| !canvas_font_family(family));
        let fallback = definitions.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
        for (name, font) in active {
            let mut family = vec![name.clone()];
            family.extend(fallback.iter().cloned());
            definitions.families.insert(FontFamily::Name(name.clone().into()), family);
            definitions.font_data.insert(name, font);
        }
        ctx.set_fonts(definitions);
    }
}

#[cfg(feature = "sdk-js")]
pub(crate) fn canvas_font_family(family: &FontFamily) -> bool {
    matches!(family, FontFamily::Name(name) if name.starts_with("plugin:"))
}

#[cfg(feature = "sdk-js")]
pub(crate) fn canvas_font(
    size: f32,
    weight: W,
    name: Option<&str>,
    installed: &[FontFamily],
) -> FontId {
    if let Some(name) = name {
        let family = FontFamily::Name(format!("plugin:{name}").into());
        if installed.contains(&family) {
            return FontId::new(size, family);
        }
    }
    font(size, weight)
}

/// 測試匯出「修復前」對照圖：去掉藏文字體後的字體表。
#[cfg(test)]
pub(crate) fn fonts_without_tibetan()->FontDefinitions {
    let mut fonts=shared_fonts().clone();
    for list in fonts.families.values_mut() {list.retain(|key|key!="tibetan");}
    fonts
}

fn shared_fonts()->&'static FontDefinitions {
    static FONTS:std::sync::OnceLock<FontDefinitions>=std::sync::OnceLock::new();
    FONTS.get_or_init(build_fonts)
}

#[cfg(windows)]
struct MappedFont {
    _source:std::fs::File,
    bytes:memmap2::Mmap,
}

#[cfg(windows)]
impl MappedFont {
    fn open(path:&Path)->std::io::Result<Self>{
        use std::os::windows::fs::OpenOptionsExt;
        // 保留檔案句柄並禁止原地寫入；允許系統以替換檔案的方式更新字體。
        let source=std::fs::OpenOptions::new().read(true).share_mode(0x1|0x4).open(path)?;
        let bytes=unsafe{memmap2::Mmap::map(&source)?};
        Ok(Self{_source:source,bytes})
    }
}

fn font_bytes(path:&Path)->Option<&'static [u8]>{
    #[cfg(windows)]
    if let Ok(mapped)=MappedFont::open(path){
        // 所有字體由進程級 OnceLock 持有，映射和只讀句柄與排版器同壽命。
        let mapped=Box::leak(Box::new(mapped));return Some(&mapped.bytes);
    }
    Some(Box::leak(std::fs::read(path).ok()?.into_boxed_slice()))
}

/// 微軟喜馬拉雅體的字形比同字號的中文小一圈，放大後藏文與中文、英文大致等高。
const TIBETAN_SCALE: f32 = 1.35;

fn build_fonts()->FontDefinitions {
    let dirs = font_dirs();
    let latin = [
        find(&[("segoeui.ttf", 0), ("selawk.ttf", 0)], &dirs),
        find(&[("seguisb.ttf", 0), ("selawksb.ttf", 0), ("segoeuib.ttf", 0)], &dirs),
        find(&[("segoeuib.ttf", 0), ("selawkb.ttf", 0)], &dirs),
    ];
    let cjk = [
        find(&[("msyh.ttc", 1), ("NotoSansSC-Regular.otf", 0), ("NotoSansCJK-Regular.ttc", 2)], &dirs),
        find(&[("msyhbd.ttc", 1), ("NotoSansSC-Medium.otf", 0), ("NotoSansCJK-Medium.ttc", 2)], &dirs),
        find(&[("msyhbd.ttc", 1), ("NotoSansSC-Bold.otf", 0), ("NotoSansCJK-Bold.ttc", 2)], &dirs),
    ];

    let korean = [
        find(&[("malgun.ttf", 0), ("NotoSansKR-Regular.otf", 0), ("NotoSansCJK-Regular.ttc", 1)], &dirs),
        find(&[("malgunbd.ttf", 0), ("malgun.ttf", 0), ("NotoSansKR-Bold.otf", 0), ("NotoSansCJK-Bold.ttc", 1)], &dirs),
    ];

    // 藏文：前面幾種字體都沒有藏文字形（#200）。微軟喜馬拉雅體的下加字、元音符號本身不佔寬度、疊在前一個字上，
    // 不做複雜排版也能拼出常見的疊字；它的字形偏小，放大到與中文大致等高。只有一檔字重，三檔共用。
    let tibetan = find(&[("himalaya.ttf", 0), ("NotoSerifTibetan-Regular.ttf", 0), ("NotoSansTibetan-Regular.ttf", 0)], &dirs);

    let mut fonts = FontDefinitions::default();
    let mut cache: Vec<(PathBuf, u32, String)> = Vec::new();
    let mut add = |fonts: &mut FontDefinitions, face: &Option<Face>, key: String| -> Option<String> {
        let face = face.as_ref()?;
        if let Some((_, _, k)) = cache.iter().find(|(p, i, _)| *p == face.path && *i == face.index) {
            return Some(k.clone());
        }
        let mut d = FontData::from_static(font_bytes(&face.path)?);
        d.index = face.index;
        fonts.font_data.insert(key.clone(), Arc::new(d));
        cache.push((face.path.clone(), face.index, key.clone()));
        Some(key)
    };

    let builtin = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let families = [
        FontFamily::Proportional,
        FontFamily::Name("semibold".into()),
        FontFamily::Name("bold".into()),
    ];
    for (i, fam) in families.iter().enumerate() {
        let mut list = Vec::new();
        if let Some(k) = add(&mut fonts, &latin[i], format!("latin{i}")) {
            list.push(k);
        }
        if let Some(k) = add(&mut fonts, &cjk[i], format!("cjk{i}")) {
            list.push(k);
        }
        if let Some(k) = add(&mut fonts, &korean[usize::from(i > 0)], format!("korean{i}")) {
            list.push(k);
        }
        if let Some(k) = add(&mut fonts, &tibetan, "tibetan".into()) {
            if let Some(data) = fonts.font_data.get_mut(&k) {
                if data.tweak.scale == 1.0 { Arc::make_mut(data).tweak.scale = TIBETAN_SCALE; }
            }
            list.push(k);
        }
        list.extend(builtin.iter().cloned());
        fonts.families.insert(fam.clone(), list);
    }
    fonts
}

/// 各個原生窗口統一使用鬼島品牌圖標，避免回落到框架預設圖標。
/// 開發版沿用同一套視窗圖示管線，只在編譯帶 `KUIDAO_CHANNEL=dev` 時換成緞帶版本。
fn app_icon_bytes()->&'static [u8]{
    if option_env!("KUIDAO_CHANNEL")==Some("dev"){ return include_bytes!("../assets/kuidao-dev-logo.png"); }
    include_bytes!("../assets/kuidao-logo.png")
}

pub fn app_icon()->std::sync::Arc<eframe::egui::IconData>{
    static ICON:std::sync::OnceLock<std::sync::Arc<eframe::egui::IconData>>=std::sync::OnceLock::new();
    ICON.get_or_init(||{
        let image=image::load_from_memory(app_icon_bytes()).expect("鬼島圖示資源應有效").thumbnail(128,128).to_rgba8();
        std::sync::Arc::new(eframe::egui::IconData{width:image.width(),height:image.height(),rgba:image.into_raw()})
    }).clone()
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    #[test]
    fn korean_titles_and_lyrics_have_glyphs_in_all_weights(){
        let ctx=eframe::egui::Context::default();
        super::load_fonts(&ctx);
        let _=ctx.run(Default::default(),|ctx|{
            for weight in [super::W::Regular,super::W::Semibold,super::W::Bold]{
                let font=super::font(14.0,weight);
                for ch in "눈, 코, 입 향기로운 숨을 담은 너의 코 사랑해 眼鼻嘴 TAEYANG ㄱㄴㅏ".chars().filter(|c|!c.is_whitespace()){
                    assert!(ctx.fonts(|f|f.has_glyph(&font,ch)),"缺少字形：{ch}，字型：{font:?}");
                }
            }
        });
    }

    /// #200：維語歌詞轉換後用到的連寫字形、藏文歌詞，在三檔字重下都要有字形（不能顯示成方框）。
    #[cfg(windows)]
    #[test]
    fn uyghur_and_tibetan_lyrics_have_glyphs_in_all_weights(){
        let ctx=eframe::egui::Context::default();
        super::load_fonts(&ctx);
        // 覆蓋維語全部字母：每個字母在詞首、詞中、詞尾、獨立各出現一次。
        let uyghur="سوي گۈ دىگەن شۇنداق بولامدۇ يايرىم سېنى سېغىندىم شۇنچە ئا ئە ب پ ت ج چ خ د ر ز ژ س ش غ ف ق ك گ ڭ ل م ن ھ و ۇ ۆ ۈ ۋ ې ى ي ئ لا بلا بەبە بۆبۆ بڭب بھب بېب بىب";
        let shown=crate::rtl_text::display(uyghur).into_owned();
        let tibetan="བཀྲ་ཤིས་བདེ་ལེགས། སྐྱིད་པོ། གྲོགས་པོ། ཁྱེད་རང་།";
        let _=ctx.run(Default::default(),|ctx|{
            for weight in [super::W::Regular,super::W::Semibold,super::W::Bold]{
                let font=super::font(14.0,weight);
                for ch in shown.chars().chain(tibetan.chars()).filter(|c|!c.is_whitespace()){
                    assert!(ctx.fonts(|f|f.has_glyph(&font,ch)),"缺少字形：U+{:04X}，字型：{font:?}",ch as u32);
                }
            }
        });
    }

    #[cfg(windows)]
    #[test]
    fn mapped_fonts_prevent_in_place_writes_and_keep_shared_reads(){
        let path=std::env::temp_dir().join(format!("daozai-font-map-{}-{}.bin",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::write(&path,b"readonly font fixture").unwrap();
        let mapped=super::MappedFont::open(&path).unwrap();
        assert_eq!(&mapped.bytes[..],b"readonly font fixture");
        assert_eq!(std::fs::read(&path).unwrap(),b"readonly font fixture");
        assert!(std::fs::OpenOptions::new().write(true).open(&path).is_err(),"對映存活期間不能允許原地改寫字型");
        drop(mapped);std::fs::remove_file(path).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn system_font_bytes_use_read_only_file_mapping(){
        #[link(name="kernel32")]
        unsafe extern "system"{
            fn K32GetMappedFileNameW(process:*mut core::ffi::c_void,address:*const core::ffi::c_void,buffer:*mut u16,size:u32)->u32;
        }
        let fonts=super::shared_fonts();let font=&fonts.font_data["cjk0"];
        let mut name=[0u16;1024];
        let count=unsafe{K32GetMappedFileNameW(windows::Win32::System::Threading::GetCurrentProcess().0,font.font.as_ptr().cast(),name.as_mut_ptr(),name.len()as u32)};
        assert!(count>0,"中文字型仍佔用完整的私有堆副本，沒有複用系統字型檔案頁");
        let name=String::from_utf16_lossy(&name[..count as usize]).to_lowercase();
        assert!([".ttc",".ttf",".otf"].iter().any(|suffix|name.ends_with(suffix)),"對映必須來自實際字型檔案");
    }

    #[test]
    fn font_data_is_shared_across_contexts_without_owned_byte_copies(){
        let fonts=super::shared_fonts();
        assert!(fonts.font_data.values().all(|font|matches!(font.font,std::borrow::Cow::Borrowed(_))),"字型不能在每個排版上下文中產生完整副本");
        let cloned=fonts.clone();
        for (name,font) in &fonts.font_data {assert!(std::sync::Arc::ptr_eq(font,&cloned.font_data[name]));}
    }
}

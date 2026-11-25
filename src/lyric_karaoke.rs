//! 歌詞導唱外掛（卡拉 OK 逐字亮起）。
//!
//! 這是一個獨立、可關閉的模組，和「整句固定漸層」互不影響：
//! 播放到哪裡，歌詞就從左到右逐字亮起，已唱過的字使用色盤漸層，
//! 還沒唱到的字保持灰暗，前緣用一小段柔和過渡，看起來像導唱光帶。
//!
//! 除了內建樣式，也支援外掛樣式包：
//! `%APPDATA%\Kuidao\lyric-karaoke\*.json`，欄位如下（皆可省略）：
//! ```json
//! { "name":"霓虹導唱", "preset":8, "shift":0.2, "edge":0.8, "dim_alpha":96 }
//! ```
//! 放進資料夾後重新啟動程式即可在設定頁選用；檔案格式錯誤會被忽略，
//! 不影響內建導唱。

use std::path::PathBuf;
use std::sync::OnceLock;

use eframe::egui::{self, Color32, Mesh, Pos2};

use crate::lyric_gradient;

/// 內建樣式名稱；設定檔存這個字串代表不套用外部樣式包。
pub const BUILTIN: &str = "內建";

/// 導唱樣式：色盤、相位、前緣柔和度與未唱字的亮度。
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub name: String,
    pub preset: usize,
    /// 漸層相位（0..1），會取模。
    pub shift: f32,
    /// 前緣柔和寬度，單位是「字」；越大過渡越長。
    pub edge: f32,
    /// 未唱字的亮度（0..255），越低越暗。
    pub dim: u8,
}

impl Style {
    /// 內建樣式沿用設定頁選的色盤與相位。
    pub fn builtin(preset: usize, shift: f32) -> Self {
        Self {
            name: BUILTIN.into(),
            preset: lyric_gradient::clamp_preset(preset),
            shift: shift.rem_euclid(1.0),
            edge: 0.7,
            dim: 118,
        }
    }

    /// 夾到安全範圍，避免外部樣式包給出離譜數值。
    fn sanitized(mut self) -> Self {
        self.preset = lyric_gradient::clamp_preset(self.preset);
        self.shift = self.shift.rem_euclid(1.0);
        self.edge = self.edge.clamp(0.1, 3.0);
        self.dim = self.dim.clamp(40, 210);
        self
    }
}

#[derive(serde::Deserialize)]
struct PackFile {
    #[serde(default = "default_name")]
    name: String,
    #[serde(default = "default_preset")]
    preset: usize,
    #[serde(default)]
    shift: f32,
    #[serde(default = "default_edge")]
    edge: f32,
    #[serde(default = "default_dim")]
    dim: u8,
}

fn default_name() -> String { "未命名導唱".into() }
fn default_preset() -> usize { 0 }
fn default_edge() -> f32 { 0.7 }
fn default_dim() -> u8 { 118 }

/// 外掛樣式包資料夾；和平台慣例一致，Windows 用 `%APPDATA%\Kuidao`。
fn packs_dir() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Kuidao")
        .join("lyric-karaoke")
}

/// 讀取樣式包；只讀一次，檔案壞掉就跳過（重新啟動程式即重新載入）。
fn loaded_packs() -> &'static [Style] {
    static PACKS: OnceLock<Vec<Style>> = OnceLock::new();
    PACKS.get_or_init(|| {
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(packs_dir()) else { return out; };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()).is_none_or(|ext| !ext.eq_ignore_ascii_case("json")) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else { continue; };
            let Ok(file) = serde_json::from_str::<PackFile>(&text) else { continue; };
            let name = file.name.trim();
            if name.is_empty() || name == BUILTIN {
                continue;
            }
            out.push(Style {
                name: name.to_owned(),
                preset: file.preset,
                shift: file.shift,
                edge: file.edge,
                dim: file.dim,
            }.sanitized());
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    })
}

/// 目前有哪些樣式可選（第一個固定是內建）。
pub fn style_names() -> Vec<String> {
    let mut names = vec![BUILTIN.to_owned()];
    names.extend(loaded_packs().iter().map(|style| style.name.clone()));
    names
}

/// 依設定選出樣式；名稱找不到就退回內建，不會因為刪掉外掛包而失效。
pub fn resolve(name: &str, preset: usize, shift: f32) -> Style {
    loaded_packs()
        .iter()
        .find(|style| style.name == name)
        .cloned()
        .unwrap_or_else(|| Style::builtin(preset, shift))
}

/// 單字的亮起權重：0 全暗、1 全亮；前緣以 `edge` 個字做柔和過渡。
pub fn glow_weight(progress: f32, glyph: usize, total: usize, edge: f32) -> f32 {
    if total == 0 { return 0.0; }
    let edge = edge.clamp(0.1, 3.0);
    let front = progress.clamp(0.0, 1.0) * total as f32;
    ((front - glyph as f32 + edge) / (2.0 * edge)).clamp(0.0, 1.0)
}

/// 已唱字的顏色：沿用整句漸層的可讀取色，依字序在色盤上取樣。
pub fn lit_color(style: &Style, glyph: usize, total: usize) -> Color32 {
    let t = if total <= 1 { 0.0 } else { glyph as f32 / (total - 1) as f32 };
    lyric_gradient::readable_text_color(style.preset, (t + style.shift).rem_euclid(1.0))
}

/// 未唱字的顏色：低亮度灰，但仍看得見歌詞內容。
pub fn dim_color(style: &Style) -> Color32 {
    Color32::from_rgb(style.dim, style.dim, style.dim)
}

#[inline]
fn mix(a: Color32, b: Color32, f: f32) -> Color32 {
    let f = f.clamp(0.0, 1.0);
    let ch = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * f).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(ch(a.r(), b.r()), ch(a.g(), b.g()), ch(a.b(), b.b()))
}

/// 把目前的歌詞 Galley 轉成導唱網格：逐字依播放進度上色，並平移到 `pos`。
///
/// `font_tex_size` 是字形貼圖寬高；Galley 的 UV 是貼圖像素座標，需正規化。
pub fn sweep_mesh(
    pos: Pos2,
    galley: &egui::Galley,
    style: &Style,
    progress: f32,
    font_tex_size: [f32; 2],
) -> Mesh {
    let total: usize = galley.rows.iter().map(|row| row.glyphs.len()).sum();
    let mut out = Mesh::default();
    let mut glyph_base = 0usize;
    for row in &galley.rows {
        let src = &row.visuals.mesh;
        let glyphs = row.glyphs.len();
        if !src.vertices.is_empty() {
            out.texture_id = src.texture_id;
        }
        let base = out.vertices.len() as u32;
        let quads = src.vertices.len() / 4;
        for (i, v) in src.vertices.iter().enumerate() {
            // 每個字形是一組四頂點；字數和頂點不對應時，退回以行內比例估算。
            let local = if quads >= glyphs && glyphs > 0 {
                (i / 4).min(glyphs - 1)
            } else if glyphs > 0 {
                (i * glyphs / src.vertices.len().max(1)).min(glyphs - 1)
            } else {
                0
            };
            let glyph = glyph_base + local;
            let weight = glow_weight(progress, glyph, total, style.edge);
            let color = mix(dim_color(style), lit_color(style, glyph, total), weight);
            out.vertices.push(egui::epaint::Vertex {
                pos: v.pos + pos.to_vec2(),
                uv: Pos2::new(
                    (v.uv.x / font_tex_size[0]).clamp(0.0, 1.0),
                    (v.uv.y / font_tex_size[1]).clamp(0.0, 1.0),
                ),
                // 保留原本的透明度（淡入淡出、次要文字）。
                color: Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), v.color.a()),
            });
        }
        out.indices.extend(src.indices.iter().map(|i| i + base));
        glyph_base += glyphs;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lum(c: Color32) -> f32 {
        0.2126 * c.r() as f32 + 0.7152 * c.g() as f32 + 0.0722 * c.b() as f32
    }

    #[test]
    fn builtin_style_is_sanitized() {
        let style = Style::builtin(99, -0.25);
        assert_eq!(style.preset, lyric_gradient::PALETTE_COUNT - 1);
        assert!((style.shift - 0.75).abs() < 1e-6);
        assert_eq!(style.name, BUILTIN);
    }

    #[test]
    fn out_of_range_style_values_are_clamped() {
        let style = Style { name: "x".into(), preset: 42, shift: 3.5, edge: 9.0, dim: 255 }.sanitized();
        assert_eq!(style.preset, lyric_gradient::PALETTE_COUNT - 1);
        assert!((style.shift - 0.5).abs() < 1e-6);
        assert!((style.edge - 3.0).abs() < 1e-6);
        assert_eq!(style.dim, 210);
    }

    #[test]
    fn weights_move_left_to_right_with_progress() {
        let edge = 0.7;
        // 開頭只有第一字的前緣擦到一點光。
        assert!(glow_weight(0.0, 0, 10, edge) < 0.6);
        // 進度一半時，前半亮、後半暗。
        for glyph in 0..4 {
            assert!(glow_weight(0.5, glyph, 10, edge) > 0.8, "第 {glyph} 字應已亮起");
        }
        for glyph in 6..10 {
            assert!(glow_weight(0.5, glyph, 10, edge) < 0.2, "第 {glyph} 字應還是暗的");
        }
        // 正好在光帶前緣的字是柔和過渡，不該非黑即白。
        let edge_weight=glow_weight(0.5, 5, 10, edge);
        assert!(edge_weight>0.2 && edge_weight<0.8, "前緣應是柔和過渡（{edge_weight}）");
        // 唱完整句全亮，回到開頭全暗。
        assert_eq!(glow_weight(1.0, 9, 10, edge), 1.0);
        assert!(glow_weight(0.0, 9, 10, edge) < 0.05);
    }

    #[test]
    fn weight_is_monotonic_for_a_fixed_glyph() {
        let mut last = -1.0;
        for k in 0..=20 {
            let w = glow_weight(k as f32 / 20.0, 3, 8, 0.7);
            assert!(w >= last, "權重不應倒退");
            last = w;
        }
    }

    #[test]
    fn lit_text_is_brighter_than_unsung_text() {
        for preset in 0..lyric_gradient::PALETTE_COUNT {
            let style = Style::builtin(preset, 0.0);
            for glyph in 0..6 {
                let lit = lum(lit_color(&style, glyph, 6));
                assert!(lit > lum(dim_color(&style)), "色盤 {preset} 已唱字不夠亮");
            }
        }
    }

    #[test]
    fn lit_colors_keep_a_gradient_across_the_line() {
        // 色盤已縮成兩組粉彩，逐一驗證每一組在整句上仍有可見的顏色差，而不是固定挑某一組。
        for preset in 0..lyric_gradient::PALETTE_COUNT {
            let style = Style::builtin(preset, 0.0);
            let lit = [0usize, 4, 7].map(|glyph| lum(lit_color(&style, glyph, 8)));
            assert!(
                (lit[0] - lit[2]).abs() > 2.0 || (lit[1] - lit[2]).abs() > 2.0,
                "色盤 {preset} 整句應有可見漸層：{lit:?}"
            );
        }
    }

    #[test]
    fn mix_hits_both_ends() {
        let a = Color32::from_rgb(10, 20, 30);
        let b = Color32::from_rgb(200, 210, 220);
        assert_eq!(mix(a, b, 0.0), a);
        assert_eq!(mix(a, b, 1.0), b);
    }

    #[test]
    fn style_names_include_builtin() {
        assert_eq!(style_names()[0], BUILTIN);
    }

    #[test]
    fn resolve_falls_back_to_builtin_for_unknown_name() {
        let style = resolve("不存在的樣式", 1, 0.4);
        assert_eq!(style.name, BUILTIN);
        assert_eq!(style.preset, 1);
    }
}

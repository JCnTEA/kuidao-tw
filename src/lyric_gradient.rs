//! 歌詞漸層色：11 組色盤、漸層取樣、漸層色帶與漸層文字網格。
//!
//! 色盤來自使用者提供的 11 張參考圖，每組 6 色。歌詞以 Galley 各 row 的
//! 文字網格為基礎，依頂點的水平位置重新取色，達成「用拉的」漸層效果。

use eframe::egui::{self, Color32, Mesh, Pos2, Rect};

/// 11 組色盤，每組 6 色（RGB）；前 5 色由深到淺，第 6 色為強調色。
pub const PALETTES: [[[u8; 3]; 6]; 11] = [
    [[0x17,0x15,0x1C],[0x29,0x25,0x31],[0x45,0x40,0x4F],[0x71,0x68,0x79],[0xAA,0xA2,0xB3],[0xE2,0xDD,0xE7]],
    [[0x21,0x1B,0x1A],[0x38,0x2A,0x28],[0x60,0x43,0x3D],[0x91,0x6B,0x60],[0xC1,0xA0,0x94],[0xE8,0xD8,0xCE]],
    [[0x11,0x15,0x22],[0x1E,0x26,0x3B],[0x30,0x3D,0x59],[0x53,0x66,0x84],[0x98,0xA9,0xC4],[0xDC,0xE3,0xEF]],
    [[0x12,0x0D,0x0A],[0x2E,0x21,0x1A],[0x6B,0x4A,0x33],[0xC0,0x9A,0x77],[0xF2,0xE6,0xD8],[0x3F,0xD9,0xA3]],
    [[0x10,0x0E,0x0B],[0x2A,0x25,0x1C],[0x5C,0x53,0x42],[0xAF,0xA4,0x8A],[0xF0,0xE9,0xDB],[0x1F,0x8A,0x8C]],
    [[0x14,0x0A,0x0E],[0x3A,0x16,0x20],[0x7A,0x2F,0x3E],[0xC9,0x81,0x7A],[0xF3,0xDC,0xCB],[0xE0,0x65,0x2F]],
    [[0x15,0x0E,0x1C],[0x2C,0x1A,0x3A],[0x5B,0x35,0x70],[0xB0,0x8B,0xC4],[0xEF,0xE2,0xF0],[0xF0,0xA8,0x30]],
    [[0x0C,0x0C,0x0F],[0x19,0x19,0x22],[0x3A,0x3A,0x48],[0x8C,0x8C,0xA3],[0xED,0xED,0xF2],[0x7C,0x4D,0xFF]],
    [[0x09,0x0D,0x1A],[0x1B,0x21,0x40],[0x3D,0x4A,0x8C],[0x9A,0xA7,0xE6],[0xE8,0xE6,0xF2],[0xC2,0xF2,0x4A]],
    [[0x0E,0x11,0x13],[0x1E,0x26,0x2B],[0x44,0x56,0x5F],[0x93,0xAA,0xB3],[0xE9,0xEF,0xF0],[0xFF,0x7A,0x5C]],
    [[0x08,0x12,0x1A],[0x12,0x29,0x3B],[0x2C,0x58,0x71],[0x86,0xB3,0xC9],[0xEA,0xF1,0xF4],[0xF2,0xA0,0xA0]],
];

/// 色盤數量。
pub const PALETTE_COUNT: usize = PALETTES.len();

/// 把色盤序號夾到合法範圍。
pub fn clamp_preset(preset: usize) -> usize {
    preset.min(PALETTE_COUNT - 1)
}

#[inline]
fn lerp_u8(a: u8, b: u8, f: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * f).round().clamp(0.0, 255.0) as u8
}

/// 在某色盤上取 t（0..1）的漸層色，於 6 個停點之間分段線性插值。
pub fn sample(preset: usize, t: f32) -> Color32 {
    let preset = clamp_preset(preset);
    let stops = &PALETTES[preset];
    let t = t.clamp(0.0, 1.0);
    let scale = t * (stops.len() - 1) as f32;
    let i = (scale.floor() as usize).min(stops.len() - 2);
    let f = scale - i as f32;
    let a = stops[i];
    let b = stops[i + 1];
    Color32::from_rgb(
        lerp_u8(a[0], b[0], f),
        lerp_u8(a[1], b[1], f),
        lerp_u8(a[2], b[2], f),
    )
}

/// 圓角處的垂直內縮（距離矩形邊緣），dx 為與左/右邊緣的水平距離。
#[inline]
fn corner_inset(dx: f32, radius: f32) -> f32 {
    if dx >= radius {
        0.0
    } else {
        // 圓心距離邊緣 radius，故頂點與圓心的水平距離為 (radius - dx)。
        let from_center = radius - dx;
        let inside = (radius * radius - from_center * from_center).max(0.0).sqrt();
        radius - inside
    }
}

/// 建立一條圓角漸層色帶的網格，水平方向連續取樣。
pub fn strip_mesh(rect: Rect, preset: usize, radius: f32) -> Mesh {
    let segments = 48;
    let radius = radius.min(rect.height() / 2.0).min(rect.width() / 2.0);
    let mut mesh = Mesh::default();
    let mut prev: Option<(f32, f32, f32)> = None; // (x, top_inset, bottom_inset)
    for k in 0..=segments {
        let x = rect.left() + rect.width() * k as f32 / segments as f32;
        let dx_left = x - rect.left();
        let dx_right = rect.right() - x;
        let inset = corner_inset(dx_left.min(dx_right), radius);
        let top = rect.top() + inset;
        let bottom = rect.bottom() - inset;
        if let Some((px, pt, pb)) = prev {
            let t_left = (px - rect.left()) / rect.width();
            let t_right = (x - rect.left()) / rect.width();
            let c_left = sample(preset, t_left);
            let c_right = sample(preset, t_right);
            let base = mesh.vertices.len() as u32;
            mesh.vertices.push(vertex(Pos2::new(px, pt), c_left));
            mesh.vertices.push(vertex(Pos2::new(px, pb), c_left));
            mesh.vertices.push(vertex(Pos2::new(x, top), c_right));
            mesh.vertices.push(vertex(Pos2::new(x, bottom), c_right));
            mesh.indices.extend_from_slice(&[base, base + 1, base + 2, base + 1, base + 3, base + 2]);
        }
        prev = Some((x, top, bottom));
    }
    mesh
}

#[inline]
fn vertex(pos: Pos2, color: Color32) -> egui::epaint::Vertex {
    egui::epaint::Vertex { pos, uv: Pos2::ZERO, color }
}

/// 可讀文字的亮度（HSL L）區間：暗部提到 READ_L_MIN 確保黑底藥丸上看得清，
/// 亮部保留在 READ_L_MAX；整段色盤的明暗差重新對應到這個區間，故漸層對比不會被洗掉。
const READ_L_MIN: f32 = 0.66;
const READ_L_MAX: f32 = 0.94;

/// RGB（0..1）轉 HSL（h、s、l 皆 0..1）。
fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d <= 1e-6 {
        return (0.0, 0.0, l);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if (max - r).abs() <= 1e-6 {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if (max - g).abs() <= 1e-6 {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } / 6.0;
    (h.rem_euclid(1.0), s.clamp(0.0, 1.0), l)
}

/// HSL 轉 RGB（0..1）。
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s <= 1e-6 {
        return (l, l, l);
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let hue = |mut x: f32| {
        x = x.rem_euclid(1.0);
        if x < 1.0 / 6.0 {
            p + (q - p) * 6.0 * x
        } else if x < 0.5 {
            q
        } else if x < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - x) * 6.0
        } else {
            p
        }
    };
    (hue(h + 1.0 / 3.0), hue(h), hue(h - 1.0 / 3.0))
}

/// 色盤各停點的 HSL 亮度（L）最小／最大值。
fn palette_lightness_range(preset: usize) -> (f32, f32) {
    let mut lmin = f32::INFINITY;
    let mut lmax = f32::NEG_INFINITY;
    for stop in &PALETTES[preset] {
        let (_, _, l) = rgb_to_hsl(
            stop[0] as f32 / 255.0,
            stop[1] as f32 / 255.0,
            stop[2] as f32 / 255.0,
        );
        lmin = lmin.min(l);
        lmax = lmax.max(l);
    }
    (lmin, lmax)
}

/// 歌詞文字取色：保留色盤色相與飽和度，把整段明暗重新對應到可讀亮度區間，
/// 暗部不再被壓成一片白，漸層在黑底藥丸上仍清楚可見。
fn readable_text_color(preset: usize, t: f32) -> Color32 {
    let preset = clamp_preset(preset);
    let raw = sample(preset, t);
    let (h, s, l) = rgb_to_hsl(
        raw.r() as f32 / 255.0,
        raw.g() as f32 / 255.0,
        raw.b() as f32 / 255.0,
    );
    let (lmin, lmax) = palette_lightness_range(preset);
    let mapped = if lmax - lmin <= 1e-4 {
        READ_L_MIN
    } else {
        let frac = ((l - lmin) / (lmax - lmin)).clamp(0.0, 1.0);
        READ_L_MIN + frac * (READ_L_MAX - READ_L_MIN)
    };
    let (r, g, b) = hsl_to_rgb(h, s, mapped);
    Color32::from_rgb(
        (r * 255.0).round().clamp(0.0, 255.0) as u8,
        (g * 255.0).round().clamp(0.0, 255.0) as u8,
        (b * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

/// 把 Galley 各 row 的文字網格合併、依頂點水平位置重新上漸層色，並平移到 `pos`。
///
/// `shift` 為漸層相位（0..1，會自動取模），拖曳拉桿即連續捲動漸層。
/// `font_tex_size` 為字形貼圖寬高，Galley 網格的 UV 是貼圖素座標，需除以它來正規化。
pub fn gradient_text_mesh(
    pos: Pos2,
    galley: &egui::Galley,
    preset: usize,
    shift: f32,
    font_tex_size: [f32; 2],
) -> Mesh {
    // 先求文字頂點的實際 x 範圍，讓漸層貼合文字而非整個 Galley 寬度。
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    for row in &galley.rows {
        for v in &row.visuals.mesh.vertices {
            min_x = min_x.min(v.pos.x);
            max_x = max_x.max(v.pos.x);
        }
    }
    let span = (max_x - min_x).max(0.0001);
    let phase = shift.rem_euclid(1.0);

    let mut out = Mesh::default();
    for row in &galley.rows {
        let src = &row.visuals.mesh;
        if !src.vertices.is_empty() {
            out.texture_id = src.texture_id;
        }
        let base = out.vertices.len() as u32;
        for v in &src.vertices {
            let local = ((v.pos.x - min_x) / span + phase).rem_euclid(1.0);
            let mut color = readable_text_color(preset, local);
            // 保留原始頂點的透明度（例如弱音/淡出），只置換色相。
            color = Color32::from_rgba_unmultiplied(
                color.r(),
                color.g(),
                color.b(),
                v.color.a(),
            );
            out.vertices.push(egui::epaint::Vertex {
                pos: v.pos + pos.to_vec2(),
                uv: Pos2::new(
                    (v.uv.x / font_tex_size[0]).clamp(0.0, 1.0),
                    (v.uv.y / font_tex_size[1]).clamp(0.0, 1.0),
                ),
                color,
            });
        }
        out.indices.extend(src.indices.iter().map(|i| i + base));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_count_is_eleven() {
        assert_eq!(PALETTE_COUNT, 11);
        assert_eq!(PALETTES.len(), 11);
    }

    #[test]
    fn clamp_preset_stays_in_range() {
        assert_eq!(clamp_preset(0), 0);
        assert_eq!(clamp_preset(10), 10);
        assert_eq!(clamp_preset(99), 10);
        assert_eq!(clamp_preset(usize::MAX), 10);
    }

    #[test]
    fn sample_endpoints_match_stops() {
        // t=0 與 t=1 應分別貼合第一與最後一個停點。
        let first = sample(8, 0.0);
        let last = sample(8, 1.0);
        assert_eq!([first.r(), first.g(), first.b()], PALETTES[8][0]);
        assert_eq!([last.r(), last.g(), last.b()], PALETTES[8][5]);
    }

    #[test]
    fn sample_clamps_out_of_range_input() {
        let a = sample(2, -1.0);
        let b = sample(2, 0.0);
        assert_eq!(a, b);
        let c = sample(2, 2.5);
        let d = sample(2, 1.0);
        assert_eq!(c, d);
    }

    #[test]
    fn sample_unknown_preset_falls_back_to_last() {
        // 超出範圍的色盤不應 panic，並夾到最後一組。
        let over = sample(40, 0.5);
        let valid = sample(10, 0.5);
        assert_eq!(over, valid);
    }

    #[test]
    fn strip_mesh_covers_rect_with_triangles() {
        let rect = Rect::from_min_size(Pos2::new(0.0, 0.0), egui::vec2(200.0, 16.0));
        let mesh = strip_mesh(rect, 7, 8.0);
        assert!(mesh.indices.len() % 3 == 0);
        assert!(!mesh.vertices.is_empty());
        // 頂點水平範圍貼合色帶。
        let xs: Vec<f32> = mesh.vertices.iter().map(|v| v.pos.x).collect();
        assert!(xs.iter().cloned().fold(f32::INFINITY, f32::min) <= 0.001);
        assert!(xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max) >= 199.99);
    }

    #[test]
    fn corner_inset_is_zero_away_from_edge_and_max_at_edge() {
        assert_eq!(corner_inset(8.0, 8.0), 0.0);
        assert!((corner_inset(0.0, 8.0) - 8.0).abs() < 1e-5);
        assert!(corner_inset(4.0, 8.0) > 0.0 && corner_inset(4.0, 8.0) < 8.0);
    }

    fn luminance(c: Color32) -> f32 {
        0.2126 * c.r() as f32 + 0.7152 * c.g() as f32 + 0.0722 * c.b() as f32
    }

    #[test]
    fn readable_text_stays_above_readable_floor() {
        // 所有色盤、所有相位的文字取色都要夠亮，黑底上看得清。
        for preset in 0..PALETTE_COUNT {
            for k in 0..=40 {
                let c = readable_text_color(preset, k as f32 / 40.0);
                assert!(luminance(c) >= 135.0, "preset {preset} 出現過暗文字 {c:?}");
            }
        }
    }

    #[test]
    fn readable_text_keeps_a_visible_gradient_spread() {
        // 同一色盤最暗與最亮的可讀色必須保留明顯差距（看整段相位，而非只比兩端點），
        // 否則看起來就像沒有漸層。
        for preset in 0..PALETTE_COUNT {
            let mut dark = f32::INFINITY;
            let mut bright = f32::NEG_INFINITY;
            for k in 0..=40 {
                let l = luminance(readable_text_color(preset, k as f32 / 40.0));
                dark = dark.min(l);
                bright = bright.max(l);
            }
            assert!(
                bright - dark >= 45.0,
                "preset {preset} 漸層對比被洗掉 ({dark}..{bright})"
            );
        }
    }

    #[test]
    fn hsl_roundtrip_for_neutral_grays() {
        for v in [0.0f32, 0.25, 0.5, 0.75, 1.0] {
            let (h, s, l) = rgb_to_hsl(v, v, v);
            assert_eq!(s, 0.0);
            let (r, g, b) = hsl_to_rgb(h, s, l);
            assert!((r - v).abs() < 1e-4 && (g - v).abs() < 1e-4 && (b - v).abs() < 1e-4);
        }
    }
}

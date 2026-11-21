//! 單線條吉祥物：五種形象都用同一套向量筆畫即時繪製。
//!
//! 不再把 144×90 的線稿縮到 48×30 造成糊成一團；身體固定，只有尾巴、
//! 耳朵與眼睛在動，所以島體不會上下左右位移。

use eframe::egui::Color32;

use crate::mascot::Mascot;
use crate::paint::Card;
use crate::theme;


/// 目前形象在 48×30 槽位內的單線條待機動畫。
pub(crate) fn draw(c: &Card, cx: f32, t: f64, state: crate::pet::PetState) {
    let species = crate::mascot::current();
    let swing = (t * 1.6).sin() as f32;
    let ear = ear_twitch(t);
    let ink = theme::WHITE;

    // 尾巴先畫，身體與頭再蓋上去，重疊處才不會多出一截。
    match species {
        Mascot::Tiger => tail(c, cx, swing, ink),
        Mascot::Shiba => shiba_tail(c, cx, swing, ink),
        Mascot::Cat => cat_tail(c, cx, swing, ink),
        Mascot::Husky => husky_tail(c, cx, swing, ink),
        Mascot::Golden => golden_tail(c, cx, swing, ink),
    }

    body(c, cx, ink);

    match species {
        Mascot::Tiger => tiger(c, cx, ear, ink),
        Mascot::Shiba => shiba(c, cx, ear, ink),
        Mascot::Cat => cat(c, cx, ear, ink),
        Mascot::Husky => husky(c, cx, ear, ink),
        Mascot::Golden => golden(c, cx, ear, ink),
    }

    face(c, cx, state, ink);
}

/// 每 6 秒一次、持續 0.4 秒的耳朵輕顫；幅度只夠看得出來，不會拉扯整張圖。
fn ear_twitch(t: f64) -> f32 {
    let phase = t.rem_euclid(6.0);
    if phase < 0.4 { ((phase / 0.4) * std::f64::consts::PI).sin() as f32 * 0.45 } else { 0.0 }
}

fn arc_ellipse(c: &Card, cx: f32, cy: f32, rx: f32, ry: f32, a0: f32, a1: f32, width: f32, ink: Color32) {
    let n = (((a1 - a0).abs() / std::f32::consts::TAU) * 64.0).ceil().max(6.0) as usize;
    let pts: Vec<_> = (0..=n).map(|i| {
        let a = a0 + (a1 - a0) * i as f32 / n as f32;
        (cx + rx * a.cos(), cy + ry * a.sin())
    }).collect();
    c.path(&pts, width, ink);
}

fn cubic(c: &Card, p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), p3: (f32, f32), width: f32, ink: Color32) {
    let mut pts = Vec::with_capacity(18);
    for i in 0..=16 {
        let t = i as f32 / 16.0;
        let u = 1.0 - t;
        let x = u * u * u * p0.0 + 3.0 * u * u * t * p1.0 + 3.0 * u * t * t * p2.0 + t * t * t * p3.0;
        let y = u * u * u * p0.1 + 3.0 * u * u * t * p1.1 + 3.0 * u * t * t * p2.1 + t * t * t * p3.1;
        pts.push((x, y));
    }
    c.path(&pts, width, ink);
}

fn body(c: &Card, cx: f32, ink: Color32) {
    cubic(
        c,
        (cx - 4.8, 20.0),
        (cx - 7.0, 22.2),
        (cx - 7.0, 27.2),
        (cx - 4.0, 28.2),
        1.3,
        ink,
    );
    cubic(
        c,
        (cx + 4.8, 20.0),
        (cx + 7.0, 22.2),
        (cx + 7.0, 27.2),
        (cx + 4.0, 28.2),
        1.3,
        ink,
    );
    arc_ellipse(c, cx, 26.5, 4.0, 1.8, 0.0, std::f32::consts::PI, 1.2, ink);
    // 兩隻前腳：只留小小的腳背，不畫成機械支架。
    arc_ellipse(c, cx - 3.2, 28.1, 1.1, 0.7, std::f32::consts::PI, std::f32::consts::TAU, 0.9, ink);
    arc_ellipse(c, cx + 3.2, 28.1, 1.1, 0.7, std::f32::consts::PI, std::f32::consts::TAU, 0.9, ink);
}

fn head(c: &Card, cx: f32, ink: Color32) {
    arc_ellipse(c, cx, 12.8, 8.6, 8.3, 0.0, std::f32::consts::TAU, 1.4, ink);
}

fn face(c: &Card, cx: f32, state: crate::pet::PetState, ink: Color32) {
    // 鼻子是實心小三角，嘴巴是一道彎；比兩團弧線更容易在 48×30 看清楚。
    let nose = [(cx - 1.2, 16.2), (cx + 1.2, 16.2), (cx, 17.4)];
    c.poly(&nose, ink);
    use crate::pet::PetState::*;
    if matches!(state, Music | Happy | Satisfied) {
        arc_ellipse(c, cx, 17.7, 1.0, 0.8, 0.1, std::f32::consts::PI - 0.1, 0.9, ink);
    } else {
        c.path(&[(cx - 1.2, 17.8), (cx, 18.5), (cx + 1.2, 17.8)], 0.9, ink);
    }
}

fn tiger(c: &Card, cx: f32, ear: f32, ink: Color32) {
    arc_ellipse(c, cx - 6.0 + ear, 6.4, 3.0, 2.8, 3.32, 6.10, 1.3, ink);
    arc_ellipse(c, cx + 6.0 + ear, 6.4, 3.0, 2.8, 3.32, 6.10, 1.3, ink);
    head(c, cx, ink);
    for dx in [-2.0, 0.0, 2.0] {
        c.seg((cx + dx, 5.9), (cx + dx - 0.2, 8.2), 0.9, ink);
    }
}

fn shiba(c: &Card, cx: f32, ear: f32, ink: Color32) {
    for side in [-1.0f32, 1.0] {
        let tip = cx + side * 5.4 + ear * side;
        c.path(&[(cx + side * 6.6, 7.4), (tip, 1.8), (cx + side * 2.2, 6.6)], 1.4, ink);
        c.seg((tip, 1.8), (cx + side * 4.4, 6.4), 0.9, ink);
        c.path(&[(cx + side * 8.2, 12.4), (cx + side * 9.6, 13.6), (cx + side * 8.2, 14.4)], 1.0, ink);
    }
    head(c, cx, ink);
    // 舌頭
    c.path(&[(cx - 0.7, 18.0), (cx - 0.8, 19.2), (cx + 0.8, 19.2), (cx + 0.7, 18.0)], 1.0, ink);
}

fn cat(c: &Card, cx: f32, ear: f32, ink: Color32) {
    for side in [-1.0f32, 1.0] {
        let tip = cx + side * 6.0 + ear * side;
        c.path(&[(cx + side * 7.4, 7.2), (tip, 1.2), (cx + side * 2.2, 6.4)], 1.4, ink);
    }
    head(c, cx, ink);
    // 貓的長尾巴在後面，這裡只保留乾淨的臉。
}

fn husky(c: &Card, cx: f32, ear: f32, ink: Color32) {
    for side in [-1.0f32, 1.0] {
        let tip = cx + side * 6.6 + ear * side;
        c.path(&[(cx + side * 6.8, 7.2), (tip, 1.6), (cx + side * 2.2, 6.4)], 1.4, ink);
    }
    head(c, cx, ink);
    // 哈士奇的面罩：眼睛上方兩道弧、額頭一道白線。
    arc_ellipse(c, cx - 3.6, 11.8, 2.8, 2.9, 3.58, 5.85, 1.0, ink);
    arc_ellipse(c, cx + 3.6, 11.8, 2.8, 2.9, 3.58, 5.85, 1.0, ink);
    c.seg((cx, 5.6), (cx, 9.6), 0.9, ink);
}

fn golden(c: &Card, cx: f32, ear: f32, ink: Color32) {
    for side in [-1.0f32, 1.0] {
        let dx = ear * side;
        c.path(
            &[
                (cx + side * 4.8 + dx, 6.0),
                (cx + side * 6.8 + dx, 9.4),
                (cx + side * 7.0 + dx, 13.4),
                (cx + side * 5.8 + dx, 15.2),
                (cx + side * 4.4 + dx, 14.4),
            ],
            1.3,
            ink,
        );
    }
    head(c, cx, ink);
    c.path(&[(cx - 2.8, 5.8), (cx - 1.9, 7.2), (cx, 5.6), (cx + 1.9, 7.2), (cx + 2.8, 5.8)], 1.0, ink);
    c.path(&[(cx - 0.8, 18.0), (cx - 0.9, 19.4), (cx + 0.9, 19.4), (cx + 0.8, 18.0)], 1.0, ink);
}

fn tail(c: &Card, cx: f32, swing: f32, ink: Color32) {
    cubic(
        c,
        (cx + 5.6, 25.4),
        (cx + 11.6, 26.0 + swing * 0.5),
        (cx + 12.0, 21.0 + swing),
        (cx + 10.0, 17.6),
        1.3,
        ink,
    );
}

fn shiba_tail(c: &Card, cx: f32, swing: f32, ink: Color32) {
    let a = swing * 0.18;
    arc_ellipse(c, cx + 8.6, 20.0, 3.2, 3.2, 0.35 + a, 5.76 + a, 1.3, ink);
    arc_ellipse(c, cx + 8.8, 19.8, 1.4, 1.4, 1.05 + a, 5.24 + a, 1.0, ink);
}

fn cat_tail(c: &Card, cx: f32, swing: f32, ink: Color32) {
    cubic(
        c,
        (cx + 5.8, 25.4),
        (cx + 12.0, 26.0 + swing * 0.6),
        (cx + 12.4, 21.0 + swing),
        (cx + 10.0, 17.8),
        1.3,
        ink,
    );
}

fn husky_tail(c: &Card, cx: f32, swing: f32, ink: Color32) {
    cubic(
        c,
        (cx + 6.2, 25.4),
        (cx + 10.8, 26.6 + swing * 0.4),
        (cx + 11.6, 22.8 + swing),
        (cx + 9.6, 20.8),
        1.3,
        ink,
    );
    c.seg((cx + 10.6, 26.9), (cx + 12.2, 26.4 + swing), 0.9, ink);
}

fn golden_tail(c: &Card, cx: f32, swing: f32, ink: Color32) {
    cubic(
        c,
        (cx + 6.0, 25.4),
        (cx + 10.4, 25.4),
        (cx + 11.6, 21.4 + swing),
        (cx + 9.4, 18.0),
        1.3,
        ink,
    );
    // 尾巴末端兩撮毛，讓黃金獵犬跟其他形象分得出來。
    c.seg((cx + 9.4, 18.0), (cx + 8.4, 16.8), 0.9, ink);
    c.seg((cx + 9.4, 18.0), (cx + 10.3, 16.6), 0.9, ink);
}

//! 維吾爾語等阿拉伯字母文字的顯示轉換（#200）。
//!
//! 界面的文字排版（egui）不做字形連寫，也不處理從右往左的方向：維語會顯示成一個個孤立字母、順序還是反的。
//! 這裡在交給 egui 之前做兩步，隻影響顯示，不改歌詞原文：
//! 1. 連寫：按前後字母把每個字母換成 Unicode「阿拉伯字母表現形式」裡的詞首 / 詞中 / 詞尾 / 獨立形式，
//!    「ل + ا」合成一個字形。系統自帶的 Segoe UI（界面首選字體）含全部維語字母的這些形式。
//! 2. 方向：按簡化的雙向文字規則排出顯示順序——阿拉伯字母從右往左，夾在其中的數字、英文、中文保持從左往右，
//!    括號在從右往左的段落裡鏡像。
//! 不含阿拉伯字母的文字原樣返回，中文、英文、韓文、藏文都不受影響。
use std::borrow::Cow;

/// 字母的連寫形式：(獨立, 詞尾, 詞首, 詞中)。詞首 / 詞中為 0 表示只向右連（如 ا د ر و），不與後一個字母相連。
fn forms(c: char) -> Option<[u32; 4]> {
    Some(match c as u32 {
        0x0621 => [0xFE80, 0, 0, 0],
        0x0622 => [0xFE81, 0xFE82, 0, 0],
        0x0623 => [0xFE83, 0xFE84, 0, 0],
        0x0624 => [0xFE85, 0xFE86, 0, 0],
        0x0625 => [0xFE87, 0xFE88, 0, 0],
        0x0626 => [0xFE89, 0xFE8A, 0xFE8B, 0xFE8C],
        0x0627 => [0xFE8D, 0xFE8E, 0, 0],
        0x0628 => [0xFE8F, 0xFE90, 0xFE91, 0xFE92],
        0x0629 => [0xFE93, 0xFE94, 0, 0],
        0x062A => [0xFE95, 0xFE96, 0xFE97, 0xFE98],
        0x062B => [0xFE99, 0xFE9A, 0xFE9B, 0xFE9C],
        0x062C => [0xFE9D, 0xFE9E, 0xFE9F, 0xFEA0],
        0x062D => [0xFEA1, 0xFEA2, 0xFEA3, 0xFEA4],
        0x062E => [0xFEA5, 0xFEA6, 0xFEA7, 0xFEA8],
        0x062F => [0xFEA9, 0xFEAA, 0, 0],
        0x0630 => [0xFEAB, 0xFEAC, 0, 0],
        0x0631 => [0xFEAD, 0xFEAE, 0, 0],
        0x0632 => [0xFEAF, 0xFEB0, 0, 0],
        0x0633 => [0xFEB1, 0xFEB2, 0xFEB3, 0xFEB4],
        0x0634 => [0xFEB5, 0xFEB6, 0xFEB7, 0xFEB8],
        0x0635 => [0xFEB9, 0xFEBA, 0xFEBB, 0xFEBC],
        0x0636 => [0xFEBD, 0xFEBE, 0xFEBF, 0xFEC0],
        0x0637 => [0xFEC1, 0xFEC2, 0xFEC3, 0xFEC4],
        0x0638 => [0xFEC5, 0xFEC6, 0xFEC7, 0xFEC8],
        0x0639 => [0xFEC9, 0xFECA, 0xFECB, 0xFECC],
        0x063A => [0xFECD, 0xFECE, 0xFECF, 0xFED0],
        0x0641 => [0xFED1, 0xFED2, 0xFED3, 0xFED4],
        0x0642 => [0xFED5, 0xFED6, 0xFED7, 0xFED8],
        0x0643 => [0xFED9, 0xFEDA, 0xFEDB, 0xFEDC],
        0x0644 => [0xFEDD, 0xFEDE, 0xFEDF, 0xFEE0],
        0x0645 => [0xFEE1, 0xFEE2, 0xFEE3, 0xFEE4],
        0x0646 => [0xFEE5, 0xFEE6, 0xFEE7, 0xFEE8],
        0x0647 => [0xFEE9, 0xFEEA, 0xFEEB, 0xFEEC],
        0x0648 => [0xFEED, 0xFEEE, 0, 0],
        // ى：維語裡前後都連，用維語 / 哈薩克語專用的詞首、詞中形式。
        0x0649 => [0xFEEF, 0xFEF0, 0xFBE8, 0xFBE9],
        0x064A => [0xFEF1, 0xFEF2, 0xFEF3, 0xFEF4],
        0x067E => [0xFB56, 0xFB57, 0xFB58, 0xFB59], // پ
        0x0686 => [0xFB7A, 0xFB7B, 0xFB7C, 0xFB7D], // چ
        0x0698 => [0xFB8A, 0xFB8B, 0, 0],           // ژ
        0x06A9 => [0xFB8E, 0xFB8F, 0xFB90, 0xFB91], // ک
        0x06AD => [0xFBD3, 0xFBD4, 0xFBD5, 0xFBD6], // ڭ
        0x06AF => [0xFB92, 0xFB93, 0xFB94, 0xFB95], // گ
        0x06BE => [0xFBAA, 0xFBAB, 0xFBAC, 0xFBAD], // ھ
        0x06C6 => [0xFBD9, 0xFBDA, 0, 0],           // ۆ
        0x06C7 => [0xFBD7, 0xFBD8, 0, 0],           // ۇ
        0x06C8 => [0xFBDB, 0xFBDC, 0, 0],           // ۈ
        0x06CB => [0xFBDE, 0xFBDF, 0, 0],           // ۋ
        0x06CC => [0xFBFC, 0xFBFD, 0xFBFE, 0xFBFF], // ی
        0x06D0 => [0xFBE4, 0xFBE5, 0xFBE6, 0xFBE7], // ې
        // ە：維語只向右連，詞尾寫法與 ه 的詞尾形式相同。
        0x06D5 => [0x06D5, 0xFEEA, 0, 0],
        _ => return None,
    })
}

/// 不佔位置的附加符號（阿拉伯語注音符號等）：連寫時跳過，排序時跟著前一個字母走。
fn transparent(c: char) -> bool {
    matches!(c as u32, 0x0610..=0x061A | 0x064B..=0x065F | 0x0670 | 0x06D6..=0x06DC | 0x06DF..=0x06E4 | 0x06E7..=0x06E8 | 0x06EA..=0x06ED)
}

/// 能和前後字母相連的「連接符」：阿拉伯延長線、零寬連接符。
fn join_causing(c: char) -> bool {
    matches!(c, '\u{0640}' | '\u{200D}')
}

fn joins_next(c: char) -> bool {
    join_causing(c) || forms(c).is_some_and(|f| f[2] != 0)
}

fn joins_prev(c: char) -> bool {
    join_causing(c) || forms(c).is_some_and(|f| f[1] != 0)
}

fn lam_alef(alef: char) -> Option<(u32, u32)> {
    Some(match alef as u32 {
        0x0622 => (0xFEF5, 0xFEF6),
        0x0623 => (0xFEF7, 0xFEF8),
        0x0625 => (0xFEF9, 0xFEFA),
        0x0627 => (0xFEFB, 0xFEFC),
        _ => return None,
    })
}

/// 第一步：按前後字母選連寫形式（仍是閱讀順序）。
fn shape(text: &[char]) -> Vec<char> {
    let neighbour = |from: usize, step: isize| -> Option<char> {
        let mut i = from as isize + step;
        while i >= 0 && (i as usize) < text.len() {
            let c = text[i as usize];
            if !transparent(c) { return Some(c); }
            i += step;
        }
        None
    };
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        let c = text[i];
        let Some(f) = forms(c) else { out.push(c); i += 1; continue; };
        let prev = neighbour(i, -1).is_some_and(joins_next);
        // ل + ا 合成一個字形（中間允許夾注音符號）。
        if c == '\u{0644}' {
            let next = (i + 1..text.len()).find(|&j| !transparent(text[j]));
            if let Some((j, (iso, fin))) = next.and_then(|j| lam_alef(text[j]).map(|forms| (j, forms))) {
                out.push(char::from_u32(if prev { fin } else { iso }).unwrap_or(c));
                out.extend(text[i + 1..j].iter().copied());
                i = j + 1;
                continue;
            }
        }
        let next = f[2] != 0 && neighbour(i, 1).is_some_and(joins_prev);
        let code = match (prev && f[1] != 0, next) {
            (true, true) => f[3],
            (true, false) => f[1],
            (false, true) => f[2],
            (false, false) => f[0],
        };
        out.push(char::from_u32(code).unwrap_or(c));
        i += 1;
    }
    out
}

#[derive(Clone, Copy, PartialEq)]
enum Class { L, R, Number, Neutral }

fn class(c: char) -> Class {
    match c as u32 {
        0x0030..=0x0039 | 0x0660..=0x0669 | 0x06F0..=0x06F9 => Class::Number,
        0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFC => Class::R,
        _ if c.is_alphabetic() => Class::L,
        _ => Class::Neutral,
    }
}

fn is_rtl_char(c: char) -> bool { class(c) == Class::R && !transparent(c) }

/// 是否含有需要從右往左顯示的文字（只有這樣的文字才做轉換）。
pub fn has_rtl(text: &str) -> bool { text.chars().any(is_rtl_char) }

fn mirror(c: char) -> char {
    match c {
        '(' => ')', ')' => '(', '[' => ']', ']' => '[', '{' => '}', '}' => '{',
        '<' => '>', '>' => '<', '«' => '»', '»' => '«', '‹' => '›', '›' => '‹',
        _ => c,
    }
}

/// 第二步：一行文字的顯示順序（簡化的 Unicode 雙向文字規則）。
fn reorder(text: &[char]) -> String {
    // 附加符號和前一個字母合成一組，排序時整組移動，不會跑到別的字母上。
    let mut clusters: Vec<Vec<char>> = Vec::new();
    for &c in text {
        match clusters.last_mut() {
            Some(last) if transparent(c) => last.push(c),
            _ => clusters.push(vec![c]),
        }
    }
    let classes: Vec<Class> = clusters.iter().map(|cluster| class(cluster[0])).collect();
    // 段落方向：第一個有方向的字決定。
    let base: u8 = match classes.iter().find(|c| matches!(c, Class::L | Class::R)) { Some(Class::R) => 1, _ => 0 };
    // 判斷中性字符兩側方向時，數字算作從右往左（與 Unicode 規則一致）。
    let strong = |c: Class| match c { Class::L => Some(false), Class::R | Class::Number => Some(true), Class::Neutral => None };
    let edge = base == 1;
    let mut levels = vec![base; clusters.len()];
    for i in 0..clusters.len() {
        levels[i] = match classes[i] {
            Class::R => 1,
            Class::L => if base == 1 { 2 } else { 0 },
            Class::Number => {
                let after_rtl = classes[..i].iter().rev().find_map(|c| match c { Class::L => Some(false), Class::R => Some(true), _ => None }).unwrap_or(edge);
                if base == 1 || after_rtl { 2 } else { 0 }
            }
            Class::Neutral => {
                let before = classes[..i].iter().rev().find_map(|c| strong(*c)).unwrap_or(edge);
                let after = classes[i + 1..].iter().find_map(|c| strong(*c)).unwrap_or(edge);
                match (before, after) {
                    (true, true) => 1,
                    (false, false) => if base == 1 { 2 } else { 0 },
                    _ => base,
                }
            }
        };
    }
    // 行尾空白回到段落方向。
    for i in (0..clusters.len()).rev() {
        if classes[i] != Class::Neutral || !clusters[i][0].is_whitespace() { break; }
        levels[i] = base;
    }
    let mut order: Vec<usize> = (0..clusters.len()).collect();
    let highest = levels.iter().copied().max().unwrap_or(0);
    for level in (1..=highest).rev() {
        let mut i = 0;
        while i < order.len() {
            if levels[order[i]] >= level {
                let start = i;
                while i < order.len() && levels[order[i]] >= level { i += 1; }
                order[start..i].reverse();
            } else {
                i += 1;
            }
        }
    }
    order.into_iter().flat_map(|i| {
        let rtl = levels[i] % 2 == 1;
        clusters[i].iter().map(move |&c| if rtl { mirror(c) } else { c }).collect::<Vec<_>>()
    }).collect()
}

/// 測試導出「修復前」對照圖時關閉轉換。
#[cfg(test)]
pub(crate) static BYPASS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 轉換成顯示用的文字；多行按行分別處理。不含從右往左文字時原樣返回、不分配內存。
pub fn display(text: &str) -> Cow<'_, str> {
    #[cfg(test)]
    if BYPASS.load(std::sync::atomic::Ordering::Relaxed) { return Cow::Borrowed(text); }
    if !has_rtl(text) { return Cow::Borrowed(text); }
    Cow::Owned(text.split('\n').map(|line| {
        let chars: Vec<char> = line.chars().collect();
        reorder(&shape(&chars))
    }).collect::<Vec<_>>().join("\n"))
}

/// 需要折行的文字（島上歌詞、歌詞列表）：從右往左的句子必須先按閱讀順序分行、再逐行轉換，
/// 否則交給 egui 折行時會先顯示句子的結尾。按空格分詞，`width` 量的是轉換後的寬度。
/// 不含從右往左文字時原樣返回，仍由 egui 自己折行。
pub fn display_wrapped(text: &str, max_width: f32, width: impl Fn(&str) -> f32) -> Cow<'_, str> {
    #[cfg(test)]
    if BYPASS.load(std::sync::atomic::Ordering::Relaxed) { return Cow::Borrowed(text); }
    if !has_rtl(text) { return Cow::Borrowed(text); }
    let mut lines: Vec<String> = Vec::new();
    for paragraph in text.split('\n') {
        let mut current = String::new();
        for word in paragraph.split(' ').filter(|w| !w.is_empty()) {
            let candidate = if current.is_empty() { word.to_owned() } else { format!("{current} {word}") };
            if !current.is_empty() && width(&display(&candidate)) > max_width {
                lines.push(std::mem::replace(&mut current, word.to_owned()));
            } else {
                current = candidate;
            }
        }
        lines.push(current);
    }
    Cow::Owned(lines.iter().map(|line| display(line).into_owned()).collect::<Vec<_>>().join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(text: &str) -> Vec<u32> { text.chars().map(|c| c as u32).collect() }

    #[test]
    fn text_without_arabic_script_is_untouched() {
        for text in ["晴天 - 周杰倫", "Hello (World) 123", "사랑해", "བཀྲ་ཤིས་བདེ་ལེགས།", ""] {
            assert!(matches!(display(text), Cow::Borrowed(_)), "{text}");
        }
    }

    #[test]
    fn uyghur_letters_join_and_read_right_to_left() {
        // سىنى（你）：س 詞首、ى 詞中（維語專用形式）、ن 詞中、ى 詞尾；顯示順序從右往左，所以倒過來。
        let shown = display("سىنى");
        assert_eq!(codes(&shown), vec![0xFEF0, 0xFEE8, 0xFBE9, 0xFEB3]);
        // ئۈ：ئ 詞首，ۈ 只向右連，取詞尾形式。
        assert_eq!(codes(&display("ئۈ")), vec![0xFBDC, 0xFE8B]);
        // 只向右連的字母後面另起：دا → د 獨立、ا 獨立。
        assert_eq!(codes(&display("دا")), vec![0xFE8D, 0xFEA9]);
    }

    #[test]
    fn lam_alef_becomes_one_ligature() {
        assert_eq!(codes(&display("لا")), vec![0xFEFB]);
        // 前面有相連的字母時用詞尾合體形式：بلا → ب 詞首 + لا 詞尾合體。
        assert_eq!(codes(&display("بلا")), vec![0xFEFC, 0xFE91]);
    }

    #[test]
    fn words_keep_their_order_from_right_to_left() {
        // 兩個詞：第一個詞顯示在右邊。
        let shown: Vec<char> = display("سىز مەن").chars().collect();
        let space = shown.iter().position(|c| *c == ' ').unwrap();
        // مەن 的 ن 在只向右連的 ە 之後，是獨立形式，顯示在最左邊；سىز 的 س 是詞首，顯示在最右邊。
        assert_eq!(shown[0] as u32, 0xFEE5);
        assert_eq!(*shown.last().unwrap() as u32, 0xFEB3);
        assert!(space > 0 && space < shown.len() - 1);
    }

    #[test]
    fn numbers_and_latin_stay_left_to_right_inside_uyghur() {
        let shown = display("ناخشا 2024 Song");
        assert!(shown.contains("2024"), "數字保持從左往右：{shown}");
        assert!(shown.contains("Song"), "英文保持從左往右：{shown}");
        // 段落從右往左：英文在數字左邊，維語詞在最右邊。
        let song = shown.find("Song").unwrap();
        let year = shown.find("2024").unwrap();
        assert!(song < year, "{shown}");
    }

    #[test]
    fn brackets_are_mirrored_in_right_to_left_text() {
        // 閱讀順序「歌名（歌詞）」從右往左顯示：括號內容在左側，鏡像後最左邊是「(」、括號內容右邊是「)」。
        let shown = display("ناخشا (سۆز)");
        let open = shown.find('(').unwrap();
        let close = shown.find(')').unwrap();
        assert_eq!(open, 0, "{shown}");
        assert!(close > open && shown[close..].contains(' '), "括號閉合在維語詞之前：{shown}");
    }

    #[test]
    fn chinese_line_with_an_uyghur_word_keeps_chinese_order() {
        let shown = display("歌曲 ناخشا 好聽");
        assert!(shown.starts_with("歌曲 ") && shown.ends_with(" 好聽"), "段落以中文開頭時仍從左往右：{shown}");
    }

    #[test]
    fn truncation_ellipsis_ends_up_on_the_left() {
        // 截斷按閱讀順序在末尾加省略號；從右往左顯示時，閱讀的末尾在最左邊。
        let shown = display("سۆيگۈ دېگەن شۇنداق…");
        assert!(shown.starts_with('…'), "{shown}");
    }

    #[test]
    fn marks_stay_with_their_letter() {
        // بَ：注音符號緊跟在字母後面，反轉後也不能跑到另一個字母上。
        let shown: Vec<char> = display("بَت").chars().collect();
        let mark = shown.iter().position(|c| *c == '\u{064E}').unwrap();
        assert_eq!(shown[mark - 1] as u32, 0xFE91, "符號緊跟在 ب 後面：{shown:?}");
    }

    #[test]
    fn wrapping_splits_by_reading_order_before_reversing() {
        // 每個詞算 10 寬，一行最多 25：前兩個詞一行、第三個詞下一行。
        let width = |s: &str| s.split(' ').count() as f32 * 10.0;
        let shown = display_wrapped("بىر ئىككى ئۈچ", 25.0, width);
        let lines: Vec<&str> = shown.split('\n').collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], display("بىر ئىككى"), "第一行是句子開頭的兩個詞");
        assert_eq!(lines[1], display("ئۈچ"));
        assert!(matches!(display_wrapped("中文歌詞", 10.0, width), Cow::Borrowed(_)));
    }
}

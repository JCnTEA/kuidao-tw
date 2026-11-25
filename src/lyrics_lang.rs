//! 歌詞語言判讀：先看歌名（必要時看歌手）屬於哪個國家／語言，再檢查抓回來的歌詞是不是同一種語言。
//!
//! 會遇到的兩種錯版：日本歌抓到中文翻譯、韓文歌抓到中文翻譯（網易雲、QQ 音樂很常見），
//! 以及日本／韓國歌抓到整份羅馬拼音。兩種都先當備選、繼續往下一個來源找原文，
//! 全部都沒有原文時才退回備選，至少島上還能逐句顯示。
//!
//! 判讀只看字元種類（假名、諺文、漢字、拉丁字母），不看歌詞內容，不需要連網也不會誤判語意。

use crate::lyrics::{is_credit, Lyrics};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Japanese,
    Korean,
    Chinese,
    Latin,
    Unknown,
}

fn is_kana(c: char) -> bool {
    matches!(c, '\u{3040}'..='\u{30FF}' | '\u{31F0}'..='\u{31FF}')
}

fn is_hangul(c: char) -> bool {
    matches!(c, '\u{AC00}'..='\u{D7A3}' | '\u{1100}'..='\u{11FF}' | '\u{3130}'..='\u{318F}')
}

fn is_han(c: char) -> bool {
    matches!(c, '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}')
}

/// 一段文字以哪種字元為主；標題通常很短，所以只要出現假名或諺文就直接判定。
pub fn of_text(text: &str) -> Lang {
    let (mut kana, mut hangul, mut han, mut latin) = (0usize, 0usize, 0usize, 0usize);
    for c in text.chars() {
        if is_kana(c) {
            kana += 1;
        } else if is_hangul(c) {
            hangul += 1;
        } else if is_han(c) {
            han += 1;
        } else if c.is_ascii_alphabetic() {
            latin += 1;
        }
    }
    if kana > 0 && kana * 10 >= kana + han + hangul + latin {
        return Lang::Japanese;
    }
    if hangul > 0 {
        return Lang::Korean;
    }
    if han > 0 {
        return Lang::Chinese;
    }
    if latin > 0 {
        return Lang::Latin;
    }
    Lang::Unknown
}

/// 這首歌「應該是什麼語言」：以歌名為準，歌名看不出來（例如只有符號或空字串）才看歌手。
pub fn expected(title: &str, artist: &str) -> Lang {
    match of_text(title) {
        Lang::Unknown => of_text(artist),
        lang => lang,
    }
}

struct Counts {
    kana: usize,
    hangul: usize,
    han: usize,
    latin: usize,
}

impl Counts {
    fn letters(&self) -> usize {
        self.kana + self.hangul + self.han + self.latin
    }
}

fn count(lyrics: &Lyrics) -> Counts {
    let mut counts = Counts { kana: 0, hangul: 0, han: 0, latin: 0 };
    // 署名行（作詞／作曲／翻譯）本來就常混語言，不列入判斷。
    for line in lyrics.lines.iter().filter(|line| !line.text.is_empty() && !is_credit(&line.text)) {
        for c in line.text.chars() {
            if is_kana(c) {
                counts.kana += 1;
            } else if is_hangul(c) {
                counts.hangul += 1;
            } else if is_han(c) {
                counts.han += 1;
            } else if c.is_ascii_alphabetic() {
                counts.latin += 1;
            }
        }
    }
    counts
}

/// 抓回來的歌詞和歌名語言不合時的說明；合得來回 None。
/// 只擋「明顯是另一種語言」的版本，判斷不了（Unknown、歌詞太短）一律放行。
pub fn mismatch(title: &str, artist: &str, lyrics: &Lyrics) -> Option<&'static str> {
    let want = expected(title, artist);
    let counts = count(lyrics);
    let letters = counts.letters();
    if letters < 40 {
        return None;
    }
    let share = |value: usize| value as f32 / letters as f32;
    match want {
        Lang::Japanese => {
            // 日文原文一定有假名；只有漢字就是中文翻譯，只有拉丁字母就是羅馬拼音。
            if counts.kana == 0 {
                if counts.latin * 2 >= letters {
                    return Some("羅馬拼音");
                }
                if counts.han > 0 {
                    return Some("中文翻譯");
                }
                return None;
            }
            if share(counts.kana) < 0.02 {
                return Some("假名比例太低，疑似翻譯");
            }
            None
        }
        Lang::Korean => {
            // 韓文原文一定有諺文；沒有諺文時分辨羅馬拼音與中文翻譯。
            if counts.hangul == 0 {
                if counts.latin * 2 >= letters {
                    return Some("羅馬拼音");
                }
                if counts.han > 0 {
                    return Some("中文翻譯");
                }
                return None;
            }
            if share(counts.hangul) < 0.02 {
                return Some("諺文比例太低，疑似翻譯");
            }
            None
        }
        // 中文歌名不擋：粵語、台語、國語都是漢字，純英文歌詞的中文歌也存在。
        // 英文等拉丁歌名也不硬擋（中文歌取英文名很常見），但整份都是漢字時可以在診斷裡看出來。
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lyrics(lines: &[&str]) -> Lyrics {
        Lyrics {
            synced: true,
            lines: lines
                .iter()
                .map(|text| crate::lyrics::LyricsLine { t: 0.0, text: text.to_string() })
                .collect(),
        }
    }

    #[test]
    fn language_comes_from_the_title_first() {
        assert_eq!(expected("夜に駆ける", "YOASOBI"), Lang::Japanese);
        assert_eq!(expected("눈,코,입", "태양"), Lang::Korean);
        assert_eq!(expected("Dynamite", "방탄소년단"), Lang::Latin, "英文歌名就照標題走，不硬套歌手語言");
        assert_eq!(expected("夢中的情話", "江蕙"), Lang::Chinese);
        assert_eq!(expected("Blinding Lights", "The Weeknd"), Lang::Latin);
        assert_eq!(expected("♪", ""), Lang::Unknown);
    }

    #[test]
    fn japanese_song_rejects_chinese_translation() {
        // 網易雲的日文歌常常回整份中文翻譯：沒有假名、全是漢字。
        let translated = lyrics(&[
            "夜晚奔跑", "只有我與你兩人", "在無人知曉的街道上", "朝著黎明奔跑而去", "直到呼吸停止",
            "再也不能回頭", "就這樣一直跑到天亮", "把所有的聲音都留在身後", "風從耳邊穿過去",
            "心跳蓋過了整座城市",
        ]);
        assert_eq!(mismatch("夜に駆ける", "YOASOBI", &translated), Some("中文翻譯"));
        let original = lyrics(&[
            "夜に駆ける", "沈むように溶けてゆくように", "二人だけの空が広がる夜に", "「さよなら」だけが",
            "消せないままでいる",
        ]);
        assert!(mismatch("夜に駆ける", "YOASOBI", &original).is_none(), "日文原文要放行");
    }

    #[test]
    fn korean_song_rejects_romanization_and_translation() {
        let romaji = lyrics(&[
            "nune koppip", "sum swineun neoui nunbit", "neoui sumgyeol hanakkaji", "geu eoneu nugudeun",
            "nareul ttara hae", "mianhae mianhae hajima", "nado neol bogo sipeo", "ireoke rado neol bonda",
            "neoui nunbit sogen", "naega salgo isseo",
        ]);
        assert_eq!(mismatch("눈,코,입", "태양", &romaji), Some("羅馬拼音"));
        let original = lyrics(&[
            "눈, 코, 입", "미안해 미안해 하지마", "나도 널 보고 싶어", "널 보고 싶어", "이렇게라도 널 본다",
        ]);
        assert!(mismatch("눈,코,입", "태양", &original).is_none(), "韓文原文要放行");
    }

    #[test]
    fn short_lyrics_and_unknown_titles_are_never_blocked() {
        let tiny = lyrics(&["短歌"]);
        assert!(mismatch("夜に駆ける", "YOASOBI", &tiny).is_none(), "太短判斷不了就放行");
        let any = lyrics(&["anything goes here", "another line of the same song", "third line keeps going on"]);
        assert!(mismatch("♪", "", &any).is_none(), "歌名看不出語言時不擋");
    }
}

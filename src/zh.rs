//! 繁體化：以 OpenCC 的簡→繁對照表做字級轉換，再用台灣常用字形校正一次。
//! 只轉漢字，不動標點、英數與假名；歌詞靠這層在顯示時統一成繁體。

use std::collections::HashMap;
use std::sync::OnceLock;

const ST_RAW:&str=include_str!("../assets/zh/STCharacters.txt");
const TW_RAW:&str=include_str!("../assets/zh/TWVariants.txt");

fn parse(raw:&str)->HashMap<char,char>{
    let mut map=HashMap::new();
    for line in raw.lines(){
        let line=line.trim_end_matches('\r');
        if line.is_empty()||line.starts_with('#'){continue;}
        let mut parts=line.split('\t');
        let Some(key)=parts.next() else {continue;};
        let Some(value)=parts.next() else {continue;};
        let mut key=key.chars();
        let (Some(from),None)=(key.next(),key.next()) else {continue;};
        // 一對多時取第一個候選（OpenCC 的常用寫法）。
        let Some(to)=value.split_whitespace().next().and_then(|value|value.chars().next()) else {continue;};
        map.insert(from,to);
    }
    map
}

fn tables()->(&'static HashMap<char,char>,&'static HashMap<char,char>){
    static ST:OnceLock<HashMap<char,char>>=OnceLock::new();
    static TW:OnceLock<HashMap<char,char>>=OnceLock::new();
    (ST.get_or_init(||parse(ST_RAW)),TW.get_or_init(||parse(TW_RAW)))
}

pub fn is_han(c:char)->bool{matches!(c,'\u{3400}'..='\u{4DBF}'|'\u{4E00}'..='\u{9FFF}'|'\u{F900}'..='\u{FAFF}')}

/// 日文假名、韓文：出現這些文字的歌詞不是中文歌，不做繁簡轉換。
pub fn has_kana_or_hangul(c:char)->bool{
    matches!(c,'\u{3040}'..='\u{30FF}'|'\u{AC00}'..='\u{D7A3}'|'\u{1100}'..='\u{11FF}'|'\u{3130}'..='\u{318F}')
}

/// 整句轉成繁體（已是繁體的字會原樣保留）。
pub fn to_traditional(text:&str)->String{
    let (st,tw)=tables();
    let mut out=String::with_capacity(text.len());
    for c in text.chars(){
        let mid=st.get(&c).copied().unwrap_or(c);
        out.push(tw.get(&mid).copied().unwrap_or(mid));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_common_simplified_characters(){
        assert_eq!(to_traditional("我们在这里等你"),"我們在這裡等你");
        assert_eq!(to_traditional("说好的幸福呢"),"說好的幸福呢");
        assert_eq!(to_traditional("时间都去哪儿了"),"時間都去哪兒了");
    }

    #[test]
    fn keeps_traditional_and_other_scripts(){
        assert_eq!(to_traditional("我們在這裡"),"我們在這裡");
        assert_eq!(to_traditional("Hello 世界 123"),"Hello 世界 123");
        assert_eq!(to_traditional("君の名は"),"君の名は");
    }

    #[test]
    fn table_covers_common_lyrics_vocabulary(){
        let (st,_)=tables();
        for (from,to) in [('这','這'),('为','為'),('个','個'),('国','國'),('说','說'),('时','時'),('会','會'),('来','來'),('对','對'),('开','開'),('关','關'),('门','門'),('东','東'),('马','馬'),('鸟','鳥'),('龙','龍'),('见','見'),('觉','覺'),('让','讓'),('读','讀'),('写','寫'),('话','話'),('谁','誰'),('认','認'),('识','識'),('变','變'),('边','邊'),('远','遠'),('还','還'),('进','進'),('过','過'),('们','們'),('么','麼'),('样','樣'),('没','沒')] {
            assert_eq!(to_traditional(&from.to_string()),to.to_string(),"缺少 {from}");
        }
    }
}

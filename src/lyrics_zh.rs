//! 非中文歌詞的自動翻譯：設定開啟後，把整首歌詞送去翻譯並依原行對齊，結果快取在 lyrics/zh。
//! 中文歌（含日文、韓文原文）不翻：中文歌只做繁體化，日韓原文保持原樣並可另行翻譯。

use crate::lyrics::Lyrics;
use std::io::Read;

const ENDPOINT:&str="https://translate.googleapis.com/translate_a/single?client=gtx&sl=auto&tl=zh-TW&dt=t";
const MAX_CACHE:usize=1024*1024;
const CHUNK_CHARS:usize=700;
const CHUNK_LINES:usize=28;

fn path(track:&str,artist:&str,duration:f64)->std::path::PathBuf{
    crate::lyrics::cache_path(track,artist,duration).with_extension("zh.json")
}

/// 已快取的譯文；長度與歌詞行數對不上時視為無效。
pub fn cached(track:&str,artist:&str,duration:f64,lines:usize)->Option<Vec<String>>{
    let file=std::fs::File::open(path(track,artist,duration)).ok()?;
    if file.metadata().ok()?.len()>MAX_CACHE as u64{return None;}
    let mut raw=String::new();
    file.take(MAX_CACHE as u64).read_to_string(&mut raw).ok()?;
    let list:Vec<String>=serde_json::from_str(&raw).ok()?;
    (list.len()==lines).then_some(list)
}

/// 只在設定開啟、且這首不是中文歌時才翻譯；已有快取就直接沿用。
/// 由歌詞查詢的背景執行緒呼叫，不阻塞介面。
pub fn ensure(track:&str,artist:&str,duration:f64,lyrics:&Lyrics){
    if lyrics.lines.is_empty()||crate::lyrics::is_chinese_lyrics(lyrics){return;}
    if cached(track,artist,duration,lyrics.lines.len()).is_some(){return;}
    let texts:Vec<String>=lyrics.lines.iter().map(|line|line.text.trim().to_owned()).collect();
    let Some(translated)=translate(&texts) else {return;};
    let raw=serde_json::to_string(&translated).unwrap_or_default();
    if raw.len()>MAX_CACHE{return;}
    let path=path(track,artist,duration);
    if let Some(parent)=path.parent(){let _=std::fs::create_dir_all(parent);}
    let _=crate::model::atomic_write(&path,raw.as_bytes());
}

fn translate(lines:&[String])->Option<Vec<String>>{
    translate_with(lines,request)
}

/// 只送非空行，回填時保留原行索引；空行、純符號行不會被 Google 合併而造成整首翻譯失敗。
fn translate_with(lines:&[String],mut request:impl FnMut(&[String])->Option<Vec<String>>)->Option<Vec<String>>{
    let active:Vec<(usize,&String)>=lines.iter().enumerate().filter(|(_,line)|!line.trim().is_empty()).collect();
    let mut output=vec![String::new();lines.len()];
    let mut start=0usize;
    while start<active.len(){
        let mut end=start;
        let mut chars=0usize;
        while end<active.len()&&end-start<CHUNK_LINES{
            let add=active[end].1.chars().count()+1;
            if end>start&&chars+add>CHUNK_CHARS{break;}
            chars+=add;end+=1;
        }
        let chunk:Vec<String>=active[start..end].iter().map(|(_,line)|(*line).clone()).collect();
        let translated=request(&chunk)?;
        if translated.len()!=chunk.len(){return None;}
        for ((index,_),text) in active[start..end].iter().zip(translated){
            output[*index]=polish_translation(&text);
        }
        start=end;
        if start<active.len(){std::thread::sleep(std::time::Duration::from_millis(120));}
    }
    Some(output)
}

/// 機翻常見的排版問題：CJK 之間被插入空格、連續標點、行首殘留逗號；統一轉繁體後再修。
fn polish_translation(text:&str)->String{
    let traditional=crate::zh::to_traditional(text);
    let trimmed=traditional.trim().trim_start_matches(|c:char|matches!(c,'，'|','|'。'|'！'|'!'|'？'|'?'|'、'|'；'|';'|'：'|':'));
    let mut out=String::new();
    let mut previous=None;
    for c in trimmed.chars(){
        if c==' ' {
            if previous.is_some_and(no_space_after)&&out.chars().next().is_some(){continue;}
        }
        if matches!(c,'，'|','|'。'|'！'|'？'|'、'|'；'|'：')&&out.chars().last().is_some_and(|last|punctuation_family(last)==punctuation_family(c)&&punctuation_family(c)>0){continue;}
        out.push(c);
        previous=Some(c);
    }
    let mut chars=out.chars().peekable();
    let mut clean=String::new();
    while let Some(c)=chars.next(){
        if c==',' {clean.push('，');}
        else if c=='.'&&chars.peek().is_none(){clean.push('。');}
        else if c==';' {clean.push('；');}
        else if c==':' {clean.push('：');}
        else if c=='!' {clean.push('！');}
        else if c=='?' {clean.push('？');}
        else {clean.push(c);}
    }
    clean.trim().to_string()
}

fn is_cjk_or_fullwidth(c:char)->bool{
    matches!(c as u32,0x3000..=0x303F|0x3400..=0x4DBF|0x4E00..=0x9FFF|0xFF00..=0xFFEF)
}

/// 中文排版不在標點後留半形空格；英文逗號、句號之後也一併收掉，避免轉全形後變成「， world」。
fn no_space_after(c:char)->bool{
    is_cjk_or_fullwidth(c)||matches!(c,','|'.'|';'|':'|'!'|'?')
}

fn punctuation_family(c:char)->u8{
    match c {
        '，'|','=>1,'。'|'.'=>2,'！'|'!'=>3,'？'|'?'=>4,'、'=>5,'；'|';'=>6,'：'|':'=>7,_=>0,
    }
}

fn request(lines:&[String])->Option<Vec<String>>{
    let joined=lines.join("\n");
    let mut body=String::with_capacity(joined.len()*3+2);
    body.push_str("q=");
    for c in joined.chars(){push_encoded(&mut body,c);}
    let response=ureq::post(ENDPOINT)
        .set("User-Agent",concat!("Kuidao/",env!("CARGO_PKG_VERSION")," (Windows)"))
        .set("Content-Type","application/x-www-form-urlencoded;charset=utf-8")
        .timeout(std::time::Duration::from_secs(8))
        .send_string(&body).ok()?;
    let mut text=String::new();
    response.into_reader().take(MAX_CACHE as u64).read_to_string(&mut text).ok()?;
    let json:serde_json::Value=serde_json::from_str(&text).ok()?;
    let segments=json.get(0)?.as_array()?;
    let mut merged=String::new();
    for segment in segments{
        if let Some(part)=segment.get(0).and_then(|value|value.as_str()){merged.push_str(part);}
    }
    if merged.trim().is_empty(){return None;}
    let parts:Vec<String>=merged.split('\n').map(|part|part.trim_end_matches('\r').to_owned()).collect();
    (parts.len()==lines.len()).then_some(parts)
}

fn push_encoded(out:&mut String,c:char){
    if c.is_ascii_alphanumeric()||matches!(c,'-'|'_'|'.'|'~'){
        out.push(c);
    }else if c==' '{
        out.push('+');
    }else{
        let mut buffer=[0u8;4];
        for byte in c.encode_utf8(&mut buffer).as_bytes(){
            out.push('%');
            out.push_str(&format!("{byte:02X}"));
        }
    }
}

/// 把每行換成要顯示的字：中文歌 → 繁體；非中文歌且開啟翻譯 → 譯文；否則原文。
pub fn display_lines(lyrics:&Lyrics,translate:bool,track:&str,artist:&str,duration:f64)->Vec<String>{
    let chinese=crate::lyrics::is_chinese_lyrics(lyrics);
    let translations=if translate&&!chinese{cached(track,artist,duration,lyrics.lines.len())}else{None};
    lyrics.lines.iter().enumerate().map(|(index,line)|{
        if line.text.trim().is_empty(){return String::new();}
        if let Some(text)=translations.as_ref().and_then(|list|list.get(index)).filter(|text|!text.trim().is_empty()){
            return text.clone();
        }
        if chinese {crate::zh::to_traditional(&line.text)} else {line.text.clone()}
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_lines_keep_alignment_and_are_not_sent() {
        let lines = vec!["hello".to_string(), "".into(), "world".into()];
        let shown = translate_with(&lines, |chunk| Some(chunk.iter().map(|_| "你 好 ，".to_string()).collect())).unwrap();
        assert_eq!(shown, vec!["你好，".to_string(), "".to_string(), "你好，".to_string()]);
    }

    #[test]
    fn machine_translation_spacing_and_punctuation_are_cleaned() {
        assert_eq!(polish_translation(" 你 好 ， 世 界 。"), "你好，世界。");
        let input = format!("{}第一句{}第二句。", "，", "，".repeat(2));
        assert_eq!(polish_translation(&input), "第一句，第二句。");
        assert_eq!(polish_translation("Hello, world."), "Hello，world。");
        assert_eq!(polish_translation("我 愛 你"), "我愛你");
    }

    #[test]
    fn display_lines_turns_chinese_into_traditional(){
        let lyrics=Lyrics{synced:true,lines:vec![
            crate::lyrics::LyricsLine{t:1.0,text:"我们都一样".into()},
            crate::lyrics::LyricsLine{t:3.0,text:"".into()},
        ]};
        let shown=display_lines(&lyrics,true,"我们都一样","歌手",180.0);
        assert_eq!(shown[0],"我們都一樣");
        assert_eq!(shown[1],"");
    }

    #[test]
    fn english_lyrics_stay_original_without_cache(){
        let lyrics=Lyrics{synced:true,lines:vec![crate::lyrics::LyricsLine{t:1.0,text:"Hello from the other side".into()}]};
        assert_eq!(display_lines(&lyrics,false,"Hello","Adele",295.0)[0],"Hello from the other side");
    }

    #[test]
    fn encodes_query_safely(){
        let mut out=String::new();
        push_encoded(&mut out,'中');
        assert_eq!(out,"%E4%B8%AD");
        let mut out=String::new();
        push_encoded(&mut out,'&');
        assert_eq!(out,"%26");
    }
}


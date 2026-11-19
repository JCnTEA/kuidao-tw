//! 音乐识别缺口记录：一首歌播放一段时间后，歌手、时长、封面、类型等仍缺哪一样，就在
//! `diagnostics/music-gaps.log` 记一行（每首歌最多一行，信息齐全的歌不记）。
//! 用户反馈「没封面」「没歌词」时，诊断包里能直接看出是哪一层缺了：播放器没给，还是我们没补上。
use super::{MediaKind,NowPlaying};
use std::time::{Duration,Instant};

/// 等这么久再判断：系统缩略图、本机补全和在线补全都是异步的，刚换歌时缺东西是正常的。
pub(crate) const SETTLE:Duration=Duration::from_secs(15);

/// 一首歌最终来自哪里：系统媒体会话，还是播放器本地记录（网易云日志、波点、酷我）。
pub(crate) fn origin(media:&NowPlaying)->&'static str {if media.song_id.is_some(){"播放器本機記錄"}else{"系統媒體會話"}}

/// 标题像节目、电台名而不是歌名（常见于电台、FM、DJ 模式）。
fn looks_like_program(title:&str)->bool {
    let lower=title.to_lowercase();
    ["電臺","廣播","fm","radio","podcast","播客","節目","dj","有聲"].iter().any(|word|lower.contains(word))
}

/// 这首歌缺了哪些信息；全都有时返回空。
pub(crate) fn gaps(media:&NowPlaying)->Vec<&'static str> {
    let mut gaps=Vec::new();
    if media.artist.trim().is_empty(){gaps.push("歌手");}
    if media.duration<=0.0{gaps.push("時長");}
    if media.artwork.is_none(){gaps.push("封面");}
    if media.kind()!=MediaKind::Music{gaps.push(if media.playback_type==MediaKind::Video{"型別為影片"}else{"型別未標明"});}
    if media.sampled_at.is_none(){gaps.push("進度");}
    if looks_like_program(&media.title){gaps.push("標題疑似節目名");}
    gaps
}

#[derive(Default)]
pub(crate) struct Tracker {key:String,since:Option<Instant>,done:bool}
impl Tracker {
    /// 每次轮询调用；到了该判断的时候返回要写入的一行（不含时间戳）。
    pub(crate) fn observe(&mut self,media:&NowPlaying,now:Instant)->Option<String> {
        if media.title.is_empty(){*self=Self::default();return None;}
        let key=format!("{}|{}|{}|{:?}",media.source,media.title,media.artist,media.song_id);
        if key!=self.key{*self=Self{key,since:Some(now),done:false};return None;}
        // 暂停中不算：播放器暂停时常常不更新封面、进度。
        if self.done || !media.playing || self.since.is_none_or(|since|now.duration_since(since)<SETTLE){return None;}
        self.done=true;
        let gaps=gaps(media);
        if gaps.is_empty(){return None;}
        if !media.is_music(){
            return Some(format!("{} · 來源 {} · 缺 {}",media.source,origin(media),gaps.join("、")));
        }
        let id=media.song_id.map(|id|format!(" ID={id}")).unwrap_or_default();
        Some(format!("{} · {} - {}{id} · 來源 {} · 缺 {}",media.source,media.title,
            if media.artist.is_empty(){"（無歌手）"}else{&media.artist},origin(media),gaps.join("、")))
    }
}

/// 写一行缺口记录，附上播放器版本（同一首歌只会写一次，读版本的开销可以忽略）。
pub(crate) fn record(line:&str,source:&str) {
    if cfg!(test){return;}
    let version=super::music_log::player_exe(source).and_then(|exe|{
        let pid=*super::player_process::pids_named(exe).first()?;
        Some(format!(" · {exe} {}",super::music_log::file_version(&super::player_process::image_path(pid)?)))
    }).unwrap_or_default();
    let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    crate::resources::append_log(&crate::model::config_dir().join("diagnostics"),"music-gaps.log",&format!("{now} {line}{version}"));
}

#[cfg(test)]
mod tests {
    use super::*;
    fn song(title:&str,artist:&str)->NowPlaying{
        NowPlaying{title:title.into(),artist:artist.into(),source:"qqmusic".into(),playing:true,duration:200.0,
            playback_type:MediaKind::Music,sampled_at:Some(Instant::now()),
            artwork:Some(std::sync::Arc::new(eframe::egui::ColorImage::new([1,1],eframe::egui::Color32::RED))),..Default::default()}
    }
    #[test]
    fn video_gap_report_omits_the_title_and_creator(){
        let mut media=song("會議回放","演示帳號");
        media.playback_type=MediaKind::Video;
        let mut tracker=Tracker::default();
        let start=Instant::now();
        tracker.observe(&media,start);
        let line=tracker.observe(&media,start+SETTLE).unwrap();
        assert!(line.contains("型別為影片"));
        assert!(!line.contains(&media.title) && !line.contains(&media.artist));
    }
    #[test]
    fn complete_songs_are_not_recorded_and_gaps_are_named(){
        assert!(gaps(&song("晴天","周杰倫")).is_empty());
        let mut radio=song("週末音樂電臺","");radio.artwork=None;radio.duration=0.0;
        assert_eq!(gaps(&radio),vec!["歌手","時長","封面","標題疑似節目名"]);
    }
    #[test]
    fn each_song_is_judged_once_after_it_settles_while_playing(){
        let start=Instant::now();let mut tracker=Tracker::default();
        let mut media=song("晴天","");
        assert!(tracker.observe(&media,start).is_none(),"剛換歌不判斷");
        assert!(tracker.observe(&media,start+Duration::from_secs(5)).is_none(),"補全還沒完成時不判斷");
        media.playing=false;
        assert!(tracker.observe(&media,start+SETTLE).is_none(),"暫停中不判斷");
        media.playing=true;
        let line=tracker.observe(&media,start+SETTLE).unwrap();
        assert!(line.contains("缺 歌手") && line.contains("來源 系統媒體會話"),"{line}");
        assert!(tracker.observe(&media,start+SETTLE*2).is_none(),"同一首隻記一次");
        let next=song("七里香","周杰倫");
        assert!(tracker.observe(&next,start+SETTLE*2).is_none());
        assert!(tracker.observe(&next,start+SETTLE*3).is_none(),"資訊齊全的歌不記");
        assert!(tracker.observe(&NowPlaying::default(),start+SETTLE*3).is_none());
    }
}

//! 换歌与歌曲信息补全时的歌词状态：不串歌、不闪烁、不叠加网络线程。
use super::*;
use crate::smtc::NowPlaying;

fn song(source:&str,title:&str,artist:&str,duration:f64)->NowPlaying{
    // 播放器上报为音乐：歌手暂时为空时仍按歌曲处理（与真实客户端一致）。
    NowPlaying{source:source.into(),title:title.into(),artist:artist.into(),duration,playing:true,position:7.0,
        playback_type:crate::smtc::MediaKind::Music,..Default::default()}
}

fn lyrics_on(h:&mut H){h.app.opts.fetch_lyrics=true;h.app.settings.island_lyrics=true;h.app.show_lyrics=true;}

/// 用一个阻塞中的假线程占住网络名额：排队的查询不会真的联网。
fn busy(h:&mut H)->(std::sync::mpsc::Sender<()>,std::thread::ThreadId){
    let (release,wait)=std::sync::mpsc::channel::<()>();
    let worker=std::thread::spawn(move||{let _=wait.recv();});
    let id=worker.thread().id();h.app.ly_worker=Some(worker);(release,id)
}

fn finish(h:&mut H,release:std::sync::mpsc::Sender<()>){
    release.send(()).unwrap();h.app.ly_worker.take().unwrap().join().unwrap();
}

/// 模拟当前歌曲已查到同步歌词。
fn showing(h:&mut H,media:&NowPlaying,lrc:&str){
    h.media=media.clone();h.frame(vec![]);
    h.app.ly=crate::app::LyricsState{
        key:format!("{}|{}|{:.0}",media.title,media.artist,media.duration),
        song:Some(crate::app::LyricSong{source:media.source.clone(),title:media.title.clone(),artist:media.artist.clone(),duration:media.duration,song_id:media.song_id}),
        lyrics:Some(crate::lyrics::parse_lrc(lrc)),..Default::default()
    };
    h.frame(vec![]);
}

#[test]
fn switching_songs_clears_old_lyrics_immediately_even_while_a_request_is_running(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();lyrics_on(&mut h);
    let (release,id)=busy(&mut h);
    showing(&mut h,&song("QQMusic.exe","舊歌","歌手",200.0),"[00:01]舊歌詞\n[00:05]第二句");
    assert_eq!(h.app.island_lyric().map(|(_,line)|line),Some("第二句"));
    let (late,rx)=std::sync::mpsc::channel();h.app.ly.rx=Some(rx);
    h.media=song("QQMusic.exe","新歌","歌手",180.0);h.frame(vec![]);
    assert!(h.app.ly.lyrics.is_none(),"換歌后不能繼續顯示上一首的歌詞");
    assert!(h.app.island_lyric().is_none());
    assert!(h.app.ly.loading,"新歌應立即顯示查詢狀態，不等舊請求結束");
    assert!(h.app.ly.key.starts_with("新歌|"));
    assert!(late.send(Ok((crate::lyrics::parse_lrc("[00:01]遲到的舊歌詞"),false))).is_err(),"上一首遲到的結果必須丟棄");
    assert!(h.app.ly.pending.is_some() && h.app.ly.rx.is_none(),"線上查詢排隊，等舊請求結束");
    assert_eq!(h.app.ly_worker.as_ref().unwrap().thread().id(),id,"舊請求未結束時不得另開取詞執行緒");
    finish(&mut h,release);
}

#[test]
fn filled_in_duration_keeps_synced_lyrics_without_a_new_lookup(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();lyrics_on(&mut h);
    let (release,_)=busy(&mut h);
    let mut media=song("QQMusic.exe","晴天","周杰倫",0.0);
    showing(&mut h,&media,"[00:01]故事的小黃花\n[00:05]從出生那年就飄著");
    media.duration=269.0;h.media=media.clone();h.frame(vec![]);
    assert_eq!(h.app.ly.key,"晴天|周杰倫|269");
    assert_eq!(h.app.ly.lyrics.as_ref().unwrap().lines[1].text,"從出生那年就飄著","補全時長不能清空歌詞");
    assert!(!h.app.ly.loading && h.app.ly.pending.is_none(),"已有同步歌詞時補全時長不再查詢");
    assert_eq!(h.app.island_lyric().map(|(_,line)|line),Some("從出生那年就飄著"));
    // 歌手从空补全、写法变化同样视为同一首。
    let mut blank=song("QQMusic.exe","晴天","",269.0);showing(&mut h,&blank,"[00:05]從出生那年就飄著");
    blank.artist="周杰倫".into();h.media=blank;h.frame(vec![]);
    assert!(h.app.ly.lyrics.is_some() && h.app.ly.pending.is_none());
    finish(&mut h,release);
}

#[test]
fn completed_artist_credits_requery_but_keep_showing_current_lyrics(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();lyrics_on(&mut h);
    let (release,_)=busy(&mut h);
    let mut media=song("QQMusic.exe","有點甜","汪蘇瀧",230.0);
    showing(&mut h,&media,"[00:01]第一句\n[00:05]我的心");
    media.artist="汪蘇瀧、BY2".into();h.media=media;h.frame(vec![]);
    assert_eq!(h.app.ly.lyrics.as_ref().unwrap().lines[1].text,"我的心","查詢期間繼續顯示已有歌詞");
    assert!(!h.app.ly.loading,"已有歌詞時不閃查詢狀態");
    assert!(h.app.ly.pending.is_some(),"署名補全後按完整署名重新查詢");
    assert_eq!(h.app.island_lyric().map(|(_,line)|line),Some("我的心"));
    finish(&mut h,release);
}

#[test]
fn same_title_with_other_version_or_artist_is_a_different_song(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for next in [song("QQMusic.exe","Track","甲",300.0),song("QQMusic.exe","Track","丙",240.0),song("QQMusic.exe","Track (Live)","甲",240.0)] {
        let mut h=H::new();lyrics_on(&mut h);let (release,_)=busy(&mut h);
        showing(&mut h,&song("QQMusic.exe","Track","甲",240.0),"[00:01]原版歌詞");
        h.media=next.clone();h.frame(vec![]);
        assert!(h.app.ly.lyrics.is_none(),"{} - {}（{} 秒）不能沿用原版歌詞",next.title,next.artist,next.duration);
        finish(&mut h,release);
    }
}

#[test]
fn native_lyrics_polling_starts_on_switch_without_waiting_for_the_old_request(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for source in ["汽水音樂","KuGou.exe"] {
        let mut h=H::new();lyrics_on(&mut h);let (release,_)=busy(&mut h);
        showing(&mut h,&song(source,"舊歌","歌手",200.0),"[00:01]舊歌詞");
        let room=crate::resources::NATIVE_LYRICS.active()<2;
        h.media=song(source,"階段二測試歌曲","不存在的歌手",180.0);h.frame(vec![]);
        if room {assert!(h.app.ly.native_rx.is_some(),"{source}：本機歌詞輪詢應立即開始");}
        assert!(h.app.ly.pending.is_some() && h.app.ly.rx.is_none(),"{source}：線上查詢仍排隊等待舊請求");
        // 本机歌词先到：排队的在线查询取消，不再联网。
        let (native,rx)=std::sync::mpsc::channel();h.app.ly.native_rx=Some(rx);
        native.send(Some(crate::lyrics::parse_lrc("[00:01]本機歌詞"))).unwrap();h.frame(vec![]);
        assert_eq!(h.app.ly.lyrics.as_ref().unwrap().lines[0].text,"本機歌詞");
        assert!(!h.app.ly.loading && h.app.ly.pending.is_none(),"{source}：本機歌詞到達後取消排隊的線上查詢");
        finish(&mut h,release);
    }
}

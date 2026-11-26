//! 換歌與歌曲資訊補全時的歌詞狀態：不串歌、不閃爍、不疊加網路線程。
use super::*;
use crate::smtc::NowPlaying;

fn song(source:&str,title:&str,artist:&str,duration:f64)->NowPlaying{
    // 播放器上報為音樂：歌手暫時為空時仍按歌曲處理（與真實客戶端一致）。
    NowPlaying{source:source.into(),title:title.into(),artist:artist.into(),duration,playing:true,position:7.0,
        playback_type:crate::smtc::MediaKind::Music,..Default::default()}
}

fn lyrics_on(h:&mut H){h.app.opts.fetch_lyrics=true;h.app.settings.island_lyrics=true;h.app.show_lyrics=true;}

/// 用一個阻塞中的假線程佔住網路名額：排隊的查詢不會真的聯網。
fn busy(h:&mut H)->(std::sync::mpsc::Sender<()>,std::thread::ThreadId){
    let (release,wait)=std::sync::mpsc::channel::<()>();
    let worker=std::thread::spawn(move||{let _=wait.recv();});
    let id=worker.thread().id();h.app.ly_worker=Some(worker);(release,id)
}

fn finish(h:&mut H,release:std::sync::mpsc::Sender<()>){
    release.send(()).unwrap();h.app.ly_worker.take().unwrap().join().unwrap();
}

/// 模擬當前歌曲已查到同步歌詞。
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
    assert!(h.app.ly.loading,"新歌應立即顯示查找狀態，不等舊請求結束");
    assert!(h.app.ly.key.starts_with("新歌|"));
    assert!(late.send(Ok((crate::lyrics::parse_lrc("[00:01]遲到的舊歌詞"),false))).is_err(),"上一首遲到的結果必須丟棄");
    assert!(h.app.ly.pending.is_some() && h.app.ly.rx.is_none(),"在線查詢排隊，等舊請求結束");
    assert_eq!(h.app.ly_worker.as_ref().unwrap().thread().id(),id,"舊請求未結束時不得另開取詞線程");
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
    // 歌手從空補全、寫法變化同樣視為同一首。
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
    assert!(!h.app.ly.loading,"已有歌詞時不閃查找狀態");
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
        assert!(h.app.ly.pending.is_some() && h.app.ly.rx.is_none(),"{source}：在線查詢仍排隊等待舊請求");
        // 本機歌詞先到：排隊的在線查詢取消，不再聯網。
        let (native,rx)=std::sync::mpsc::channel();h.app.ly.native_rx=Some(rx);
        native.send(Some(crate::lyrics::parse_lrc("[00:01]本機歌詞"))).unwrap();h.frame(vec![]);
        assert_eq!(h.app.ly.lyrics.as_ref().unwrap().lines[0].text,"本機歌詞");
        assert!(!h.app.ly.loading && h.app.ly.pending.is_none(),"{source}：本機歌詞到達後取消排隊的在線查詢");
        finish(&mut h,release);
    }
}

#[test]
#[cfg(windows)]
fn player_version_without_progress_shows_the_title_instead_of_frozen_lyrics(){
    use crate::smtc::music_compat::{self as compat,Item,Support};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();lyrics_on(&mut h);
    let media=song("kugou","紅玫瑰","陳奕迅",240.0);
    compat::set("KuGou.exe",Item::Progress,Support::Ok);
    showing(&mut h,&media,"[00:01]第一句\n[00:05]第二句");
    assert_eq!(h.app.island_lyric().map(|(_,line)|line),Some("第二句"));
    // 酷狗版本讀不到進度：歌詞無法同步，島上改顯示歌名（暖色），不顯示停住的歌詞。
    compat::set("KuGou.exe",Item::Progress,Support::Unavailable("版本不在適配表裡".into()));
    h.frame(vec![]);
    assert!(h.app.music_lyrics_limited());
    assert_eq!(h.app.island_lyric(),None);
    // 拖動跳轉不可用不影響歌詞。
    compat::set("KuGou.exe",Item::Progress,Support::Learned);
    compat::set("KuGou.exe",Item::Seek,Support::Unavailable("接口不可用".into()));
    h.frame(vec![]);
    assert!(!h.app.music_lyrics_limited());
    assert_eq!(h.app.island_lyric().map(|(_,line)|line),Some("第二句"));
    compat::clear("KuGou.exe",Item::Progress);compat::clear("KuGou.exe",Item::Seek);
    // 其他播放器不受酷狗的結論影響。
    compat::set("KuGou.exe",Item::Progress,Support::Unavailable("x".into()));
    showing(&mut h,&song("QQMusic.exe","舊歌","歌手",200.0),"[00:01]舊歌詞\n[00:05]第二句");
    assert!(!h.app.music_lyrics_limited());
    compat::clear("KuGou.exe",Item::Progress);
}

#[test]
#[cfg(windows)]
fn song_with_system_position_keeps_lyrics_after_player_fallback_fails(){
    use crate::smtc::music_compat::{self as compat,Item,Support};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();lyrics_on(&mut h);
    let (release,_)=busy(&mut h);
    compat::set("cloudmusic.exe",Item::Progress,Support::Unavailable("備用進度讀取失敗".into()));
    showing(&mut h,&song("cloudmusic.exe","缺少進度的歌","歌手",0.0),"[00:01]舊歌詞");
    let without_position=h.app.island_lyric().is_none();
    let mut next=song("cloudmusic.exe","帶系統進度的歌","歌手",220.0);
    next.sampled_at=Some(std::time::Instant::now());
    showing(&mut h,&next,"[00:01]新歌詞");
    let with_position=h.app.island_lyric().map(|(_,line)|line.to_owned());
    compat::clear("cloudmusic.exe",Item::Progress);
    finish(&mut h,release);
    assert!(without_position,"讀不到當前歌曲進度時應顯示歌名");
    assert_eq!(with_position.as_deref(),Some("新歌詞"));
}

#[test]
#[cfg(windows)]
#[ignore="按需匯出：設定音樂頁、收起時讀不到進度的歌名，以及播放器版本較舊時的更新提醒"]
fn music_compat_gallery(){
    use crate::smtc::music_compat::{self as compat,Item,Support};
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_MUSIC_COMPAT_PREVIEW").expect("設定匯出目錄"));
    std::fs::create_dir_all(&dest).unwrap();
    compat::set_version("QQMusic.exe","21.10.0.0");
    compat::set_version("bodian_pc.exe","1.2.0");compat::set("bodian_pc.exe",Item::Reader,Support::Unavailable("播放組件版本不在適配表裡（已適配波點 1.1.7）".into()));
    compat::set_version("cloudmusic.exe","3.1.43.9019");compat::set("cloudmusic.exe",Item::Progress,Support::Learned);
    compat::set_version("KuGou.exe","20.1.61.28014");
    compat::set("KuGou.exe",Item::Progress,Support::Ok);compat::set("KuGou.exe",Item::Duration,Support::Ok);compat::set("KuGou.exe",Item::Seek,Support::Ok);
    // 設定頁：深色、淺色。
    for (name,theme) in [("settings-music-dark",crate::settings_theme::SettingsTheme::Dark),("settings-music-light",crate::settings_theme::SettingsTheme::Light)] {
        let mut h=H::new();h.app.settings.island_lyrics=true;h.app.settings.settings_theme=theme;h.app.settings_tab=1;h.app.open_settings();h.ctx.set_embed_viewports(false);
        let size=egui::vec2(740.0,1180.0);let scale=1.5;
        let mut raster=crate::preview_raster::Raster::new((size.x*scale)as usize,(size.y*scale)as usize);
        for frame in 0..4 {
            let children=std::sync::Arc::new(Mutex::new(Vec::new()));let capture=children.clone();h.t+=0.02;let time=h.t;
            egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
                let id=viewport.ids.this;
                let raw=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,size)),time:Some(time),
                    viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(scale),..Default::default()})).collect(),..Default::default()};
                let out=ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx));capture.lock().unwrap().push((id,out));
            });
            let root=h.ctx.run(egui::RawInput{time:Some(time),..Default::default()},|ctx|h.app.settings_ui(ctx,false));
            raster.update_textures(&root.textures_delta);
            let children=std::mem::take(&mut *children.lock().unwrap());
            for (_,out) in &children{raster.update_textures(&out.textures_delta);}
            let out=&children.iter().find(|(id,_)|*id==egui::ViewportId::from_hash_of("daozai-settings")).unwrap().1;
            if frame==3 {
                raster.buf.fill([0.10,0.10,0.11,1.0]);raster.draw(&h.ctx.tessellate(out.shapes.clone(),out.pixels_per_point),out.pixels_per_point);
                let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                image::save_buffer(dest.join(format!("{name}.png")),&rgba,(size.x*scale)as u32,(size.y*scale)as u32,image::ColorType::Rgba8).unwrap();
            }
            for (_,out) in &children{raster.free_textures(&out.textures_delta);}
        }
    }
    // 島上：正常同步歌詞 與 讀不到進度時的暖色歌名。
    for (name,limited) in [("island-lyrics-normal",false),("island-lyrics-limited",true),("island-update-notice",true),("island-update-notice-plain",true)] {
        let mut h=H::new();lyrics_on(&mut h);h.app.settings.pet=false;
        compat::set("KuGou.exe",Item::Progress,if limited{Support::Unavailable("版本不在適配表裡".into())}else{Support::Ok});
        if name.starts_with("island-update-notice") {compat::set_version("KuGou.exe",if name.ends_with("plain"){"19.8.3.25813"}else{"19.8.3.25812"});compat::outdated("KuGou.exe");}
        if name.ends_with("plain") {h.app.settings.island_lyrics=false;}
        let media=song("kugou","七月七日晴","許慧欣",240.0);
        showing(&mut h,&media,"[00:01]第一句\n[00:05]不能哭泣 不能放棄");
        let ppp=2.0;let (w,hgt)=(crate::app::CANVAS_W,WIN_H);
        let mut raster=feature_raster();
        let frames=if name.starts_with("island-update-notice"){40}else{60};
        for f in 0..frames {
            h.t+=1.0/60.0;
            let env=Env{t:h.t,clock:"09:41:00".into(),today:"2026-11-21".into(),date:"11月21日 週六".into(),media:h.media.clone(),bt_ready:true,..Default::default()};
            let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(w,hgt))),time:Some(h.t),focused:true,
                viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(ppp),..Default::default()})).collect(),..Default::default()};
            let app=&mut h.app;
            let output=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{app.frame(ui,Rect::from_min_size(ui.max_rect().min,Vec2::new(WIN_W,WIN_H)),&env);});});
            raster.update_textures(&output.textures_delta);
            if f+1==frames {
                raster.buf.fill([0.20,0.24,0.32,1.0]);raster.draw(&h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                let image=image::RgbaImage::from_raw((w*ppp) as u32,(hgt*ppp) as u32,rgba).unwrap();
                image::imageops::crop_imm(&image,(WIN_W*ppp/2.0-360.0).max(0.0) as u32,0,720,if name.starts_with("island-update-notice"){170}else{100}).to_image().save(dest.join(format!("{name}.png"))).unwrap();
            }
            raster.free_textures(&output.textures_delta);
        }
    }
    for exe in ["KuGou.exe","cloudmusic.exe","bodian_pc.exe"] {for item in [Item::Progress,Item::Duration,Item::Seek,Item::Reader]{compat::clear(exe,item);}}
}

#[test]
fn artist_filled_in_later_requeries_lyrics_with_the_full_credit(){
    // 波點：換歌后本機歌曲庫約 30 秒才寫入，先只有歌名，隨後補上歌手和歌曲 ID。
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();lyrics_on(&mut h);
    let (release,_)=busy(&mut h);
    let bare=song("bodian_pc.exe","微風細雨","",229.3);
    h.media=bare.clone();h.frame(vec![]);
    let first=h.app.ly.pending.is_some().then(||h.app.ly.song.as_ref().map(|song|(song.title.clone(),song.artist.clone()))).flatten();
    println!("只有歌名時排隊的查詢：{first:?}，key={}",h.app.ly.key);
    let mut full=bare.clone();full.artist="鄧麗君".into();full.song_id=Some(27148558);
    h.media=full;h.frame(vec![]);
    let second=h.app.ly.pending.is_some().then(||h.app.ly.song.as_ref().map(|song|(song.title.clone(),song.artist.clone()))).flatten();
    println!("補上歌手後排隊的查詢：{second:?}，key={}",h.app.ly.key);
    assert_eq!(second,Some(("微風細雨".into(),"鄧麗君".into())),"補上歌手後要按完整署名重新查詢");
    finish(&mut h,release);
}

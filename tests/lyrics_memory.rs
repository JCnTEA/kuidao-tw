//! 連續切歌后保留的堆內存應有界，字體預熱和單幀臨時分配不計入增長。
use std::{alloc::{GlobalAlloc,Layout,System},sync::atomic::{AtomicUsize,Ordering}};
use daozai::app::{App,Env,Options,WIN_W,WIN_H};
use eframe::egui;

struct CountingAllocator;
static LIVE:AtomicUsize=AtomicUsize::new(0);
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self,layout:Layout)->*mut u8 {
        let ptr=System.alloc(layout);if !ptr.is_null(){LIVE.fetch_add(layout.size(),Ordering::Relaxed);}ptr
    }
    unsafe fn dealloc(&self,ptr:*mut u8,layout:Layout){System.dealloc(ptr,layout);LIVE.fetch_sub(layout.size(),Ordering::Relaxed);}
    unsafe fn realloc(&self,ptr:*mut u8,old:Layout,size:usize)->*mut u8 {
        let next=System.realloc(ptr,old,size);
        if !next.is_null(){if size>=old.size(){LIVE.fetch_add(size-old.size(),Ordering::Relaxed);}else{LIVE.fetch_sub(old.size()-size,Ordering::Relaxed);}}
        next
    }
}
#[global_allocator]
static ALLOCATOR:CountingAllocator=CountingAllocator;

#[test]
fn changing_songs_does_not_retain_per_song_animation_state(){
    let root=std::env::temp_dir().join(format!("daozai-lyrics-memory-{}",std::process::id()));
    std::env::set_var("KUIDAO_DATA_DIR",&root);
    for expanded in [false,true] {
    let ctx=egui::Context::default();let mut app=App::new(&ctx,Options{fetch_lyrics:false,pet_idle:1e9});
    app.settings.island_lyrics=true;app.settings.pet=false;
    if expanded{app.show_page(0);}
    let lyrics=daozai::lyrics::parse_lrc("[00:00]切歌后的當前歌詞\n[00:10]下一句歌詞");
    let mut env=Env{clock:"16:00:00".into(),today:"2026-11-04".into(),
        media:daozai::smtc::NowPlaying{source:"cloudmusic.exe".into(),artist:"測試".into(),duration:120.0,position:5.0,playing:true,..Default::default()},..Default::default()};
    fn draw(ctx:&egui::Context,app:&mut App,env:&mut Env){
        env.t+=1.0/60.0;
        let _=ctx.run(egui::RawInput{time:Some(env.t),screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(WIN_W,WIN_H))),..Default::default()},|ctx|{
            egui::CentralPanel::default().show(ctx,|ui|{app.frame(ui,egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(WIN_W,WIN_H)),env);});
        });
    }
    if expanded{env.cursor=Some(egui::pos2(WIN_W/2.0,12.0));}
    let mut baseline=0;
    for song in 0..13_000 {
        env.media.title=format!("測試歌曲 {song}");env.media.song_id=Some(song+1);
        draw(&ctx,&mut app,&mut env);
        app.set_lyrics(&env.media.title,"測試",120.0,lyrics.clone());
        draw(&ctx,&mut app,&mut env);
        if song==999{baseline=LIVE.load(Ordering::Relaxed);}
    }
    let retained=LIVE.load(Ordering::Relaxed).saturating_sub(baseline);
    println!("展開={expanded}，切換額外 12000 首歌曲後，新增保留堆內存：{retained} 字節");
    assert!(retained<128*1024,"歷史歌曲的動畫狀態持續累積：新增 {retained} 字節");
    }
}

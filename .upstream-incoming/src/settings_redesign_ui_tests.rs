use super::*;

/// 旧版「新邮件提醒」关着的用户：升级后邮箱模块关闭，内部开关恢复为开，不会突然开始收信。
#[test]
fn merged_mail_switch_migrates_old_off_to_module_off(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.frame(vec![]);
    h.app.settings.visible_pages[crate::app::MAIL_PAGE]=true;h.app.settings.mail_enabled=false;
    assert!(!h.app.mail_active());
    h.app.migrate_merged_switches();
    assert!(!h.app.settings.visible_pages[crate::app::MAIL_PAGE],"舊的新郵件提醒關閉，遷移為信箱模組關閉");
    assert!(h.app.settings.mail_enabled,"內部開關恢復為開，以後只由模組開關控制");
    assert!(!h.app.mail_active(),"遷移前後都不收信");
    assert!(crate::model::Settings::load().mail_enabled,"遷移結果要儲存");
    h.app.set_page_visible(crate::app::MAIL_PAGE,true);
    assert!(h.app.mail_active(),"重新開啟模組即恢復收信，不再需要第二個開關");
}

#[test]
fn merged_quota_switch_migrates_old_off_to_module_off(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.frame(vec![]);
    h.app.settings.visible_pages[crate::app::QUOTA_PAGE]=true;h.app.quota.config.enabled=false;
    h.app.migrate_merged_switches();
    assert!(!h.app.settings.visible_pages[crate::app::QUOTA_PAGE]);
    assert!(h.app.quota.config.enabled);
    assert!(!h.app.quota_active());
}

/// 已经开着的用户不受影响。
#[test]
fn merged_switches_keep_enabled_users_unchanged(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.frame(vec![]);
    let pages=h.app.settings.visible_pages;
    h.app.settings.mail_enabled=true;h.app.quota.config.enabled=true;
    h.app.migrate_merged_switches();
    assert_eq!(h.app.settings.visible_pages,pages);
    assert!(h.app.settings.mail_enabled && h.app.quota.config.enabled);
}

/// 只剩邮箱一个页面时模块关不掉：保持原来的“页面在、不收信”，不改变行为。
#[test]
fn merged_mail_switch_keeps_last_page_behavior(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();h.frame(vec![]);
    h.app.settings.visible_pages=[false;11];h.app.settings.visible_pages[crate::app::MAIL_PAGE]=true;
    h.app.settings.mail_enabled=false;
    h.app.migrate_merged_switches();
    assert!(h.app.settings.visible_pages[crate::app::MAIL_PAGE]);
    assert!(!h.app.settings.mail_enabled,"關不掉模組時保留舊的內部開關");
    assert!(!h.app.mail_active());
}

/// 旧的分页序号（岛上入口、更新提示等按序号跳转）仍能打开合并后的页面，每个序号只属于一个侧栏项。
#[test]
fn settings_old_tab_numbers_open_merged_pages(){
    use crate::extras::settings_tab_group_for_tests as group;
    assert_eq!(group(2),"專注與健康");assert_eq!(group(3),"專注與健康");
    assert_eq!(group(4),"待辦與置物");assert_eq!(group(5),"待辦與置物");
    assert_eq!(group(6),"訊息提醒");assert_eq!(group(7),"信箱");assert_eq!(group(8),"行程");
    assert_eq!(group(0),"通用");assert_eq!(group(13),"外觀");assert_eq!(group(14),"位置");
    assert_eq!(group(10),"頁面管理");assert_eq!(group(9),"關於");
}

/// 每个分页都能正常绘制（含功能关闭时的提示），不会因为挪动布局而崩溃。
#[test]
fn settings_every_tab_renders_with_modules_on_and_off(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for on in [true,false] {
        let mut h=H::new();h.app.open_settings();h.ctx.set_embed_viewports(true);
        if !on {for page in 0..9 {if page!=crate::app::MUSIC_PAGE {h.app.settings.visible_pages[page]=false;}}}
        if on {h.app.set_page_visible(crate::app::PERFORMANCE_PAGE,true);}
        for tab in 0..=16 {
            h.app.settings_tab=tab;
            for _ in 0..2 {let _=h.ctx.run(egui::RawInput::default(),|ctx|h.app.settings_ui(ctx,false));}
            assert_eq!(h.app.settings_tab,tab,"繪製分頁 {tab} 不應自行跳轉");
        }
    }
}

/// 设置窗口拉到最小（600×450）时，侧栏 13 项都在可见区内，「关于」不会被挤出去。
#[test]
fn settings_sidebar_keeps_about_visible_at_minimum_height(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();h.app.open_settings();h.ctx.set_embed_viewports(false);
    let output=std::sync::Arc::new(Mutex::new(None));let capture=output.clone();
    egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
        let id=viewport.ids.this;
        let raw=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(600.0,450.0))),
            viewports:std::iter::once((id,egui::ViewportInfo::default())).collect(),..Default::default()};
        *capture.lock().unwrap()=Some(ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx)));
    });
    for _ in 0..3 {let _=h.ctx.run(Default::default(),|ctx|h.app.settings_ui(ctx,false));}
    let frame=output.lock().unwrap().take().unwrap();
    for label in ["通用","天氣","關於"] {
        let rect=frame.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t) if t.galley.text()==label && t.pos.x<180.0=>Some(t.visual_bounding_rect()),_=>None})
            .unwrap_or_else(||panic!("側欄缺少 {label}"));
        assert!(rect.bottom()<=450.0,"{label} 超出視窗底部：{rect:?}");
    }
}

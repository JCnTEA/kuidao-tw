//! 和風設定的原生交互與截圖；只使用佔位憑據和合成天氣。
use super::*;
use crate::{weather_source_ui::Provider,settings_theme::SettingsTheme};

fn fixture(theme:SettingsTheme)->H {
    let mut h=H::new();h.app.settings.settings_theme=theme;h.app.settings_tab=12;h.app.open_settings();h.ctx.set_embed_viewports(false);
    h.app.weather.snapshot=Some(crate::weather::Snapshot{place:"杭州".into(),temp:19.0,code:2,day:true,feels:Some(18.0),humidity:Some(58),wind:Some(12.0),high:Some(23.0),low:Some(15.0),at:chrono::Utc::now().timestamp(),..Default::default()});
    h
}

#[test]
fn weather_badge_only_dims_after_repeated_failures_and_old_data(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for (provider,interval) in [(Provider::Qweather,crate::qweather::CURRENT_INTERVAL),(Provider::OpenMeteo,crate::weather::INTERVAL)] {
        let mut h=H::new();h.app.weather_source.displayed=provider;
        for (age,failures,gray) in [(interval+900,0,false),(interval*3,0,false),(interval*3,1,false),(interval,2,false),(interval*3,2,true),(interval*3,0,false)] {
            if provider==Provider::Qweather&&failures>0 {continue;}
            h.app.weather.failures=failures;
            let snapshot=crate::weather::Snapshot{place:"廣州".into(),temp:23.0,code:2,day:true,at:chrono::Utc::now().timestamp()-age,..Default::default()};
            let output=h.ctx.run(Default::default(),|ctx|{egui::CentralPanel::default().show(ctx,|ui|{
                let c=crate::paint::Card::new(ui,Pos2::ZERO,ui.max_rect());
                let layout=crate::bar_layout::Layout::new(false,0,false,false,false,false,true);
                h.app.paint_pill_weather(&c,100.0,&layout,&snapshot);
                h.app.paint_side_weather(&c,220.0,30.0,&snapshot);
            });});
            let colors:Vec<_>=output.shapes.iter().filter_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()=="23°"=>Some(t.fallback_color),_=>None}).collect();
            assert_eq!(colors.len(),2,"橫向和側邊都應顯示溫度");
            assert!(colors.iter().all(|c|*c==if gray{crate::theme::GRAY}else{crate::theme::WHITE}),"等待重新整理或單次失敗時應保持白色，只有連續失敗且數據明顯過期才變灰：age={age}, failures={failures}");
        }
    }
}

// 通過真實的和風解析與緩存入口準備示例，避免逐幀輪詢覆蓋手工注入的天氣。
fn qweather_data_fixture(h:&mut H){
    let location=crate::qweather::Location{position:crate::weather::Position{lat:30.27,lon:120.15,label:"杭州".into(),region:"浙江省 · 杭州市".into()},timezone:"Asia/Shanghai".into()};
    let today=chrono::Utc::now().with_timezone(&chrono_tz::Asia::Shanghai).date_naive();let now=chrono::Utc::now().timestamp();
    let days:Vec<_>=(0..7).map(|i|{let date=today+chrono::Duration::days(i);serde_json::json!({"forecastStartTime":format!("{date}T00:00+08:00"),"astro":{"sunrise":format!("{date}T06:23+08:00"),"sunset":format!("{date}T17:05+08:00")},"temperatureMax":{"value":23-i,"unit":"°C"},"temperatureMin":{"value":15-i/2,"unit":"°C"},"daytime":{"condition":{"code":"101"},"precipitation":{"probability":0.2}},"nighttime":{"precipitation":{"probability":0.3}}})}).collect();
    let credit=serde_json::json!({"attributions":["https://developer.qweather.com/attribution.html"]});
    let cache=serde_json::json!({"host":"example.re.qweatherapi.com","location":location,"current_at":now,"daily_at":now,"current":{"validTime":chrono::Utc::now().to_rfc3339(),"temperature":{"value":19,"unit":"°C"},"feelsLike":{"value":18,"unit":"°C"},"condition":{"code":"101","text":"多雲"},"humidity":0.58,"wind":{"speed":{"value":3.333333,"unit":"m/s"}},"metadata":credit},"daily":{"days":days,"metadata":credit}});
    let dir=crate::model::config_dir();std::fs::create_dir_all(&dir).unwrap();std::fs::write(dir.join("qweather-cache.json"),serde_json::to_vec(&cache).unwrap()).unwrap();
    let source=&mut h.app.weather_source;source.displayed=Provider::Qweather;source.selected=Provider::Qweather;source.config.host="example.re.qweatherapi.com".into();source.config.location=Some(location.clone());source.config.saved=vec![location.clone()];source.host=source.config.host.clone();source.picked=Some(location.clone());
    source.client=Some(crate::qweather::Client::new(&source.config.host,"fake-test-key",std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true))).unwrap());source.runtime.reset(&source.config.host,Some(location));h.app.weather.snapshot=source.runtime.snapshot();
}

fn draw(h:&mut H,events:Vec<egui::Event>,size:Vec2,scale:f32)->(egui::FullOutput,Vec<(egui::ViewportId,egui::FullOutput)>){
    let children=std::sync::Arc::new(Mutex::new(Vec::new()));let capture=children.clone();h.t+=0.02;let time=h.t;let tutorial_open=h.app.weather_source.tutorial_open;
    egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
        let id=viewport.ids.this;
        let tutorial=id==egui::ViewportId::from_hash_of("daozai-weather-tutorial");
        let raw=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,if tutorial{egui::vec2(820.0,680.0)}else{size})),time:Some(time),
            events:if tutorial==tutorial_open{events.clone()}else{vec![]},viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(scale),..Default::default()})).collect(),..Default::default()};
        let out=ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx));capture.lock().unwrap().push((id,out));
    });
    let root=h.ctx.run(egui::RawInput{time:Some(time),..Default::default()},|ctx|h.app.settings_ui(ctx,false));
    let child_frames=std::mem::take(&mut *children.lock().unwrap());
    h.shapes=child_frames.iter().find(|(id,_)|*id==egui::ViewportId::from_hash_of("daozai-settings")).unwrap().1.shapes.clone();
    (root,child_frames)
}

fn click(h:&mut H,label:&str){
    let point=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()==label=>Some(t.visual_bounding_rect().center()),_=>None}).unwrap_or_else(||panic!("未找到控件：{label}"));
    for pressed in [true,false]{draw(h,vec![egui::Event::PointerMoved(point),egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}],egui::vec2(740.0,1100.0),1.0);}
}

#[ignore = "和風天氣 UI 已於 0.7.3 移除"]

#[test]
fn qweather_verified_connection_is_prominent_and_cities_have_dropdown_controls(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=fixture(SettingsTheme::Dark);h.app.weather_source.selected=Provider::Qweather;h.app.weather_source.mock_verified();
    for _ in 0..3{draw(&mut h,vec![],egui::vec2(740.0,1100.0),1.0);}
    let texts:Vec<_>=h.shapes.iter().filter_map(|s|if let egui::Shape::Text(t)=&s.shape{Some(t.galley.text())}else{None}).collect();
    assert!(texts.contains(&"連線測試通過"),"成功狀態應明確顯示在配置區");
    assert!(texts.contains(&"不限省份"),"省份應有可直接選擇的下拉列表");
    assert!(texts.contains(&"新增城市"),"儲存城市應有獨立入口");
}

fn district(name:&str,index:usize)->crate::qweather::Location {
    crate::qweather::Location{position:crate::weather::Position{lat:30.42+index as f64*0.01,lon:119.99,label:name.into(),region:"浙江省 · 杭州市".into()},timezone:"Asia/Shanghai".into()}
}

#[ignore = "和風天氣 UI 已於 0.7.3 移除"]

#[test]
fn qweather_five_saved_cities_are_independent_selectable_and_removable(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=fixture(SettingsTheme::Dark);
    h.app.weather_source.selected=Provider::Qweather;h.app.weather_source.mock_verified();
    h.app.settings.weather_saved=vec![district("預設來源城市",0).position];let original=h.app.settings.weather_saved.clone();
    let locations:Vec<_>=["餘杭","臨平","富陽","臨安","桐廬"].iter().enumerate().map(|(i,n)|district(n,i)).collect();
    for location in &locations{h.app.weather_source.mock_choose_city(location.clone());h.app.qweather_add_city();}
    assert_eq!(h.app.weather_source.config.saved.len(),5);assert!(h.app.weather_source.displayed==Provider::OpenMeteo,"新增城市不能自動切換來源");
    h.app.weather_source.mock_choose_city(locations[0].clone());h.app.qweather_add_city();assert_eq!(h.app.weather_source.config.saved.len(),5,"同一城市不能重複儲存");
    h.app.weather_source.mock_choose_city(district("建德",6));h.app.qweather_add_city();assert_eq!(h.app.weather_source.config.saved.len(),5,"第六個城市不能繞過上限");
    assert_eq!(h.app.settings.weather_saved,original);assert_eq!(crate::qweather::Config::load().saved.len(),5);
    h.app.weather_source.mock_choose_city(locations[0].clone());
    for _ in 0..3{draw(&mut h,vec![],egui::vec2(740.0,1100.0),1.0);}click(&mut h,"儲存並啟用");assert!(h.app.weather_source.displayed==Provider::Qweather);
    h.app.qweather_select_location(locations[3].clone());assert!(h.app.weather_source.config.location.as_ref()==Some(&locations[3]));
    h.app.qweather_remove_city(&locations[1]);assert!(h.app.weather_source.config.location.as_ref()==Some(&locations[3]));
    h.app.qweather_remove_city(&locations[3]);assert!(h.app.weather_source.config.location.as_ref()==Some(&locations[0]),"刪除當前城市後應切換到剩餘城市");
    for location in h.app.weather_source.config.saved.clone(){h.app.qweather_remove_city(&location);}
    assert!(h.app.weather_source.config.saved.is_empty()&&h.app.weather_source.config.location.is_none());h.app.weather_tick();assert!(h.app.weather.snapshot.is_none());assert!(h.app.weather.error.as_ref().unwrap().contains("先選擇"));
    h.app.weather_source.mock_choose_city(locations[0].clone());h.app.qweather_add_city();assert!(h.app.weather_source.config.location.is_none()&&h.app.weather_source.picked.is_none(),"新增城市不能冒充已經切換到該城市");
    h.app.qweather_select_location(locations[0].clone());assert!(h.app.weather_source.config.location.as_ref()==Some(&locations[0]));
    assert_eq!(h.app.settings.weather_saved,original);
}

#[ignore = "和風天氣 UI 已於 0.7.3 移除"]

#[test]
fn qweather_city_dropdown_supports_fuzzy_input_keyboard_selection_and_add(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=fixture(SettingsTheme::Dark);
    h.app.weather_source.selected=Provider::Qweather;h.app.weather_source.mock_verified();
    for _ in 0..3{draw(&mut h,vec![],egui::vec2(740.0,1100.0),1.0);}
    click(&mut h,"不限省份");for _ in 0..3{draw(&mut h,vec![],egui::vec2(740.0,1100.0),1.0);}
    let point=h.shapes.iter().find_map(|s|if let egui::Shape::Text(t)=&s.shape{(t.galley.text()=="北京").then(||t.visual_bounding_rect().center())}else{None}).expect("省份下拉應顯示候選");
    for frame in 0..25{let mut events=vec![egui::Event::PointerMoved(point)];if frame==0{events.push(egui::Event::MouseWheel{unit:egui::MouseWheelUnit::Point,delta:egui::vec2(0.0,-120.0),modifiers:Default::default()});}draw(&mut h,events,egui::vec2(740.0,1100.0),1.0);}
    click(&mut h,"浙江");assert_eq!(h.app.weather_source.province,"浙江");
    click(&mut h,"輸入城市、縣或區名稱");
    draw(&mut h,vec![egui::Event::Text("杭".into())],egui::vec2(740.0,1100.0),1.0);
    for _ in 0..26{draw(&mut h,vec![],egui::vec2(740.0,1100.0),1.0);}
    let candidates=vec![district("杭州",0),district("餘杭",1),district("臨平",2)];h.app.weather_source.mock_search_results(candidates.clone());
    for _ in 0..2{draw(&mut h,vec![],egui::vec2(740.0,1100.0),1.0);}
    assert!(h.ctx.memory(|m|m.is_popup_open(egui::Id::new("qweather-city-options"))));
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="杭州")),"模糊輸入應顯示匹配候選");
    let key=|key|egui::Event::Key{key,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()};
    draw(&mut h,vec![egui::Event::Ime(egui::ImeEvent::Enabled),key(egui::Key::ArrowDown)],egui::vec2(740.0,1100.0),1.0);
    draw(&mut h,vec![egui::Event::Ime(egui::ImeEvent::Disabled),key(egui::Key::Enter)],egui::vec2(740.0,1100.0),1.0);
    assert_eq!(h.app.weather_source.query,"杭","輸入法選詞不能選取城市");
    draw(&mut h,vec![key(egui::Key::ArrowDown)],egui::vec2(740.0,1100.0),1.0);
    draw(&mut h,vec![key(egui::Key::Enter)],egui::vec2(740.0,1100.0),1.0);
    assert_eq!(h.app.weather_source.query,"餘杭");
    draw(&mut h,vec![],egui::vec2(740.0,1100.0),1.0);click(&mut h,"新增城市");assert!(h.app.weather_source.config.saved.contains(&candidates[1]));
    assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="連線測試通過")),"搜尋和新增不能覆蓋連線成功狀態");
}

#[ignore = "和風天氣 UI 已於 0.7.3 移除"]

#[test]
fn qweather_draft_hides_default_cities_requires_validation_and_can_collapse_without_hiding_cities(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for theme in [SettingsTheme::Dark,SettingsTheme::Light]{
        let mut h=fixture(theme);let settings=format!("{:?}",h.app.settings);
        for _ in 0..3{draw(&mut h,vec![],egui::vec2(740.0,1100.0),1.0);}
        click(&mut h,"和風天氣");
        assert!(h.app.weather_source.selected==Provider::Qweather&&h.app.weather_source.displayed==Provider::OpenMeteo);
        let text=|h:&H|h.shapes.iter().filter_map(|s|if let egui::Shape::Text(t)=&s.shape{Some(t.galley.text().to_string())}else{None}).collect::<Vec<_>>().join("\n");
        assert!(!text(&h).contains("19°")&&!text(&h).contains("杭州"),"和風草稿不應混入預設源數據");
        assert!(text(&h).contains("先填寫 Host / Key"));
        h.app.weather_source.host="https://not-official.invalid".into();h.app.weather_source.key="frontend-placeholder".into();
        click(&mut h,"測試連線");assert!(h.app.weather_source.message.unwrap().1);
        click(&mut h,"儲存並啟用");assert!(h.app.weather_source.displayed==Provider::OpenMeteo);
        click(&mut h,"免費額度詳情");assert!(text(&h).contains("50,000"));assert!(!text(&h).contains("840")&&!text(&h).contains("預估"));
        click(&mut h,"收起");assert!(!h.app.weather_source.config_open);draw(&mut h,vec![],egui::vec2(740.0,1100.0),1.0);
        assert!(!text(&h).lines().any(|line|line=="API Host"));assert!(text(&h).contains("和風城市")&&text(&h).contains("儲存並啟用"));
        click(&mut h,"展開");assert!(h.app.weather_source.config_open);
        assert!(!text(&h).contains("frontend-placeholder"));assert_eq!(format!("{:?}",h.app.settings),settings);
        h.app.settings.save();let saved=std::fs::read_to_string(crate::model::config_dir().join("settings.txt")).unwrap();assert!(!saved.contains("frontend-placeholder"));
        click(&mut h,"配置教程 ↗");assert!(h.app.weather_source.tutorial_open);
        let (_,children)=draw(&mut h,vec![],egui::vec2(740.0,1100.0),1.0);assert!(children.iter().any(|(id,_)|*id==egui::ViewportId::from_hash_of("daozai-weather-tutorial")));
        let second_tiles:Vec<_>=h.ctx.tex_manager().read().allocated().filter(|(_,meta)|meta.name.starts_with("weather-tutorial-1-")).map(|(id,_)|*id).collect();assert!(!second_tiles.is_empty());
        let mut second_visible=false;
        for frame in 0..45 {
            let mut events=vec![egui::Event::PointerMoved(Pos2::new(400.0,400.0))];if frame==2||frame==24{events.push(egui::Event::MouseWheel{unit:egui::MouseWheelUnit::Point,delta:egui::vec2(0.0,if frame==2{-2000.0}else{-10000.0}),modifiers:Default::default()});}
            let (_,children)=draw(&mut h,events,egui::vec2(740.0,1100.0),1.0);let help=&children.iter().find(|(id,_)|*id==egui::ViewportId::from_hash_of("daozai-weather-tutorial")).unwrap().1;
            second_visible|=help.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Rect(r)if second_tiles.contains(&r.fill_texture_id())&&s.clip_rect.intersects(r.rect)));
        }
        assert!(second_visible,"長圖應能連續滾到新教程後半段");
        h.app.weather_source.close();assert!(h.app.weather_source.key.is_empty());assert!(!h.app.weather_source.tutorial_open);
    }
}

#[ignore = "和風天氣 UI 已於 0.7.3 移除"]

#[test]
fn qweather_apply_switches_only_after_validation_and_module_switch_controls_source_and_resident(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=fixture(SettingsTheme::Dark);
    h.app.weather_source.selected=Provider::Qweather;h.app.weather_source.config_open=false;
    h.app.weather_source.mock_verified();
    for _ in 0..3{draw(&mut h,vec![],egui::vec2(740.0,1100.0),1.0);}
    click(&mut h,"儲存並啟用");assert!(h.app.weather_source.displayed==Provider::Qweather);assert!(h.app.weather.snapshot.is_none(),"切換後立即清除預設源天氣");
    assert!(h.app.settings.weather_saved.is_empty(),"和風城市不能寫進預設源列表");assert_eq!(h.app.weather_source.config.saved[0].position.label,"餘杭");
    h.app.weather.snapshot=Some(crate::weather::Snapshot{place:"餘杭".into(),..Default::default()});h.app.settings.show_weather=true;
    assert!(h.app.weather_pill().is_some());h.app.settings.show_weather=false;assert!(h.app.weather_pill().is_none());h.app.settings.show_weather=true;
    h.app.settings.visible_pages[8]=false;h.app.weather_tick();assert!(h.app.weather_pill().is_none());assert!(!h.app.weather_source.client.as_ref().unwrap().gate.load(std::sync::atomic::Ordering::SeqCst));
    h.app.settings.visible_pages[8]=true;h.app.weather_tick();assert!(h.app.weather_source.client.as_ref().unwrap().gate.load(std::sync::atomic::Ordering::SeqCst));
    h.app.weather_source.selected=Provider::OpenMeteo;for _ in 0..3{draw(&mut h,vec![],egui::vec2(740.0,1100.0),1.0);}
    click(&mut h,"應用 Open-Meteo");assert!(h.app.weather_source.displayed==Provider::OpenMeteo);
}

#[ignore = "和風天氣 UI 已於 0.7.3 移除"]

#[test]
fn qweather_configuration_site_opens_developer_console(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for theme in [SettingsTheme::Dark,SettingsTheme::Light]{
        let mut h=fixture(theme);h.app.weather_source.selected=Provider::Qweather;
        for _ in 0..3{draw(&mut h,vec![],egui::vec2(740.0,1100.0),1.0);}
        let website=h.shapes.iter().find_map(|s|match &s.shape{egui::Shape::Text(t)if t.galley.text()=="配置網站 ↗"=>Some(t.visual_bounding_rect().center()),_=>None}).expect("配置標題旁應有控制檯入口");
        let mut opened=None;
        for pressed in [true,false]{
            let (root,children)=draw(&mut h,vec![egui::Event::PointerMoved(website),egui::Event::PointerButton{pos:website,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}],egui::vec2(740.0,1100.0),1.0);
            for out in std::iter::once(root).chain(children.into_iter().map(|(_,out)|out)){
                opened=out.platform_output.commands.into_iter().find_map(|command|if let egui::OutputCommand::OpenUrl(url)=command{Some(url.url)}else{None}).or(opened);
            }
        }
        assert_eq!(opened,Some("https://console.qweather.com/".into()),"配置網站應打開開發者控制檯");
    }
}

#[ignore = "和風天氣 UI 已於 0.7.3 移除"]

#[test]
fn qweather_source_card_shows_account_free_quota_separately_from_local_usage(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=fixture(SettingsTheme::Dark);qweather_data_fixture(&mut h);h.app.weather_source.config_open=false;
    for _ in 0..3{draw(&mut h,vec![],egui::vec2(740.0,1200.0),1.0);}
    let text=h.shapes.iter().filter_map(|s|if let egui::Shape::Text(t)=&s.shape{Some(t.galley.text())}else{None}).collect::<Vec<_>>().join("\n");
    assert!(text.contains("本機本月已請求")&&text.contains("免費額度 50,000 次"));assert!(!text.contains("5,000"),"來源卡不能把本機保護值寫成帳號免費額度");
}

#[ignore = "和風天氣 UI 已於 0.7.3 移除"]

#[test]
fn qweather_data_credit_opens_the_provider_site_in_forecast_and_solar_views(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for solar in [false,true]{
        let mut h=H::new();h.app.settings.pet=false;qweather_data_fixture(&mut h);
        h.app.show_page(8);h.app.weather.solar_open=solar;h.hover(H::local(300.0,60.0),1.0);
        assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text()=="QWeather")));
        assert!(h.app.weather.snapshot.as_ref().is_some_and(|s|s.days.len()==7),"檢查署名時必須實際顯示和風數據");
        assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.text().starts_with("預報 "))),"預報需標明自己的更新時間");
        let point=h.ctx.read_response(egui::Id::new(("card",true,"weather-source"))).unwrap().rect.center();
        h.click(point);assert!(h.cmds.contains(&crate::app::Cmd::OpenUrl("https://www.qweather.com/".into())));assert_eq!(h.app.page(),8);
    }
}

#[test]
#[ignore="按需匯出和風配置、離線教程和署名的原生界面"]
fn qweather_frontend_gallery(){
    let _g=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("DAOZAI_QWEATHER_PREVIEW_DIR").expect("需指定預覽目錄"));std::fs::create_dir_all(&dest).unwrap();
    for (name,dark,size) in [("weather-source-cards-dark",true,egui::vec2(500.0,86.0)),("weather-source-cards-light",false,egui::vec2(500.0,86.0)),("weather-source-cards-compact",true,egui::vec2(350.0,86.0))]{
        let mut h=H::new();h.ctx.set_visuals(if dark{egui::Visuals::dark()}else{egui::Visuals::light()});h.ctx.set_pixels_per_point(2.0);
        let mut raster=crate::preview_raster::Raster::new((size.x*2.0)as usize,(size.y*2.0)as usize);
        for _ in 0..4 {
            h.t+=0.02;
            let out=h.ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,size)),time:Some(h.t),..Default::default()},|ctx|{
                egui::CentralPanel::default().frame(egui::Frame::new().fill(if dark{egui::Color32::from_rgb(25,25,28)}else{egui::Color32::from_rgb(245,246,248)}).inner_margin(10)).show(ctx,|ui|{
                    ui.visuals_mut().override_text_color=Some(if dark{egui::Color32::WHITE}else{egui::Color32::from_rgb(30,34,41)});
                    let width=(ui.available_width()-ui.spacing().item_spacing.x)/2.0;
                    ui.horizontal(|ui|{for provider in [Provider::OpenMeteo,Provider::Qweather]{crate::weather_source_ui::provider_option(ui,provider,provider==Provider::Qweather,width);}});
                });
            });
            raster.update_textures(&out.textures_delta);raster.buf.fill([0.0,0.0,0.0,1.0]);raster.draw(&h.ctx.tessellate(out.shapes,out.pixels_per_point),out.pixels_per_point);raster.free_textures(&out.textures_delta);
        }
        let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);image::save_buffer(dest.join(format!("{name}.png")),&rgba,(size.x*2.0)as u32,(size.y*2.0)as u32,image::ColorType::Rgba8).unwrap();
    }
    for (name,theme,size,help,end) in [
        ("qweather-dark",SettingsTheme::Dark,egui::vec2(740.0,840.0),false,false),
        ("qweather-light",SettingsTheme::Light,egui::vec2(740.0,840.0),false,false),
        ("qweather-collapsed",SettingsTheme::Dark,egui::vec2(690.0,700.0),false,false),
        ("qweather-ready-dark",SettingsTheme::Dark,egui::vec2(740.0,850.0),false,false),
        ("qweather-ready-light",SettingsTheme::Light,egui::vec2(740.0,850.0),false,false),
        ("qweather-city-picker-dark",SettingsTheme::Dark,egui::vec2(740.0,950.0),false,false),
        ("qweather-city-picker-light",SettingsTheme::Light,egui::vec2(740.0,950.0),false,false),
        ("qweather-five-cities",SettingsTheme::Dark,egui::vec2(740.0,1250.0),false,false),
        ("qweather-small",SettingsTheme::Dark,egui::vec2(600.0,450.0),false,false),
        ("qweather-default",SettingsTheme::Dark,egui::vec2(690.0,560.0),false,false),
        ("qweather-tutorial-long",SettingsTheme::Dark,egui::vec2(740.0,1100.0),true,false),
        ("qweather-tutorial-long-end",SettingsTheme::Light,egui::vec2(740.0,1100.0),true,true),
    ]{
        let mut h=fixture(theme);h.app.weather_source.selected=Provider::Qweather;
        h.app.weather_source.host="example.xy.qweatherapi.com".into();h.app.weather_source.key="frontend-placeholder".into();
        h.app.weather_source.tutorial_open=help;h.app.weather_source.config_open=name!="qweather-collapsed";
        if name.contains("ready")||name.contains("city-picker"){
            h.app.weather_source.mock_verified();h.app.weather_source.config_open=false;
            h.app.weather_source.config.saved=vec![district("富陽",3),district("臨安",4)];h.app.weather_source.picked=Some(district("富陽",3));
        }
        if name.contains("city-picker"){
            h.app.weather_source.province="浙江".into();h.app.weather_source.query="杭".into();h.app.weather_source.mock_search_results(vec![district("杭州",0),district("餘杭",1),district("臨平",2)]);
            h.ctx.memory_mut(|m|m.open_popup(egui::Id::new("qweather-city-options")));
        }
        if name=="qweather-five-cities" {
            qweather_data_fixture(&mut h);h.app.weather_source.key.clear();h.app.weather_source.config_open=false;
            let current=h.app.weather_source.config.location.clone().unwrap();h.app.weather_source.config.saved=vec![current,district("餘杭",1),district("臨平",2),district("富陽",3),district("臨安",4)];
        }
        let canvas=if help{egui::vec2(820.0,680.0)}else{size};let scale=1.5;
        let mut raster=crate::preview_raster::Raster::new((canvas.x*scale)as usize,(canvas.y*scale)as usize);
        let frames=if end{45}else{4};
        for frame in 0..frames{
            let mut events=if end{vec![egui::Event::PointerMoved(Pos2::new(400.0,400.0))]}else{vec![]};
            if end&&frame==2{events.push(egui::Event::MouseWheel{unit:egui::MouseWheelUnit::Point,delta:egui::vec2(0.0,-10000.0),modifiers:Default::default()});}
            let (root,children)=draw(&mut h,events,size,scale);raster.update_textures(&root.textures_delta);
            for (_,out) in &children{raster.update_textures(&out.textures_delta);}
            let id=egui::ViewportId::from_hash_of(if help{"daozai-weather-tutorial"}else{"daozai-settings"});
            let out=&children.iter().find(|(child,_)|*child==id).unwrap().1;
            if frame==frames-1{
                raster.buf.fill([0.10,0.10,0.11,1.0]);raster.draw(&h.ctx.tessellate(out.shapes.clone(),out.pixels_per_point),out.pixels_per_point);
                let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);image::save_buffer(dest.join(format!("{name}.png")),&rgba,(canvas.x*scale)as u32,(canvas.y*scale)as u32,image::ColorType::Rgba8).unwrap();
                for shape in &out.shapes{if let egui::Shape::Text(text)=&shape.shape {
                    let rect=text.visual_bounding_rect();if shape.clip_rect.contains(rect.center()) {assert!(rect.left()>=-1.0&&rect.right()<=canvas.x+1.0,"窗口內文字不能橫向溢出：{}",text.galley.text());}
                }}
            }
            for (_,out) in &children{raster.free_textures(&out.textures_delta);}
        }
    }
    let mut h=H::new();h.app.settings.pet=false;qweather_data_fixture(&mut h);
    h.app.show_page(8);let mut raster=feature_raster();
    shoot_feature(&mut h,&mut raster,&dest,"qweather-attribution",90,Some(H::local(180.0,185.0)),&chrono::Local::now().format("%Y-%m-%d").to_string());
}

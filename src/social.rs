//! 社群訊息來源：從 Windows 通知辨識 Threads／Messenger／Facebook／LINE／Instagram，
//! 只記住「有幾則」與應用程式開關，不保存訊息內容。
use crate::paint::Card;
use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Social { Threads, Messenger, Facebook, Line, Instagram }

impl Social {
    pub const ALL:[Self;5]=[Self::Threads,Self::Messenger,Self::Facebook,Self::Line,Self::Instagram];
    pub fn key(self)->&'static str{match self{Self::Threads=>"threads",Self::Messenger=>"messenger",Self::Facebook=>"facebook",Self::Line=>"line",Self::Instagram=>"instagram"}}
    pub fn name(self)->&'static str{match self{Self::Threads=>"Threads",Self::Messenger=>"Messenger",Self::Facebook=>"Facebook",Self::Line=>"LINE",Self::Instagram=>"Instagram"}}
    pub fn url(self)->&'static str{match self{
        Self::Threads=>"https://www.threads.net/",
        Self::Messenger=>"https://www.messenger.com/",
        Self::Facebook=>"https://www.facebook.com/",
        Self::Line=>"https://line.me/",
        Self::Instagram=>"https://www.instagram.com/",
    }}
    /// 以 (x,y) 為中心畫 20×20 的圓角標誌。
    pub fn paint(self,c:&Card,x:f32,y:f32){
        let rect=c.rect(x-10.0,y-10.0,20.0,20.0);
        let icon=match self {
            Self::Threads=>egui::Image::new(egui::include_image!("../assets/social/threads.png")),
            Self::Messenger=>egui::Image::new(egui::include_image!("../assets/social/messenger.png")),
            Self::Facebook=>egui::Image::new(egui::include_image!("../assets/social/facebook.png")),
            Self::Line=>egui::Image::new(egui::include_image!("../assets/social/line.png")),
            Self::Instagram=>egui::Image::new(egui::include_image!("../assets/social/instagram.png")),
        };
        if let Ok(egui::load::TexturePoll::Ready{texture})=icon.load_for_size(c.ui.ctx(),rect.size()){
            c.painter.image(texture.id,rect,egui::Rect::from_min_max(egui::Pos2::ZERO,egui::pos2(1.0,1.0)),egui::Color32::WHITE);
        }
    }
    fn index(self)->usize{match self{Self::Threads=>0,Self::Messenger=>1,Self::Facebook=>2,Self::Line=>3,Self::Instagram=>4}}
}

/// 由通知的應用程式名稱與 AUMID 判斷來源；Messenger 先於 Facebook，避免誤判。
pub fn detect(app:&str,id:&str)->Option<Social>{
    let text=format!("{} {}",app,id).to_lowercase();
    let app=app.trim().to_lowercase();
    let id=id.to_lowercase();
    if text.contains("messenger"){return Some(Social::Messenger);}
    if text.contains("threads"){return Some(Social::Threads);}
    if text.contains("instagram")||text.contains("insta gram"){return Some(Social::Instagram);}
    if text.contains("naver.line")||app=="line"||id.ends_with(".line")||id.contains("line.messenger"){return Some(Social::Line);}
    if text.contains("facebook"){return Some(Social::Facebook);}
    None
}

/// 每個來源的未讀則數；點開應用程式或到通知中心查看後歸零。
#[derive(Clone,Debug,Default)]
pub struct Badges{counts:[u16;5]}
impl Badges{
    pub fn load()->Self{
        let path=crate::model::config_dir().join("social-badges.json");
        let Ok(raw)=std::fs::read(path) else{return Self::default()};
        let Ok(list)=serde_json::from_slice::<Vec<serde_json::Value>>(&raw) else{return Self::default()};
        let mut badges=Self::default();
        for item in list{
            let Some(key)=item.get("app").and_then(|v|v.as_str()) else{continue};
            let Some(count)=item.get("count").and_then(|v|v.as_u64()) else{continue};
            if let Some(social)=Social::ALL.into_iter().find(|s|s.key()==key){badges.counts[social.index()]=count.min(9999) as u16;}
        }
        badges
    }
    fn save(&self){
        let list:Vec<_>=Social::ALL.into_iter().filter(|s|self.counts[s.index()]>0)
            .map(|s|serde_json::json!({"app":s.key(),"count":self.counts[s.index()]})).collect();
        let path=crate::model::config_dir().join("social-badges.json");
        if let Some(parent)=path.parent(){let _=std::fs::create_dir_all(parent);}
        if let Ok(data)=serde_json::to_vec(&list){let _=crate::model::atomic_write(&path,&data);}
    }
    pub fn bump(&mut self,social:Social){
        let slot=&mut self.counts[social.index()];
        *slot=slot.saturating_add(1).min(9999);
    }
    pub fn clear(&mut self,social:Social){
        self.counts[social.index()]=0;
    }
    pub fn persist(&self){self.save();}
    pub fn count(&self,social:Social)->u16{self.counts[social.index()]}
    pub fn active(&self)->Vec<(Social,u16)>{
        Social::ALL.into_iter().filter_map(|s|{let n=self.count(&s);(n>0).then_some((s,n))}).collect()
    }
    pub fn total(&self)->u32{self.active().iter().map(|(_,n)|*n as u32).sum()}
}

/// 右上角數字徽章；`count` 為 0 時畫小紅點。
pub fn badge(c:&Card,x:f32,y:f32,count:u16,radius:f32,font:f32){
    use crate::paint::W;
    let red=egui::Color32::from_rgb(255,59,48);
    if count==0{c.circle(x,y,radius*0.45,red);return;}
    c.circle(x,y,radius,red);
    let text=if count>99{"99+".to_string()}else{count.to_string()};
    c.centered_text(x,y,&text,font,W::Bold,egui::Color32::WHITE,crate::paint::H::Center);
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn detects_the_five_sources_without_mixing_facebook_and_messenger(){
        assert_eq!(detect("Threads","com.burbn.threads"),Some(Social::Threads));
        assert_eq!(detect("Messenger","com.facebook.messenger"),Some(Social::Messenger));
        assert_eq!(detect("Facebook","com.facebook.katana"),Some(Social::Facebook));
        assert_eq!(detect("LINE","jp.naver.line"),Some(Social::Line));
        assert_eq!(detect("Instagram","com.instagram.android"),Some(Social::Instagram));
        assert_eq!(detect("LINE","jp.naver.line"),Some(Social::Line));
        assert_eq!(detect("Outlook","Microsoft.Outlook"),None);
    }
    #[test]
    fn counts_survive_a_round_trip_and_clear(){
        let mut badges=Badges::default();
        badges.bump(Social::Threads);badges.bump(Social::Threads);badges.bump(Social::Line);
        assert_eq!(badges.count(Social::Threads),2);
        assert_eq!(badges.total(),3);
        badges.clear(Social::Threads);
        assert_eq!(badges.count(Social::Threads),0);
        assert_eq!(badges.total(),1);
    }
}

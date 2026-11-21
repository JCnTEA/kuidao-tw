//! 信箱服務圖示：統一 224px 內容圓、透明背景，設定頁與島上通知共用。
use crate::mail::Provider;
use crate::paint::Card;
use eframe::egui;

pub(crate) fn image(provider: Provider) -> egui::Image<'static> {
    match provider {
        Provider::Yahoo => egui::Image::new(egui::include_image!("../assets/mail-icons/yahoo.png")),
        Provider::ICloud => egui::Image::new(egui::include_image!("../assets/mail-icons/icloud.png")),
        Provider::Gmail => egui::Image::new(egui::include_image!("../assets/mail-icons/gmail.png")),
        _ => egui::Image::new(egui::include_image!("../assets/mail-icons/generic.png")),
    }
}

pub(crate) fn paint(c: &Card, provider: Provider, x: f32, y: f32, size: f32) {
    let rect = c.rect(x - size / 2.0, y - size / 2.0, size, size);
    let image = image(provider);
    if let Ok(egui::load::TexturePoll::Ready { texture }) = image.load_for_size(c.ui.ctx(), rect.size()) {
        c.painter.image(
            texture.id,
            rect,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_provider_has_an_icon_and_the_three_main_services_are_distinct() {
        for provider in Provider::ALL {
            let _ = image(provider);
        }
        assert_eq!(Provider::Yahoo.key(), "yahoo");
        assert_eq!(Provider::Gmail.key(), "gmail");
        assert_eq!(Provider::ICloud.key(), "icloud");
    }
}

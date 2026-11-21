//! 信箱服務圖示：512px 畫布、448px 平面品牌圓、透明背景，
//! 與社群通知圖示共用同一套圓形比例，設定頁與島上通知共用。
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

    #[test]
    fn mail_icons_keep_the_same_round_geometry_as_social_icons() {
        for bytes in [
            include_bytes!("../assets/mail-icons/gmail.png").as_slice(),
            include_bytes!("../assets/mail-icons/yahoo.png").as_slice(),
            include_bytes!("../assets/mail-icons/icloud.png").as_slice(),
            include_bytes!("../assets/mail-icons/generic.png").as_slice(),
        ] {
            let image = image::load_from_memory(bytes).expect("信箱圖示必須可讀").into_rgba8();
            assert_eq!(image.dimensions(), (512, 512), "信箱圖示固定 512×512 畫布");
            assert_eq!(image.get_pixel(0, 0).0[3], 0, "圓外必須透明");
            let mut min_x = 512;
            let mut max_x = 0;
            for y in 0..512 {
                for x in 0..512 {
                    if image.get_pixel(x, y).0[3] > 16 {
                        min_x = min_x.min(x);
                        max_x = max_x.max(x);
                    }
                }
            }
            assert!((448..=458).contains(&(max_x - min_x + 1)), "內容圓直徑要與社群圖示一致");
        }
    }
}

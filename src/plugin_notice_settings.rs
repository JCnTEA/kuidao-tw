use crate::app::App;
use crate::extras::{settings_card, settings_control_row, settings_hint, settings_label};
use crate::plugin_notice::{Bridge, Event};
use eframe::egui;

#[derive(Default)]
pub(crate) struct NoticeHost {
    bridge: Option<Bridge>,
    source_id: String,
    source_name: String,
    message: String,
}

impl App {
    pub fn start_plugin_notices(&mut self, ctx: &egui::Context) {
        match Bridge::start(&crate::model::config_dir(), ctx.clone()) {
            Ok(bridge) => {
                bridge.display(
                    self.settings.notifications,
                    self.settings.notification_preview,
                );
                self.plugin_notice_host.bridge = Some(bridge);
            }
            Err(error) => self.plugin_notice_host.message = error,
        }
    }

    pub fn poll_plugin_notices(&mut self) {
        if let Some(bridge) = self
            .plugin_notice_host
            .bridge
            .as_ref()
            .filter(|bridge| !bridge.sources().is_empty())
        {
            bridge.display(
                self.settings.notifications,
                self.settings.notification_preview,
            );
            for event in bridge.drain() {
                self.receive_plugin_notice(event);
            }
        }
    }

    pub fn request_plugin_notice_open(&mut self, source: &str, id: &str, session: &str) {
        if !self.settings.notifications {
            return;
        }
        let sent = self
            .plugin_notice_host
            .bridge
            .as_ref()
            .is_some_and(|bridge| bridge.open_session(source, id, session).is_ok());
        if !sent {
            self.receive_plugin_notice(Event::Opened {
                source: source.to_owned(),
                id: id.to_owned(),
                ok: false,
            });
        }
    }

    pub(crate) fn plugin_notice_settings(&mut self, ui: &mut egui::Ui) {
        let host = &mut self.plugin_notice_host;
        settings_label(ui, "本機通知來源");
        let mut revoke = None;
        settings_card(ui, |ui| {
            let Some(bridge) = &mut host.bridge else {
                settings_hint(
                    ui,
                    if host.message.is_empty() {
                        "本機接入尚未啟動"
                    } else {
                        &host.message
                    },
                );
                return;
            };
            settings_hint(
                ui,
                "接入端可發送通知、更新狀態並處理你點擊的動作。關閉提醒會暫停接收，撤銷全部授權會關閉本機接口。",
            );
            for source in bridge.sources() {
                ui.push_id(&source.id, |ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("撤銷授權").clicked() {
                            revoke = Some(source.id.clone());
                        }
                        if ui.button("複製憑據路徑").clicked() {
                            let path = bridge.connection_file(&source.id);
                            ui.ctx().copy_text(path.to_string_lossy().into_owned());
                            host.message = "已複製憑據文件路徑".into();
                        }
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.add(egui::Label::new(&source.name).truncate())
                                .on_hover_text(&source.id);
                        });
                    });
                });
                ui.separator();
            }
            settings_control_row(ui, "來源 ID", 104.0, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut host.source_id)
                        .char_limit(64)
                        .hint_text("由接入端提供，例如 author.tool"),
                )
            });
            settings_control_row(ui, "顯示名稱", 104.0, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut host.source_name)
                        .char_limit(40)
                        .hint_text("顯示在島上提醒中"),
                )
            });
            let ready = !host.source_id.trim().is_empty() && !host.source_name.trim().is_empty();
            if ui
                .add_enabled(ready, egui::Button::new("授權來源"))
                .clicked()
            {
                host.message =
                    match bridge.authorize(host.source_id.trim(), host.source_name.trim()) {
                        Ok(()) => {
                            host.source_id.clear();
                            host.source_name.clear();
                            "已授權。將憑據文件路徑交給接入端。".into()
                        }
                        Err(error) => error,
                    };
            }
            if let Some(id) = revoke.take() {
                host.message = match bridge.revoke(&id) {
                    Ok(()) => {
                        revoke = Some(id);
                        "已撤銷授權".into()
                    }
                    Err(error) => error,
                };
            }
            if !host.message.is_empty() {
                settings_hint(ui, &host.message);
            }
        });
        if let Some(id) = revoke {
            self.remove_plugin_notice_source(&id);
        }
        if let Some(bridge) = &self.plugin_notice_host.bridge {
            bridge.display(
                self.settings.notifications,
                self.settings.notification_preview,
            );
        }
    }
}

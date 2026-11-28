//! 集市頂部的「做插件」入口和新手教程抽屜。
//!
//! 教程分「還沒做好」「已經做好了」兩種情況，同一位置、同一結構：已開放能力、
//! 一句話交給 AI 的卡片、可展開的完整步驟，底部固定開發者工具和投稿指南。

use super::*;
use std::path::{Path, PathBuf};

/// 島仔插件投稿倉，AI 助手從這裡讀規範和投稿要求。
const REPO: &str = "https://github.com/Renly1994/xiaotang-daozai-plugins";

fn guide_id() -> egui::Id {
    egui::Id::new("plugin-guide")
}

/// 「從模板創建」生成的插件文件夾，教程裡的那段話會自動填上。
pub(super) fn template_dir_id() -> egui::Id {
    guide_id().with("template")
}

#[derive(Clone, Default)]
struct GuideState {
    have: bool,
    idea: String,
    existing: String,
    copied: Option<f64>,
}

const IDEA_EXAMPLE: &str = "在待辦裡顯示離下次還信用卡還有幾天";
const EXISTING_EXAMPLE: &str = "我寫的番茄鍾統計腳本，代碼在 D:\\我的工具\\pomodoro";

pub(super) fn open_guide(ui: &egui::Ui) {
    let now = ui.input(|input| input.time);
    ui.data_mut(|data| data.insert_temp(guide_id().with("opened"), now));
    ui.ctx().request_repaint();
}

fn close_guide(ui: &egui::Ui) {
    ui.data_mut(|data| data.remove::<f64>(guide_id().with("opened")));
}

/// 發給 AI 助手的那段話。`idea` 為空時用示例佔位。
pub(super) fn prompt(have: bool, idea: &str, template: Option<&Path>, exe: Option<&Path>) -> (String, std::ops::Range<usize>) {
    let idea = if idea.trim().is_empty() {
        if have { EXISTING_EXAMPLE } else { IDEA_EXAMPLE }
    } else {
        idea.trim()
    };
    let mut text = if have {
        "幫我把已有的功能改造成島仔（Windows 靈動島工具）插件：".to_owned()
    } else {
        "幫我做一個島仔（Windows 靈動島工具）插件：".to_owned()
    };
    let start = text.len();
    text.push_str(idea);
    let range = start..text.len();
    text.push('。');
    if have {
        text.push_str("先判斷它需要的能力島仔現在支不支持，不支持的部分先告訴我。");
    }
    text.push('\n');
    match template {
        Some(dir) => text.push_str(&format!(
            "插件模板在 {}（島仔「從模板創建」生成）。",
            dir.display()
        )),
        None => text.push_str(
            "插件模板我會在島仔「設置 › 插件 › 開發者工具 › 從模板創建」生成，再告訴你文件夾位置。",
        ),
    }
    text.push_str(&format!("規範和投稿要求看 {REPO} 。\n要求：\n"));
    text.push_str("- 只改模板裡的 main.js、plugin.json 和 cover.png，用 JavaScript\n");
    text.push_str("- refresh 每次從頭運行，需要記住的內容放進返回的 storage\n");
    text.push_str("- 讓用戶填的名字、日期寫成 plugin.json 裡的設置項\n");
    let exe = exe.map_or_else(|| "daozai.exe".to_owned(), |exe| exe.display().to_string());
    text.push_str(&format!(
        "- 改完運行 \"{exe}\" --plugin-test plugin.json，直到校驗通過\n"
    ));
    text.push_str("- 可追加 --input mock.json，模擬設置、私有數據、當前時間和網絡響應\n");
    text.push_str("- 最後告訴我怎麼在島仔裡安裝試用");
    (text, range)
}

/// 集市頂部的教程入口：虛線框橫幅，整塊可點。
pub(super) fn guide_banner(ui: &mut egui::Ui) -> bool {
    let ctx = ui.ctx().clone();
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 60.0), egui::Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "看教程"));
    let accent = plugin_accent(ui);
    let painter = ui.painter_at(rect.expand(1.0));
    let left = color(&ctx, Color32::from_rgb(42, 36, 56), Color32::from_rgb(243, 239, 253));
    let right = color(&ctx, Color32::from_rgb(36, 36, 42), Color32::from_rgb(250, 250, 252));
    let mut mesh = egui::Mesh::default();
    let mid = egui::pos2(rect.left() + rect.width() * 0.7, rect.top());
    for (a, b, ca, cb) in [
        (rect.left_top(), egui::pos2(mid.x, rect.bottom()), left, right),
        (mid, rect.right_bottom(), right, right),
    ] {
        let base = mesh.vertices.len() as u32;
        mesh.colored_vertex(a, ca);
        mesh.colored_vertex(egui::pos2(b.x, a.y), cb);
        mesh.colored_vertex(egui::pos2(a.x, b.y), ca);
        mesh.colored_vertex(b, cb);
        mesh.add_triangle(base, base + 1, base + 2);
        mesh.add_triangle(base + 1, base + 2, base + 3);
    }
    painter.add(egui::Shape::mesh(mesh));
    let border = if response.hovered() || response.has_focus() {
        accent
    } else {
        color(&ctx, Color32::from_rgb(74, 65, 96), Color32::from_rgb(205, 196, 238))
    };
    let r = rect.shrink(0.5);
    let outline = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
    painter.extend(egui::Shape::dashed_line(&outline, egui::Stroke::new(1.0, border), 5.0, 3.0));

    let icon = egui::Rect::from_min_size(egui::pos2(rect.left() + 14.0, rect.center().y - 18.0), egui::vec2(36.0, 36.0));
    let tag_fg = color(&ctx, Color32::from_rgb(215, 201, 252), Color32::from_rgb(91, 80, 140));
    painter.rect_filled(icon, 9.0, color(&ctx, Color32::from_rgb(58, 49, 81), Color32::from_rgb(232, 225, 250)));
    painter.text(icon.center(), egui::Align2::CENTER_CENTER, "</>", egui::FontId::monospace(13.0), tag_fg);

    let button_text = ui.fonts(|fonts| {
        fonts.layout_no_wrap("看教程".to_owned(), egui::FontId::proportional(12.5), Color32::WHITE)
    });
    let button = egui::Rect::from_min_size(
        egui::pos2(rect.right() - 14.0 - button_text.size().x - 24.0, rect.center().y - 14.0),
        egui::vec2(button_text.size().x + 24.0, 28.0),
    );
    painter.rect_filled(button, 7.0, accent);
    painter.galley(
        button.center() - button_text.size() / 2.0,
        button_text,
        Color32::WHITE,
    );

    let text_left = icon.right() + 14.0;
    let text_w = (button.left() - 14.0 - text_left).max(40.0);
    let strong = ui.visuals().strong_text_color();
    let muted = muted_color(ui);
    let line = |text: &str, size: f32, tint: Color32| {
        let mut job = egui::text::LayoutJob::simple_singleline(
            text.to_owned(),
            egui::FontId::proportional(size),
            tint,
        );
        job.wrap.max_width = text_w;
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        ui.fonts(|fonts| fonts.layout_job(job))
    };
    let title = line("把你做的功能變成插件，放到集市給大家用", 13.5, strong);
    let sub = line("不會寫代碼也能跟著做，已經做好的功能也能搬進來", 12.0, muted);
    let total = title.size().y + 3.0 + sub.size().y;
    let top = rect.center().y - total / 2.0;
    painter.galley(egui::pos2(text_left, top), title.clone(), strong);
    painter.galley(egui::pos2(text_left + 0.3, top), title.clone(), strong);
    painter.galley(egui::pos2(text_left, top + title.size().y + 3.0), sub, muted);
    response.clicked()
}

/// 右側整窗抽屜外殼：蓋住整個設置窗口並置灰背後，從右側滑入。Esc 和點背景都會關；
/// 上面另有彈窗時交給彈窗處理，免得一次 Esc 關掉兩層。`content` 返回真也表示關閉。
pub(super) fn side_drawer(
    ui: &mut egui::Ui,
    salt: &str,
    opened: f64,
    content: impl FnOnce(&mut egui::Ui) -> bool,
) -> bool {
    let ctx = ui.ctx().clone();
    let clip = ctx.screen_rect();
    let elapsed = (ui.input(|input| input.time) - opened) as f32;
    let t = if crate::bar_layout::animations_enabled() {
        let t = (elapsed / 0.2).clamp(0.0, 1.0);
        if t < 1.0 {
            ctx.request_repaint();
        }
        1.0 - (1.0 - t).powi(3)
    } else {
        1.0
    };
    let width = (clip.width() * 0.92).min(440.0);
    let drawer_id = egui::Id::new((salt, "drawer"));
    let layer = egui::LayerId::new(egui::Order::Foreground, drawer_id);
    let on_top = ctx.memory(|memory| memory.is_above_modal_layer(layer));
    let mut close = on_top && ctx.input(|input| input.key_pressed(egui::Key::Escape));
    let scrim = egui::Area::new(egui::Id::new((salt, "scrim")))
        .order(egui::Order::Middle)
        .fixed_pos(clip.min)
        .show(&ctx, |ui| {
            let (rect, response) = ui.allocate_exact_size(clip.size(), egui::Sense::click());
            ui.painter()
                .rect_filled(rect, 0.0, Color32::from_black_alpha((120.0 * t) as u8));
            response.clicked()
        });
    close |= on_top && scrim.inner;
    let left = clip.right() - width * t;
    egui::Area::new(drawer_id)
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(left, clip.top()))
        .show(&ctx, |ui| {
            egui::Frame::new()
                .fill(color(&ctx, Color32::from_rgb(30, 30, 34), Color32::WHITE))
                .stroke(egui::Stroke::new(1.0, line_color(ui)))
                .inner_margin(egui::Margin::same(18))
                .show(ui, |ui| {
                    ui.set_width(width - 36.0);
                    ui.set_height(clip.height() - 36.0);
                    settings_apply_style(ui);
                    close |= content(ui);
                });
        });
    close
}

/// 抽屜右上角的 ×。
pub(super) fn drawer_close(ui: &mut egui::Ui) -> bool {
    ui.add(egui::Button::new(egui::RichText::new("×").size(18.0)).frame(false))
        .on_hover_text("關閉")
        .clicked()
}

fn sdk_capabilities(ui: &mut egui::Ui) {
    let groups: &[(&str, &[(&str, &str)])] = &[
        ("島上顯示", &[
            ("卡片與標註", "待辦信息卡、日曆日期標註，可在各自的功能頁顯示。"),
            ("功能頁", "可做獨立狀態頁，支持指標、進度、列表、折線、標籤、告警和雙色標題。"),
            ("自繪畫布", "用文字、圖形和圖片自由排版，可鋪滿狀態頁，也可與標準組件混排。"),
            ("島上常駐", "開啟常駐的插件在島收起時一起顯示，可在「頁面管理 › 島上顯示」排序。同一插件可提供多張輪播畫布。"),
        ]),
        ("交互與數據", &[
            ("設置與刷新", "支持文本、日期、開關、選項、數字、時刻、星期、時區與列表，可保存私有數據、讀取包內 JSON 並定時刷新。"),
            ("頁面交互", "支持按鈕、單行或多行輸入、可點列表和畫布交互，可打開 HTTPS 鏈接或複製文字。"),
            ("實時組件", "島仔計算時鐘、倒計時和跳動數字，可將敏感值默認打碼。"),
            ("專注事件", "可訂閱專注計時結束事件，由島仔調用插件腳本。"),
            #[cfg(windows)]
            ("一次性提醒", "可發出提醒，由島仔按用戶的通知和正文設置顯示。"),
            #[cfg(windows)]
            ("授權取數", "只讀用戶選定的文件、本機 HTTP 或白名單內的 HTTPS。HTTPS 最多 3 個 GET 來源，可調刷新間隔，密鑰由 Windows 託管。"),
            ("AI 額度", "為現有 AI 額度面板添加自定義數據來源。"),
        ]),
        ("資源與接入", &[
            ("外觀資源", "可提供皮膚、功能圖標、吉祥物表情和畫布字體。"),
            ("快捷導航", "為已啟用的功能頁提供底部快捷入口，可配置選中顏色。"),
            ("無線接收器", "可為兼容的 USB 無線接收器提供識別配置和電量讀取規則。"),
            #[cfg(all(windows, feature = "sdk-material"))]
            ("GPU 材質", "可安裝自定義材質，需開啟 DX12 兼容渲染。"),
            #[cfg(windows)]
            ("本機接入", "獨立接入端可發通知、更新狀態卡，並接收用戶點擊發出的操作。"),
        ]),
        ("開發調試", &[
            ("本地工具", "可從模板創建、安裝、監聽文件變化和查看腳本日誌，並用模擬數據校驗及本地打包。"),
        ]),
    ];
    egui::Frame::new()
        .fill(card_color(ui))
        .stroke(egui::Stroke::new(1.0, line_color(ui)))
        .corner_radius(10.0)
        .inner_margin(egui::Margin::symmetric(12, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let spacing = ui.spacing_mut();
            spacing.item_spacing.y = 4.0;
            spacing.button_padding.y = 2.0;
            spacing.interact_size.y = 24.0;
            egui::CollapsingHeader::new(egui::RichText::new("當前版本已開放的 SDK 能力").size(13.5).strong())
                .id_salt("plugin-guide-sdk-capabilities")
                .show_background(false)
                .show(ui, |ui| {
                    let muted = muted_color(ui);
                    let accent = plugin_accent(ui);
                    for (index, (group, capabilities)) in groups.iter().enumerate() {
                        if index > 0 {
                            ui.separator();
                        }
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new(*group).size(11.5).strong().color(accent));
                        for (label, description) in *capabilities {
                            ui.scope(|ui| {
                                ui.spacing_mut().item_spacing.y = 2.0;
                                ui.add(egui::Label::new(egui::RichText::new(*label).size(12.5).strong()).wrap());
                                ui.add(egui::Label::new(egui::RichText::new(*description).size(12.0).color(muted)).wrap());
                            });
                        }
                    }
                    ui.add_space(4.0);
                });
        });
}

/// 步驟說明裡用 `反引號` 包起來的路徑和文件名，顯示成小底色塊。
fn step_text(ui: &egui::Ui, text: &str) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let muted = muted_color(ui);
    let code_bg = color(ui.ctx(), Color32::from_rgb(52, 52, 59), Color32::from_rgb(236, 237, 242));
    let strong = ui.visuals().strong_text_color();
    for (index, part) in text.split('`').enumerate() {
        if part.is_empty() {
            continue;
        }
        let format = if index % 2 == 1 {
            egui::TextFormat {
                font_id: egui::FontId::proportional(11.5),
                color: strong,
                background: code_bg,
                ..Default::default()
            }
        } else {
            egui::TextFormat {
                font_id: egui::FontId::proportional(12.0),
                color: muted,
                line_height: Some(19.0),
                ..Default::default()
            }
        };
        let part = if index % 2 == 1 { format!(" {part} ") } else { part.to_owned() };
        job.append(&part, 0.0, format);
    }
    job
}

fn steps(ui: &mut egui::Ui, items: &[(&str, &str)]) {
    let accent_bg = color(ui.ctx(), Color32::from_rgb(58, 49, 81), Color32::from_rgb(232, 225, 250));
    let accent_fg = color(ui.ctx(), Color32::from_rgb(215, 201, 252), Color32::from_rgb(91, 80, 140));
    for (index, (title, text)) in items.iter().enumerate() {
        egui::Frame::new()
            .fill(card_color(ui))
            .stroke(egui::Stroke::new(1.0, line_color(ui)))
            .corner_radius(9.0)
            .inner_margin(egui::Margin::symmetric(12, 11))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
                    ui.painter().circle_filled(rect.center(), 11.0, accent_bg);
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        (index + 1).to_string(),
                        egui::FontId::proportional(12.0),
                        accent_fg,
                    );
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 3.0;
                        ui.label(egui::RichText::new(*title).size(13.0).strong());
                        let job = step_text(ui, text);
                        ui.add(egui::Label::new(job).wrap());
                    });
                });
            });
        ui.add_space(2.0);
    }
}

const NEW_STEPS: [(&str, &str); 5] = [
    ("想好要顯示什麼", "用一句話寫下來，比如「收起時顯示距離發薪還有幾天」。先選好內容放在哪個功能頁，或者是否要在島上常駐。"),
    ("用模板生成插件", "到 `開發者工具 › 從模板創建`，填插件 ID、名稱和作者，選一個文件夾。島仔會生成可直接運行的樣例。"),
    ("改成你的內容", "用記事本或 VS Code 打開文件夾。`main.js` 寫顯示邏輯，`plugin.json` 聲明要用的能力與設置。把封面換成你的 `cover.png`（16:9，不超過 256 KB）。"),
    ("裝到自己的島仔上試", "在開發者工具點「安裝本地插件…」，選 plugin.json，打開插件開關。勾選「監視文件變化」後保存本地文件即可重載，運行信息可在「腳本日誌」裡查看。"),
    ("校驗並投稿到集市", "點「本地校驗」通過後，按投稿指南提交到島仔插件倉庫。審核通過後，大家就能在集市裡搜到並安裝。"),
];

const HAVE_STEPS: [(&str, &str); 5] = [
    ("用模板生成插件外殼", "到 `開發者工具 › 從模板創建` 生成樣例，裡麵包含清單、入口和封面。"),
    ("把邏輯搬進 refresh", "打開 `main.js`，把你原來計算結果的部分放進 `refresh`：它拿到設置、今天日期和上次保存的數據，返回要顯示的文字和要保存的數據。其他語言寫的，可以讓 AI 助手改寫成 JavaScript。"),
    ("把要用戶填的東西變成設置", "原來寫死在代碼裡的名字、日期，改成 `plugin.json` 裡的設置項。用戶在設置頁填，插件刷新時讀到。"),
    ("配一張封面", "放一張 16:9 的 `cover.png`，不超過 256 KB、1920×1080。集市卡片和詳情會原樣顯示它。"),
    ("試裝、校驗、投稿", "「安裝本地插件…」裝上試用，「本地校驗」通過後按投稿指南提交。之後發新版本只要改版本號再提交一次，用戶會看到「更新」。"),
];

/// 打開開發者工具；`template` 為真時直接進入「從模板創建」表單。
fn open_developer_tools(ui: &egui::Ui, template: bool) {
    let key = egui::Id::new("plugin-developer-tools");
    ui.data_mut(|data| {
        data.insert_temp(key, true);
        if template {
            data.insert_temp(key.with("author"), plugin_author_ui::AuthorTools::template());
        }
    });
    ui.ctx().request_repaint();
}

impl LocalPlugins {
    /// 教程抽屜。打開時蓋住整個設置窗口。
    pub(super) fn guide_drawer(&mut self, ui: &mut egui::Ui) {
        let Some(opened) = ui.data(|data| data.get_temp::<f64>(guide_id().with("opened"))) else {
            return;
        };
        let state_id = guide_id().with("state");
        let mut state = ui.data(|data| data.get_temp::<GuideState>(state_id)).unwrap_or_default();
        let template: Option<PathBuf> = ui.data(|data| data.get_temp(template_dir_id()));
        let exe = std::env::current_exe().ok();
        let now = ui.input(|input| input.time);
        let close = side_drawer(ui, "plugin-guide", opened, |ui| {
            let mut close = false;
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("做一個島仔插件").size(17.0).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    close |= drawer_close(ui);
                    let options = ["還沒做好".to_owned(), "已經做好了".to_owned()];
                    if let Some(index) = segmented(ui, "plugin-guide-mode", &options, usize::from(state.have), &[false, false], 12.0) {
                        state.have = index == 1;
                        state.copied = None;
                    }
                });
            });
            ui.add_space(8.0);
            let footer_h = 50.0;
            egui::ScrollArea::vertical()
                .id_salt("plugin-guide-scroll")
                .max_height((ui.available_height() - footer_h).max(80.0))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    sdk_capabilities(ui);
                    ui.add_space(8.0);
                    self.guide_ai_card(ui, &mut state, template.as_deref(), exe.as_deref(), now);
                    ui.add_space(6.0);
                    egui::CollapsingHeader::new(egui::RichText::new("想自己動手，看完整步驟").size(13.0))
                        .id_salt(("plugin-guide-manual", state.have))
                        .default_open(false)
                        .show(ui, |ui| {
                            steps(ui, if state.have { &HAVE_STEPS } else { &NEW_STEPS });
                        });
                });
            let top = ui.cursor().top() + 4.0;
            ui.painter().hline(
                ui.max_rect().x_range().expand(18.0),
                top,
                egui::Stroke::new(1.0, line_color(ui)),
            );
            ui.add_space(14.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(market_button(ui, "投稿指南")).clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(plugin_author_ui::GUIDE));
                }
                if ui.button("打開開發者工具").clicked() {
                    open_developer_tools(ui, false);
                }
            });
            close
        });
        if close {
            close_guide(ui);
        }
        ui.data_mut(|data| data.insert_temp(state_id, state));
    }

    fn guide_ai_card(
        &mut self,
        ui: &mut egui::Ui,
        state: &mut GuideState,
        template: Option<&Path>,
        exe: Option<&Path>,
        now: f64,
    ) {
        let ctx = ui.ctx().clone();
        let accent = plugin_accent(ui);
        egui::Frame::new()
            .fill(paint::mix(card_color(ui), accent, 0.12))
            .stroke(egui::Stroke::new(1.0, color(&ctx, Color32::from_rgb(74, 65, 96), Color32::from_rgb(205, 196, 238))))
            .corner_radius(10.0)
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 8.0;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    let title = if state.have { "一句話交給 AI 改造" } else { "一句話交給 AI 做" };
                    ui.label(egui::RichText::new(title).size(13.5).strong());
                    small_tag(ui, "JS 卡片模板");
                });
                let hint = if state.have {
                    "把原代碼的文件夾位置也告訴 AI，請它對照當前 SDK 判斷哪些能力適合遷移到插件。"
                } else {
                    "先點下方「從模板創建」生成插件文件夾，再把這段話發給 Claude Code、Cursor 等 AI 編程助手。模板位置和校驗命令島仔已經幫你填好。"
                };
                ui.add(egui::Label::new(egui::RichText::new(hint).size(12.0).color(muted_color(ui))).wrap());
                let (value, example, salt) = if state.have {
                    (&mut state.existing, EXISTING_EXAMPLE, "plugin-guide-existing")
                } else {
                    (&mut state.idea, IDEA_EXAMPLE, "plugin-guide-idea")
                };
                ui.add_sized(
                    [ui.available_width(), 30.0],
                    egui::TextEdit::singleline(value)
                        .id_salt(salt)
                        .hint_text(example)
                        .char_limit(120),
                );
                let (text, range) = prompt(state.have, value, template, exe);
                egui::Frame::new()
                    .fill(color(&ctx, Color32::from_rgb(26, 26, 30), Color32::from_rgb(247, 247, 250)))
                    .stroke(egui::Stroke::new(1.0, line_color(ui)))
                    .corner_radius(7.0)
                    .inner_margin(egui::Margin::symmetric(10, 9))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        egui::ScrollArea::vertical()
                            .id_salt(("plugin-guide-prompt", state.have))
                            .max_height(170.0)
                            .show(ui, |ui| {
                                let base = egui::TextFormat {
                                    font_id: egui::FontId::proportional(11.5),
                                    color: color(&ctx, Color32::from_rgb(201, 201, 211), Color32::from_rgb(60, 64, 74)),
                                    line_height: Some(19.0),
                                    ..Default::default()
                                };
                                let mut job = egui::text::LayoutJob::default();
                                job.append(&text[..range.start], 0.0, base.clone());
                                job.append(
                                    &text[range.clone()],
                                    0.0,
                                    egui::TextFormat {
                                        color: color(&ctx, Color32::from_rgb(215, 201, 252), Color32::from_rgb(91, 80, 140)),
                                        ..base.clone()
                                    },
                                );
                                job.append(&text[range.end..], 0.0, base);
                                ui.add(egui::Label::new(job).wrap().selectable(true));
                            });
                    });
                ui.horizontal(|ui| {
                    let width = ui.available_width();
                    ui.allocate_ui_with_layout(
                        egui::vec2((width - 196.0).max(80.0), 0.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new("AI 改完後，回來點「安裝本地插件…」試用")
                                        .size(11.5)
                                        .color(muted_color(ui)),
                                )
                                .wrap(),
                            );
                        },
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let copied = state.copied.is_some_and(|at| now - at < 1.5);
                        if copied {
                            ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
                        }
                        let label = if copied { "已複製" } else { "複製這段話" };
                        if ui.add(market_button(ui, label)).clicked() {
                            ui.ctx().copy_text(text.clone());
                            state.copied = Some(now);
                        }
                        if ui.button("從模板創建").clicked() {
                            open_developer_tools(ui, true);
                        }
                    });
                });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_fills_the_idea_template_and_validator() {
        let (text, range) = prompt(
            false,
            "  顯示離週末還有幾天 ",
            Some(Path::new(r"D:\插件\weekend")),
            Some(Path::new(r"C:\Daozai\daozai.exe")),
        );
        assert_eq!(&text[range], "顯示離週末還有幾天");
        assert!(text.contains(r"插件模板在 D:\插件\weekend"));
        assert!(text.contains(r#""C:\Daozai\daozai.exe" --plugin-test plugin.json"#));
        let (text, range) = prompt(true, "", None, None);
        assert_eq!(&text[range], EXISTING_EXAMPLE);
        assert!(text.contains("不支持的部分先告訴我"));
        assert!(text.contains("從模板創建」生成，再告訴你文件夾位置"));
    }
}

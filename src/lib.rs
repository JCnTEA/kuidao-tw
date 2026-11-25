//! 鬼島：Windows 頂部「靈動島」。
//!
//! - `app` / `pages`：與平台無關的界面與動效（外殼與錄製器共用）
//! - `anim` / `paint` / `shape` / `theme`：彈簧、矢量繪製、面板輪廓、配色字體
//! - `model` / `lyrics` / `smtc` / `icons`：數據、歌詞、系統媒體、文件圖標
//! - `native` / `tray`：僅 Windows 的窗口區域、命中穿透與託盤

#[cfg(any(feature="sdk-js",feature="sdk-wasm"))]
#[path="../spike/mod.rs"]
pub mod sdk_spike;
pub mod display_layout;
pub mod diagnostics_tool;
pub mod dev_update;
pub mod audio_id;
pub mod rtl_text;
mod mail_privacy;
#[cfg(windows)]
mod plugin_notice;
#[cfg(windows)]
mod plugin_notice_settings;
#[cfg(windows)]
mod plugin_notice_ui;
pub mod poll_clock;
pub mod resources;
pub mod anim;
pub mod oled_shift;
pub mod app;
pub mod messenger;
pub mod social;
pub mod line_login;
pub mod mascot;
pub mod mascot_pack;
pub mod child_process;
mod settings_window;
pub mod bt;
mod bluetooth_ui;
pub mod calendar;
mod calendar_ui;
mod local_calendar;
mod lunar_calendar;
mod calendar_editor;
pub mod glass;
pub mod skin_paint;
pub mod icons;
pub mod input;
pub mod lyrics;
pub mod lyrics_lang;
pub mod lyrics_library;
pub mod lyrics_zh;
pub mod lyric_gradient;
pub mod lyric_sync;
pub mod lyric_karaoke;
pub mod zh;
pub mod mail;
pub mod mail_format;
mod mail_icon;
mod mail_reader;
pub mod mail_manage;
mod mail_manage_ui;
pub mod clipboard;
pub mod side_dock;
mod task_picker;
pub mod model;
pub mod renderer;
#[cfg(all(windows, feature = "sdk-material"))]
pub mod material;
pub mod otp;
pub mod quota;
#[cfg(feature = "sdk-js")]
mod quota_plugin;
#[cfg(feature = "sdk-js")]
mod plugin_canvas;
#[cfg(feature = "sdk-js")]
mod plugin_canvas_ui;
#[cfg(feature = "sdk-js")]
mod plugin_setting_catalog;
#[cfg(feature = "sdk-js")]
mod plugin_status;
#[cfg(feature = "sdk-js")]
mod plugin_status_ui;
mod quota_ui;
pub mod weather;
mod weather_ui;
mod weather_source_ui;
mod qweather;
mod pages;
mod games;
mod extras;
mod mascot_art;
mod pet;
mod pet_expressions;
mod bar_layout;
pub mod performance;
mod performance_ui;
mod resident_settings;
mod resident_date;
mod avoidance;
mod focus_bar;
#[cfg(windows)]
pub mod backdrop;
#[cfg(windows)]
pub mod composition;
#[cfg(windows)]
pub mod layered;
#[cfg(windows)]
pub mod system;
pub mod paint;
pub mod shape;
mod fluid;
pub mod smtc;
pub mod browser_media;
pub mod music_volume;
pub mod theme;
pub mod settings_theme;
mod power_ui;
mod updates;
mod supporters;
#[cfg(windows)]
pub use updates::record_install_size;

#[cfg(windows)]
pub mod native;
#[cfg(windows)]
mod window_bounds;
#[cfg(windows)]
mod crop_front_buffer;
#[cfg(windows)]
pub mod wechat;
#[cfg(windows)]
pub mod tray;

#[cfg(test)]
mod ui_tests;
#[cfg(test)]
#[path = "bin/daozai-record/raster.rs"]
mod preview_raster;

//! Мазок — приложение для цифрового рисования, написанное на Rust.

pub mod app;
pub mod autotest;
pub mod blend;
pub mod brush;
pub mod brushio;
pub mod doc;
pub mod editor;
pub mod filters;
pub mod formats;
pub mod palette;
pub mod platform;
pub mod raster;
pub mod sqlite;
pub mod state;
pub mod ui;
pub mod util;
pub mod vector;

use std::path::PathBuf;

const ICON: &[u8] = include_bytes!("../../icons/icon-192.png");

fn viewport() -> egui::ViewportBuilder {
    let mut vb = egui::ViewportBuilder::default().with_title("Мазок").with_inner_size([1400.0, 900.0]).with_min_inner_size([480.0, 400.0]);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(ICON) {
        vb = vb.with_icon(std::sync::Arc::new(icon));
    }
    vb
}

/// Каталог данных на компьютере: %APPDATA%\Mazok (или ~/.mazok).
#[cfg(not(target_os = "android"))]
pub fn desktop_data_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("MAZOK_DATA") {
        return PathBuf::from(p);
    }
    if let Some(p) = std::env::var_os("APPDATA") {
        return PathBuf::from(p).join("Mazok");
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".mazok")).unwrap_or_else(|| PathBuf::from("mazok-data"))
}

#[cfg(not(target_os = "android"))]
pub fn run_desktop() {
    let dir = desktop_data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let opts = eframe::NativeOptions { viewport: viewport().with_drag_and_drop(true), vsync: true, multisampling: 0, ..Default::default() };
    let res = eframe::run_native("Мазок", opts, Box::new(move |cc| Ok(Box::new(app::App::new(cc, dir)))));
    if let Err(e) = res {
        let msg = format!("Не удалось запустить Мазок: {e}\n\nНужна видеокарта с поддержкой OpenGL 3.");
        let _ = rfd::MessageDialog::new().set_title("Мазок").set_description(msg).set_level(rfd::MessageLevel::Error).show();
    }
}

#[cfg(target_os = "android")]
pub fn run_desktop() {}

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("mazok"));
    let dir = app.internal_data_path().unwrap_or_else(|| PathBuf::from("/data/local/tmp/mazok"));
    let _ = std::fs::create_dir_all(&dir);
    if let Some(ext) = app.external_data_path() {
        platform::set_external_dir(ext);
    }
    let opts = eframe::NativeOptions { viewport: viewport(), android_app: Some(app), ..Default::default() };
    let res = eframe::run_native("Мазок", opts, Box::new(move |cc| Ok(Box::new(app::App::new(cc, dir)))));
    if let Err(e) = res {
        log::error!("eframe: {e}");
    }
}

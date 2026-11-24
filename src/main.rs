// Sem janela de console no Windows (em modo release).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod downloader;
mod engine;
mod manager;
mod paths;
mod settings;
mod ui;

use eframe::egui;
use std::sync::Arc;

pub const APP_NAME: &str = "Zek Download";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> eframe::Result<()> {
    let icon = egui::IconData {
        rgba: include_bytes!("../assets/icon64.rgba").to_vec(),
        width: 64,
        height: 64,
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(APP_NAME)
            .with_inner_size([940.0, 760.0])
            .with_min_inner_size([800.0, 600.0])
            .with_icon(Arc::new(icon)),
        ..Default::default()
    };
    eframe::run_native(
        APP_NAME,
        options,
        Box::new(|cc| Ok(Box::new(ui::App::new(cc)))),
    )
}

// A desktop app: no console window on Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(not(any(windows, target_os = "macos")))]
compile_error!("model-downloader runs on Windows and macOS; build it on one of those.");

mod app;
mod catalog;
mod download;
mod downloader;
mod format;
mod hardware;
mod volumes;

use eframe::egui;

fn main() -> eframe::Result {
    // Network and disk work runs here; the window owns the main thread,
    // which macOS requires.
    let rt = tokio::runtime::Runtime::new().map_err(|e| eframe::Error::AppCreation(Box::new(e)))?;

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Model Downloader")
        .with_inner_size([1040.0, 700.0])
        .with_min_inner_size([780.0, 500.0]);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    let handle = rt.handle().clone();
    eframe::run_native(
        "Model Downloader",
        options,
        Box::new(move |cc| {
            // egui's defaults are dense; give text and controls room to breathe.
            cc.egui_ctx.all_styles_mut(|style| {
                use egui::{FontId, TextStyle};
                style.spacing.item_spacing = egui::vec2(8.0, 7.0);
                style.spacing.button_padding = egui::vec2(10.0, 5.0);
                style
                    .text_styles
                    .insert(TextStyle::Body, FontId::proportional(14.0));
                style
                    .text_styles
                    .insert(TextStyle::Button, FontId::proportional(14.0));
                style
                    .text_styles
                    .insert(TextStyle::Heading, FontId::proportional(21.0));
                style
                    .text_styles
                    .insert(TextStyle::Small, FontId::proportional(11.5));
            });
            Ok(Box::new(app::App::new(handle)))
        }),
    )
}

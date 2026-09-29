use eframe::egui;
use std::{fs, path::Path};

pub const BACKGROUND: egui::Color32 = egui::Color32::from_rgb(15, 18, 24);
pub const SIDEBAR: egui::Color32 = egui::Color32::from_rgb(20, 24, 32);
pub const SURFACE: egui::Color32 = egui::Color32::from_rgb(25, 30, 40);
pub const MAP_BACKGROUND: egui::Color32 = egui::Color32::from_rgb(12, 20, 25);
pub const GRID: egui::Color32 = egui::Color32::from_rgb(32, 51, 58);
pub const MUTED_TEXT: egui::Color32 = egui::Color32::from_rgb(137, 151, 165);
pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(66, 211, 146);
pub const SUCCESS: egui::Color32 = egui::Color32::from_rgb(91, 214, 140);
pub const WARNING: egui::Color32 = egui::Color32::from_rgb(245, 184, 74);
pub const DANGER: egui::Color32 = egui::Color32::from_rgb(239, 101, 101);

pub fn install(ctx: &egui::Context) {
    install_chinese_font(ctx);

    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = BACKGROUND;
    style.visuals.window_fill = SURFACE;
    style.visuals.faint_bg_color = SURFACE;
    style.visuals.selection.bg_fill = ACCENT.gamma_multiply(0.65);
    style.visuals.hyperlink_color = ACCENT;
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    ctx.set_style(style);
}

fn install_chinese_font(ctx: &egui::Context) {
    let candidates = [
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\msyhbd.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/System/Library/Fonts/PingFang.ttc",
    ];

    let Some(data) = candidates
        .iter()
        .find_map(|path| fs::read(Path::new(path)).ok())
    else {
        return;
    };

    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "life_paper_cjk".to_string(),
        egui::FontData::from_owned(data).into(),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, "life_paper_cjk".to_string());
    }
    ctx.set_fonts(fonts);
}

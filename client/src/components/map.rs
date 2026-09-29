use crate::{config::Language, i18n::text, theme};
use eframe::egui;

pub fn show(ui: &mut egui::Ui, language: Language) {
    let desired_size = egui::vec2(ui.available_width(), ui.available_height().max(360.0));
    let (response, painter) = ui.allocate_painter(desired_size, egui::Sense::click_and_drag());
    let rect = response.rect;

    painter.rect_filled(rect, 8, theme::MAP_BACKGROUND);

    let cell_size = 34.0;
    let columns = (rect.width() / cell_size).ceil() as usize;
    let rows = (rect.height() / cell_size).ceil() as usize;

    for column in 0..=columns {
        let x = rect.left() + column as f32 * cell_size;
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(1.0, theme::GRID),
        );
    }
    for row in 0..=rows {
        let y = rect.top() + row as f32 * cell_size;
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            egui::Stroke::new(1.0, theme::GRID),
        );
    }

    for (column, row, color) in [
        (4, 5, theme::ACCENT),
        (9, 3, theme::WARNING),
        (13, 8, theme::SUCCESS),
        (18, 6, theme::DANGER),
    ] {
        let center = egui::pos2(
            rect.left() + (column as f32 + 0.5) * cell_size,
            rect.top() + (row as f32 + 0.5) * cell_size,
        );
        if rect.contains(center) {
            painter.circle_filled(center, 7.0, color);
            painter.circle_stroke(
                center,
                11.0,
                egui::Stroke::new(1.0, color.gamma_multiply(0.55)),
            );
        }
    }

    painter.text(
        rect.left_top() + egui::vec2(14.0, 12.0),
        egui::Align2::LEFT_TOP,
        text(language, "world.map_preview"),
        egui::FontId::proportional(14.0),
        theme::MUTED_TEXT,
    );
}

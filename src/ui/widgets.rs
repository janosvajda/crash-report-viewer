//! Small reusable controls with stable sizing across hover and selection states.

use eframe::egui::{self, Align, Align2, Layout, RichText, Sense};

pub fn selection_row(
    ui: &mut egui::Ui,
    selected: bool,
    text: impl Into<String>,
    height: f32,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), height), Sense::click());
    if selected {
        ui.painter()
            .rect_filled(rect, 5.0, ui.visuals().selection.bg_fill);
        ui.painter().rect_filled(
            egui::Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height())),
            2.0,
            ui.visuals().selection.stroke.color,
        );
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, 5.0, ui.visuals().faint_bg_color);
    }
    ui.painter().with_clip_rect(rect.shrink(6.0)).text(
        rect.left_center() + egui::vec2(10.0, 0.0),
        Align2::LEFT_CENTER,
        text.into(),
        egui::TextStyle::Button.resolve(ui.style()),
        if selected {
            ui.visuals().strong_text_color()
        } else {
            ui.visuals().text_color()
        },
    );
    response
}

pub fn filter(ui: &mut egui::Ui, value: &mut String, count: usize) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{count} items")).color(ui.visuals().weak_text_color()));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_sized(
                [220.0, 28.0],
                egui::TextEdit::singleline(value).hint_text("Filter"),
            );
        });
    });
    ui.add_space(5.0);
}

pub fn pane_heading(ui: &mut egui::Ui, title: &str, caption: &str) {
    ui.label(RichText::new(title).size(16.0).strong());
    ui.label(
        RichText::new(caption)
            .small()
            .color(ui.visuals().weak_text_color()),
    );
    ui.add_space(8.0);
}

pub fn evidence(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(
        RichText::new(label)
            .small()
            .strong()
            .color(ui.visuals().weak_text_color()),
    );
    ui.label(RichText::new(value).monospace());
    ui.end_row();
}

pub fn value_or_unknown(value: &str) -> &str {
    if value.is_empty() {
        "Not available"
    } else {
        value
    }
}

pub fn section_title(ui: &mut egui::Ui, title: &str, caption: &str) {
    ui.label(RichText::new(title).size(20.0).strong());
    ui.label(RichText::new(caption).color(ui.visuals().weak_text_color()));
    ui.add_space(12.0);
}

pub fn header_row(ui: &mut egui::Ui, cells: &[&str]) {
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.columns(cells.len(), |columns| {
                for (column, cell) in columns.iter_mut().zip(cells) {
                    column.label(
                        RichText::new(*cell)
                            .small()
                            .strong()
                            .color(column.visuals().weak_text_color()),
                    );
                }
            });
        });
}

#[cfg(test)]
mod tests {
    use super::value_or_unknown;

    #[test]
    fn empty_values_have_a_consistent_fallback() {
        assert_eq!(value_or_unknown(""), "Not available");
        assert_eq!(value_or_unknown("present"), "present");
    }
}

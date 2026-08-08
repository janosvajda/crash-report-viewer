//! Cross-report evidence search.

use crate::{domain::DumpReport, services::investigation, ui::widgets::section_title};
use eframe::egui::{self, RichText};

pub fn search(ui: &mut egui::Ui, report: &DumpReport, query: &mut String) {
    section_title(
        ui,
        "Search crash evidence",
        "Find functions, modules, source paths, thread names, addresses and stream types.",
    );
    ui.add_sized(
        [ui.available_width(), 34.0],
        egui::TextEdit::singleline(query).hint_text("Search all crash data…"),
    );
    ui.add_space(12.0);
    if query.trim().is_empty() {
        ui.label(
            RichText::new("Enter a term or address to search this dump.")
                .color(ui.visuals().weak_text_color()),
        );
        return;
    }
    let matches = investigation::search(report, query);
    ui.label(RichText::new(format!("{} results", matches.len())).strong());
    egui::ScrollArea::vertical().show_rows(ui, 55.0, matches.len(), |ui, range| {
        for index in range {
            let hit = &matches[index];
            egui::Frame::new()
                .fill(ui.visuals().faint_bg_color)
                .inner_margin(8)
                .show(ui, |ui| {
                    ui.label(RichText::new(hit.kind).small().strong());
                    ui.label(&hit.value);
                });
        }
    });
}

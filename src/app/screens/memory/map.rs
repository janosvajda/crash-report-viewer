//! Visual composition of memory captured in the dump, not a full address-space map.

use super::{
    MemoryAction,
    analysis::{region_references, region_relevance, stack_thread_for_region},
    state::{MemoryAnalysisCache, MemoryMapSummary},
};
use crate::{
    domain::DumpReport,
    services::{analyzer, memory as memory_analysis},
    ui::widgets::pane_heading,
};
use eframe::egui::{self, RichText};

pub(super) fn memory_map_view(
    ui: &mut egui::Ui,
    report: &DumpReport,
    cache: &mut MemoryAnalysisCache,
) -> Option<MemoryAction> {
    cache.refresh_map(report);
    let summary = cache.map_summary.as_ref()?;
    let total = summary.groups.iter().map(|group| group.bytes).sum::<u64>();

    pane_heading(
        ui,
        "Crash focus",
        "The captured memory most directly connected to the failure",
    );
    let action = render_memory_crash_focus(ui, report, summary);
    ui.add_space(22.0);
    ui.separator();
    ui.add_space(14.0);
    pane_heading(
        ui,
        "Captured memory composition",
        "Secondary context: how the saved bytes are distributed",
    );
    ui.label(
        RichText::new("Bar width represents bytes copied into the dump; this is not the application's complete address space.")
            .small()
            .color(ui.visuals().weak_text_color()),
    );
    ui.add_space(8.0);

    let labels = [
        (
            "Crashed stack / fault",
            egui::Color32::from_rgb(218, 105, 38),
        ),
        ("Other thread stacks", egui::Color32::from_rgb(62, 117, 196)),
        ("Module-backed", egui::Color32::from_rgb(126, 82, 180)),
        ("Other captured", egui::Color32::from_rgb(150, 157, 168)),
    ];
    let (bar, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 44.0), egui::Sense::hover());
    let mut x = bar.left();
    for (group, (label, color)) in summary.groups.iter().zip(labels) {
        if group.bytes == 0 || total == 0 {
            continue;
        }
        let width = bar.width() * group.bytes as f32 / total as f32;
        let rect = egui::Rect::from_min_max(
            egui::pos2(x, bar.top()),
            egui::pos2((x + width).min(bar.right()), bar.bottom()),
        );
        ui.painter().rect_filled(rect, 3.0, color);
        if rect.width() > 130.0 {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(14.0),
                egui::Color32::WHITE,
            );
        }
        x += width;
    }

    ui.add_space(8.0);
    ui.horizontal_wrapped(|ui| {
        for (group, (label, color)) in summary.groups.iter().zip(labels) {
            ui.colored_label(color, "■");
            ui.label(RichText::new(label).strong());
            ui.label(format!(
                "{} · {} region{}",
                analyzer::human_bytes(group.bytes),
                group.regions,
                if group.regions == 1 { "" } else { "s" }
            ));
            ui.add_space(18.0);
        }
    });

    action
}

fn render_memory_crash_focus(
    ui: &mut egui::Ui,
    report: &DumpReport,
    summary: &MemoryMapSummary,
) -> Option<MemoryAction> {
    let Some(index) = summary.crash_region else {
        ui.label("No captured region is directly connected to the fault or crashed thread.");
        return None;
    };
    let region = &report.memory[index];
    let roles = memory_analysis::region_roles(report, region);
    let references = region_references(report, region);
    let (priority, title, explanation) = region_relevance(report, region, references.len(), &roles);
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(250, 239, 222))
        .corner_radius(8)
        .inner_margin(16)
        .show(ui, |ui| {
            ui.label(
                RichText::new(format!("{priority} · {title}"))
                    .size(18.0)
                    .strong(),
            );
            ui.label(explanation);
            ui.label(
                RichText::new(format!(
                    "{} captured at {}",
                    analyzer::human_bytes(region.size),
                    region.start
                ))
                .small()
                .color(ui.visuals().weak_text_color()),
            );
        });
    stack_thread_for_region(report, region).and_then(|(thread, id, crashed)| {
        ui.button(if crashed {
            format!("Open crashed thread {id}  →")
        } else {
            format!("Open thread {id}  →")
        })
        .clicked()
        .then_some(MemoryAction { thread, frame: 0 })
    })
}

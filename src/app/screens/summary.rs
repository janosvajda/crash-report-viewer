use super::ScreenAction;
use crate::{
    domain::DumpReport,
    services::{analyzer as dump, investigation},
    ui::{
        view_model::{CrashEvidence, investigation_insights},
        widgets::{section_title, value_or_unknown},
    },
};
use eframe::egui::{self, Color32, RichText};

pub fn summary(ui: &mut egui::Ui, r: &DumpReport) -> Option<ScreenAction> {
    let mut requested_page = None;
    let evidence = CrashEvidence::from_report(r);
    section_title(
        ui,
        "Crash overview",
        "Start with the likely failure, then follow the evidence only when needed.",
    );
    let top_frame = r
        .threads
        .iter()
        .find(|thread| thread.crashed)
        .and_then(|thread| thread.frames.first());

    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .corner_radius(8)
        .inner_margin(14)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.columns(2, |columns| {
                columns[0].label(RichText::new("LIKELY FAILURE").small().strong());
                let location = top_frame
                    .map(|frame| {
                        if !frame.function.is_empty() {
                            frame.function.clone()
                        } else if !frame.module.is_empty() {
                            frame.module.clone()
                        } else {
                            frame.instruction.to_string()
                        }
                    })
                    .unwrap_or_else(|| "No crashing frame recovered".into());
                columns[0].label(RichText::new(location).size(20.0).strong());
                columns[0].label(
                    RichText::new(&r.crash_reason)
                        .strong()
                        .color(columns[0].visuals().error_fg_color),
                );
                columns[0].label(
                    RichText::new(format!(
                        "{} · confidence {}",
                        investigation::likely_cause(r),
                        top_frame
                            .map(|frame| value_or_unknown(&frame.trust))
                            .unwrap_or("unknown")
                    ))
                    .small()
                    .color(columns[0].visuals().weak_text_color()),
                );

                columns[1].label(RichText::new("INVESTIGATE").small().strong());
                if columns[1]
                    .add_sized(
                        [220.0, 34.0],
                        egui::Button::new("Inspect crashed thread  →"),
                    )
                    .clicked()
                {
                    requested_page = Some(ScreenAction::OpenThreads);
                }
                if columns[1]
                    .add_sized([220.0, 30.0], egui::Button::new("View related module"))
                    .clicked()
                {
                    requested_page = Some(ScreenAction::OpenModules);
                }
                if evidence.needs_symbols
                    && columns[1]
                        .add_sized([220.0, 30.0], egui::Button::new("Add missing symbols"))
                        .clicked()
                {
                    requested_page = Some(ScreenAction::ConfigureSymbols);
                }
            });
        });

    ui.add_space(14.0);
    ui.label(RichText::new("What needs attention").size(17.0).strong());
    let insights = investigation_insights(r);
    for insight in insights {
        egui::Frame::new()
            .fill(ui.visuals().faint_bg_color)
            .corner_radius(6)
            .inner_margin(16)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(insight.priority).small().strong().color(
                        if insight.priority == "BLOCKER" || insight.priority == "HIGH" {
                            ui.visuals().error_fg_color
                        } else {
                            ui.visuals().weak_text_color()
                        },
                    ));
                    ui.label(RichText::new(insight.title).strong());
                });
                ui.label(RichText::new(insight.detail).color(ui.visuals().weak_text_color()));
            });
        ui.add_space(5.0);
    }

    ui.add_space(14.0);
    egui::CollapsingHeader::new("Dump capture details")
        .default_open(false)
        .show(ui, |ui| {
            ui.label(format!("Fault address: {}", r.crash_address));
            ui.label(format!(
                "Platform: {} · {}",
                r.operating_system, r.architecture
            ));
            ui.label(format!(
                "Captured: {} threads · {} modules · {} streams",
                r.threads.len(),
                r.modules.len(),
                r.streams.len()
            ));
            ui.label(format!(
                "File: {} · {}",
                dump::human_bytes(r.file_size),
                r.format
            ));
        });
    egui::CollapsingHeader::new("Technical evidence path")
        .default_open(false)
        .show(ui, |ui| {
            ui.label(
                RichText::new("Exception → thread → frame → module → source")
                    .small()
                    .color(ui.visuals().weak_text_color()),
            );
            egui::Grid::new("evidence_path")
                .num_columns(3)
                .spacing([16.0, 7.0])
                .striped(true)
                .show(ui, |ui| {
                    for (index, step) in evidence.steps.iter().enumerate() {
                        ui.label(RichText::new(format!("{}", index + 1)).strong());
                        ui.label(RichText::new(step.title).strong())
                            .on_hover_text(step.explanation);
                        ui.label(RichText::new(&step.value).monospace());
                        ui.end_row();
                    }
                });
        });
    if !r.diagnostics.is_empty() {
        ui.add_space(8.0);
        egui::CollapsingHeader::new(format!("Diagnostics ({})", r.diagnostics.len())).show(
            ui,
            |ui| {
                for diagnostic in &r.diagnostics {
                    ui.colored_label(Color32::from_rgb(180, 120, 25), format!("• {diagnostic}"));
                }
            },
        );
    }
    requested_page
}

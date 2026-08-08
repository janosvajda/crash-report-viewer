use super::ScreenAction;
use crate::{
    domain::{DumpReport, ModuleOwnership},
    services::investigation,
    ui::view_model::{ComparisonAnalysis, FieldDelta, ModuleDelta, ModulePresence},
};
use eframe::egui::{self, Color32, FontId, RichText};
use std::path::Path;

pub fn compare_view(
    ui: &mut egui::Ui,
    current: &DumpReport,
    comparison_path: &mut String,
    comparison: Option<&DumpReport>,
) -> Option<ScreenAction> {
    let mut action = None;
    ui.label(RichText::new("COMPARISON").small().strong());
    ui.label(RichText::new("Compare results").size(21.0).strong());
    if let Some(other) = comparison {
        let analysis = ComparisonAnalysis::new(current, other);
        ui.add_space(8.0);
        ui.columns(2, |columns| {
            comparison_card(
                &mut columns[0],
                "A",
                current,
                Color32::from_rgb(44, 92, 180),
            );
            comparison_card(&mut columns[1], "B", other, Color32::from_rgb(115, 65, 155));
        });
        ui.add_space(10.0);
        let current_evidence = has_crash_evidence(current);
        let other_evidence = has_crash_evidence(other);
        let comparison_limited = !current_evidence || !other_evidence;
        egui::Frame::new()
            .fill(if comparison_limited {
                Color32::from_rgb(250, 232, 232)
            } else if analysis.same_signature {
                Color32::from_rgb(232, 244, 235)
            } else {
                Color32::from_rgb(250, 239, 222)
            })
            .corner_radius(8)
            .inner_margin(18)
            .show(ui, |ui| {
                ui.label(RichText::new(if comparison_limited {
                    "Comparison is incomplete"
                } else if analysis.same_signature {
                    "Likely the same crash"
                } else {
                    "Different crash signatures"
                }).size(19.0).strong());
                ui.label(if comparison_limited {
                    "One dump has no usable exception and crashing-stack evidence. Crash identity cannot be compared reliably; differences below describe available metadata only."
                } else if analysis.same_signature {
                    "The crashing stacks produce the same signature. Focus on environment and module differences."
                } else {
                    "The top crashing frames differ. Treat these as separate failures unless other evidence links them."
                });
                if comparison_limited {
                    ui.add_space(5.0);
                    ui.label(RichText::new(format!(
                        "File A evidence: {}   ·   File B evidence: {}",
                        if current_evidence { "available" } else { "missing" },
                        if other_evidence { "available" } else { "missing" }
                    )).strong());
                }
            });
        ui.add_space(24.0);
        ui.separator();
        ui.add_space(14.0);
        ui.label(RichText::new("1 · Crash event").size(21.0).strong());
        ui.label(
            RichText::new("The facts recorded when each process stopped. Red items directly affect crash identity.")
                .color(ui.visuals().weak_text_color()),
        );
        ui.add_space(6.0);
        comparison_fields(ui, &analysis.changed);
        if analysis.changed.is_empty() {
            ui.label(
                RichText::new("No core crash fields changed.")
                    .color(ui.visuals().weak_text_color()),
            );
        }
        if !analysis.unchanged.is_empty() {
            ui.label(
                RichText::new(format!(
                    "Same in both files: {}",
                    analysis
                        .unchanged
                        .iter()
                        .map(|field| field.field)
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
                .small()
                .color(ui.visuals().weak_text_color()),
            );
        }
        ui.add_space(28.0);
        ui.separator();
        ui.add_space(14.0);
        ui.label(
            RichText::new("2 · Crashing stack evidence")
                .size(21.0)
                .strong(),
        );
        ui.label(
            RichText::new("Aligned stack evidence identifies whether both failures passed through the same code.")
                .color(ui.visuals().weak_text_color()),
        );
        ui.add_space(6.0);
        ui.columns(2, |columns| {
            stack_preview(&mut columns[0], "FILE A", &analysis.current_stack);
            stack_preview(&mut columns[1], "FILE B", &analysis.comparison_stack);
        });

        ui.add_space(28.0);
        ui.separator();
        ui.add_space(14.0);
        ui.label(
            RichText::new("3 · Loaded-code differences")
                .size(21.0)
                .strong(),
        );
        ui.label(
            RichText::new("Application and runtime changes may explain different behaviour. OS inventory changes are summarized separately.")
                .color(ui.visuals().weak_text_color()),
        );
        ui.add_space(6.0);
        module_comparison(
            ui,
            &analysis.changed_modules,
            &analysis.only_current_modules,
            &analysis.only_comparison_modules,
        );
        ui.add_space(16.0);
    } else {
        egui::Frame::new()
            .fill(ui.visuals().faint_bg_color)
            .inner_margin(14)
            .show(ui, |ui| {
                ui.label(RichText::new("Select two dumps in the crash library").strong());
                ui.label("Return to the library, tick two crashes, and choose Compare selected.");
            });
        ui.add_space(12.0);
    }

    if comparison.is_none() {
        ui.label(RichText::new("Or load file B manually").small().strong());
        ui.horizontal(|ui| {
            ui.add_sized(
                [(ui.available_width() - 90.0).max(260.0), 32.0],
                egui::TextEdit::singleline(comparison_path)
                    .font(FontId::monospace(13.0))
                    .hint_text("Path to another .dmp file"),
            );
            if ui.button("Load").clicked() {
                action = Some(ScreenAction::LoadComparison);
            }
        });
    }
    action
}

fn comparison_fields(ui: &mut egui::Ui, fields: &[FieldDelta]) {
    for field in fields {
        let critical = matches!(field.field, "Crash reason" | "Fault address");
        egui::Frame::new()
            .fill(if critical {
                ui.visuals().error_fg_color.gamma_multiply(0.07)
            } else {
                ui.visuals().faint_bg_color
            })
            .corner_radius(6)
            .inner_margin(10)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(field.field).strong());
                    ui.separator();
                    ui.label(RichText::new(field_meaning(field.field)).small());
                });
                ui.columns(2, |columns| {
                    columns[0].label(RichText::new("FILE A").size(17.0).strong());
                    columns[0].label(&field.current);
                    columns[1].label(RichText::new("FILE B").size(17.0).strong());
                    columns[1].label(&field.comparison);
                });
            });
        ui.add_space(5.0);
    }
}

fn has_crash_evidence(report: &DumpReport) -> bool {
    report.crash_thread.is_some()
        && report
            .threads
            .iter()
            .any(|thread| thread.crashed && !thread.frames.is_empty())
        && !report
            .crash_reason
            .eq_ignore_ascii_case("No exception stream")
}

fn field_meaning(field: &str) -> &'static str {
    match field {
        "Crash reason" => "Exception type differs",
        "Fault address" => "Fault location differs or is missing",
        "Operating system" => "Environment differs; context only",
        "Architecture" => "CPU target differs; context only",
        "Thread count" => "Process-state context; not crash identity",
        "Module count" => "Loaded-code inventory differs; inspect section 3",
        _ => "Value changed",
    }
}

fn stack_preview(ui: &mut egui::Ui, label: &str, frames: &[String]) {
    egui::Frame::new()
        .fill(ui.visuals().code_bg_color)
        .corner_radius(7)
        .inner_margin(16)
        .show(ui, |ui| {
            ui.label(RichText::new(label).size(18.0).strong());
            ui.add_space(6.0);
            if frames.is_empty() {
                ui.label("No crashing frames recovered");
            }
            for frame in frames {
                ui.label(RichText::new(frame).monospace());
            }
        });
}

fn module_comparison(
    ui: &mut egui::Ui,
    changed: &[ModuleDelta],
    only_a: &[ModulePresence],
    only_b: &[ModulePresence],
) {
    if changed.is_empty() && only_a.is_empty() && only_b.is_empty() {
        ui.label(RichText::new("No module differences.").color(ui.visuals().weak_text_color()));
        return;
    }
    if !changed.is_empty() {
        ui.label(
            RichText::new(format!(
                "Same module, different location or version ({})",
                changed.len()
            ))
            .strong(),
        );
        ui.label(
            RichText::new("Review these first: both crashes loaded the module, but not the same build or path.")
                .small()
                .color(ui.visuals().weak_text_color()),
        );
        egui::Grid::new("changed_module_paths")
            .num_columns(3)
            .striped(true)
            .spacing([16.0, 7.0])
            .show(ui, |ui| {
                ui.strong("MODULE");
                ui.strong("FILE A");
                ui.strong("FILE B");
                ui.end_row();
                for module in changed {
                    ui.label(RichText::new(&module.name).strong());
                    compact_path_label(ui, &module.current);
                    compact_path_label(ui, &module.comparison);
                    ui.end_row();
                }
            });
        ui.add_space(10.0);
    }

    let relevant_count = only_a
        .iter()
        .chain(only_b)
        .filter(|module| !is_system_module(module))
        .count();
    let system_count = only_a.len() + only_b.len() - relevant_count;
    ui.label(RichText::new("Application/runtime modules present in only one file").strong());
    ui.label(
        RichText::new(format!(
            "{} difference{}. A blank side means that file did not load an equivalent module.",
            relevant_count,
            if relevant_count == 1 { "" } else { "s" }
        ))
        .small()
        .color(ui.visuals().weak_text_color()),
    );
    ui.add_space(6.0);
    ui.columns(2, |columns| {
        module_side_panel(
            &mut columns[0],
            "FILE A ONLY",
            only_a,
            false,
            Color32::from_rgb(44, 92, 180),
        );
        module_side_panel(
            &mut columns[1],
            "FILE B ONLY",
            only_b,
            false,
            Color32::from_rgb(115, 65, 155),
        );
    });
    if system_count > 0 {
        ui.add_space(12.0);
        let system_a = only_a
            .iter()
            .filter(|module| is_system_module(module))
            .count();
        let system_b = only_b
            .iter()
            .filter(|module| is_system_module(module))
            .count();
        egui::Frame::new()
            .fill(Color32::from_rgb(249, 243, 226))
            .corner_radius(8)
            .inner_margin(16)
            .show(ui, |ui| {
                ui.label(
                    RichText::new("OS environment changed")
                        .size(19.0)
                        .strong(),
                );
                ui.label(format!(
                    "File A has {system_a} unique OS modules; File B has {system_b}. This usually indicates a different macOS environment, not the crash cause."
                ));
                ui.add_space(12.0);
                ui.columns(2, |columns| {
                    module_side_panel(
                        &mut columns[0],
                        "FILE A · UNIQUE OS MODULES",
                        only_a,
                        true,
                        Color32::from_rgb(44, 92, 180),
                    );
                    module_side_panel(
                        &mut columns[1],
                        "FILE B · UNIQUE OS MODULES",
                        only_b,
                        true,
                        Color32::from_rgb(115, 65, 155),
                    );
                });
            });
    }
}

fn module_side_panel(
    ui: &mut egui::Ui,
    title: &str,
    modules: &[ModulePresence],
    show_system: bool,
    accent: Color32,
) {
    let mut rows = modules
        .iter()
        .filter(|module| is_system_module(module) == show_system)
        .collect::<Vec<_>>();
    rows.sort_by_key(|module| module.name.to_ascii_lowercase());
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .corner_radius(6)
        .inner_margin(16)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(
                RichText::new(format!("{title} ({})", rows.len()))
                    .size(17.0)
                    .strong()
                    .color(accent),
            );
            ui.add_space(4.0);
            if rows.is_empty() {
                ui.label(RichText::new("None").color(ui.visuals().weak_text_color()));
            }
            for module in rows {
                ui.label(RichText::new(&module.name).strong());
                ui.add(
                    egui::Label::new(
                        RichText::new(compact_path(&module.path))
                            .small()
                            .color(ui.visuals().weak_text_color()),
                    )
                    .truncate(),
                )
                .on_hover_text(&module.path);
                ui.add_space(10.0);
            }
        });
}

fn compact_path(path: &str) -> String {
    let components: Vec<_> = Path::new(path).components().rev().take(3).collect();
    components
        .into_iter()
        .rev()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn compact_path_label(ui: &mut egui::Ui, path: &str) {
    let compact = compact_path(path);
    ui.add(egui::Label::new(compact).truncate())
        .on_hover_text(path);
}

fn is_system_module(module: &ModulePresence) -> bool {
    module.ownership == ModuleOwnership::System
}

fn comparison_card(ui: &mut egui::Ui, label: &str, report: &DumpReport, accent: Color32) {
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .inner_margin(18)
        .corner_radius(8)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(
                RichText::new(format!("FILE {label}"))
                    .size(19.0)
                    .strong()
                    .color(accent),
            );
            ui.label(
                RichText::new(
                    report
                        .path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy(),
                )
                .size(18.0)
                .strong(),
            );
            ui.label(
                RichText::new(report.path.display().to_string())
                    .small()
                    .monospace()
                    .color(ui.visuals().weak_text_color()),
            );
            ui.label(RichText::new(investigation::crash_signature(report)).monospace());
        });
}

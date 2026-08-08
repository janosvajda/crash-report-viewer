use crate::{
    domain::{DumpReport, ModuleOwnership, ModuleRow},
    services::analyzer,
    ui::widgets::{filter, pane_heading, section_title, selection_row, value_or_unknown},
};
use eframe::egui::{self, RichText};
use egui_extras::{Column, TableBuilder};

pub enum ModuleAction {
    OpenFrame { thread: usize, frame: usize },
    ConfigureSymbols,
}

pub fn modules(
    ui: &mut egui::Ui,
    report: &DumpReport,
    query: &mut String,
    selected: &mut usize,
) -> Option<ModuleAction> {
    section_title(
        ui,
        "Module investigation",
        "Connect loaded code to the fault and to every stack frame that references it.",
    );
    filter(ui, query, report.modules.len());
    if report.modules.is_empty() {
        ui.label("This dump contains no module list.");
        return None;
    }
    *selected = (*selected).min(report.modules.len() - 1);
    let normalized = query.to_ascii_lowercase();
    let visible: Vec<usize> = report
        .modules
        .iter()
        .enumerate()
        .filter(|(_, module)| {
            normalized.is_empty() || module.name.to_ascii_lowercase().contains(&normalized)
        })
        .map(|(index, _)| index)
        .collect();
    let mut action = None;
    ui.columns(2, |columns| {
        pane_heading(
            &mut columns[0],
            "Loaded code",
            "Select a module to see why it matters",
        );
        TableBuilder::new(&mut columns[0])
            .striped(true)
            .column(Column::remainder().at_least(180.0))
            .column(Column::initial(120.0))
            .header(26.0, |mut header| {
                header.col(|ui| {
                    ui.strong("MODULE");
                });
                header.col(|ui| {
                    ui.strong("RELEVANCE");
                });
            })
            .body(|body| {
                body.rows(34.0, visible.len(), |mut row| {
                    let index = visible[row.index()];
                    let module = &report.modules[index];
                    row.col(|ui| {
                        if selection_row(ui, *selected == index, &module.name, 30.0).clicked() {
                            *selected = index;
                        }
                    });
                    row.col(|ui| {
                        if module.contains_fault {
                            ui.label(
                                RichText::new("FAULT")
                                    .strong()
                                    .color(ui.visuals().error_fg_color),
                            );
                        } else {
                            ui.label(module.ownership.to_string());
                        }
                    });
                })
            });

        let module = &report.modules[*selected];
        let references = related_frames(report, module);
        pane_heading(&mut columns[1], &module.name, "Relationship to this crash");
        egui::Frame::new()
            .fill(columns[1].visuals().faint_bg_color)
            .inner_margin(10)
            .show(&mut columns[1], |ui| {
                if module.contains_fault {
                    ui.label(
                        RichText::new("Contains the fault address")
                            .strong()
                            .color(ui.visuals().error_fg_color),
                    );
                } else if references.is_empty() {
                    ui.label(
                        RichText::new("Loaded, but not referenced by recovered stacks").strong(),
                    );
                } else {
                    ui.label(
                        RichText::new(format!(
                            "Referenced by {} recovered frame{}",
                            references.len(),
                            if references.len() == 1 { "" } else { "s" }
                        ))
                        .strong(),
                    );
                }
                ui.label(recommendation(module, references.len()));
            });
        columns[1].add_space(10.0);
        egui::Grid::new("module_details")
            .num_columns(2)
            .striped(true)
            .show(&mut columns[1], |ui| {
                property(ui, "Ownership", module.ownership);
                property(ui, "Symbols", module.symbol_status);
                property(ui, "Base address", module.base);
                property(ui, "Image size", analyzer::human_bytes(module.size));
                property(ui, "Code identifier", value_or_unknown(&module.code_id));
            });
        if module.symbol_status.is_missing() && columns[1].button("Configure symbols →").clicked()
        {
            action = Some(ModuleAction::ConfigureSymbols);
        }
        columns[1].add_space(12.0);
        columns[1].label(RichText::new("Related stack frames").size(16.0).strong());
        if references.is_empty() {
            columns[1].label(
                RichText::new("No recovered frame points into this module.")
                    .color(columns[1].visuals().weak_text_color()),
            );
        }
        egui::ScrollArea::vertical()
            .id_salt("module_frames")
            .show(&mut columns[1], |ui| {
                for (thread_index, frame_index) in references {
                    let thread = &report.threads[thread_index];
                    let frame = &thread.frames[frame_index];
                    let name = if frame.function.is_empty() {
                        frame.instruction.to_string()
                    } else {
                        frame.function.clone()
                    };
                    if ui
                        .button(format!(
                            "Thread {} · frame {} · {}  →",
                            thread.id, frame.index, name
                        ))
                        .clicked()
                    {
                        action = Some(ModuleAction::OpenFrame {
                            thread: thread_index,
                            frame: frame_index,
                        });
                    }
                }
            });
    });
    action
}

fn related_frames(report: &DumpReport, module: &ModuleRow) -> Vec<(usize, usize)> {
    report
        .threads
        .iter()
        .enumerate()
        .flat_map(|(thread_index, thread)| {
            thread
                .frames
                .iter()
                .enumerate()
                .filter_map(move |(frame_index, frame)| {
                    (frame.module == module.name || module.name.ends_with(&frame.module))
                        .then_some((thread_index, frame_index))
                })
        })
        .collect()
}

fn recommendation(module: &ModuleRow, references: usize) -> &'static str {
    if module.contains_fault && module.symbol_status.is_missing() {
        "Highest priority: load matching symbols, then inspect the faulting frame."
    } else if module.contains_fault {
        "High priority: inspect related frames and source around the fault address."
    } else if references > 0 && module.ownership == ModuleOwnership::ApplicationOrThirdParty {
        "Application code on a recovered stack; inspect callers near the crashing frame."
    } else {
        "Low direct relevance unless it appears in another diagnostic or stack."
    }
}

fn property(ui: &mut egui::Ui, label: &str, value: impl std::fmt::Display) {
    ui.label(RichText::new(label).color(ui.visuals().weak_text_color()));
    ui.label(RichText::new(value.to_string()).monospace());
    ui.end_row();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{FrameRow, ThreadRow};

    #[test]
    fn finds_frames_that_reference_a_module() {
        let module = ModuleRow {
            name: "app.dll".into(),
            base: String::new().into(),
            size: 0,
            code_id: String::new(),
            symbol_status: crate::domain::SymbolStatus::Loaded,
            ownership: ModuleOwnership::ApplicationOrThirdParty,
            contains_fault: false,
        };
        let report = DumpReport {
            threads: vec![ThreadRow {
                id: 1,
                name: String::new(),
                stack_start: String::new().into(),
                stack_size: 0,
                crashed: true,
                frames: vec![FrameRow {
                    module: "app.dll".into(),
                    ..Default::default()
                }],
            }],
            modules: vec![module.clone()],
            ..Default::default()
        };
        assert_eq!(related_frames(&report, &module), vec![(0, 0)]);
    }
}

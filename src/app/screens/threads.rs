//! Virtualised thread and stack-frame investigation screen.

use super::ScreenAction;
use crate::{
    domain::{DumpReport, ModuleOwnership},
    services::{analyzer as dump, investigation},
    ui::widgets::{evidence, filter, pane_heading, section_title, selection_row, value_or_unknown},
};
use eframe::egui::{self, RichText};

pub fn threads(
    ui: &mut egui::Ui,
    r: &DumpReport,
    query: &mut String,
    selected_thread: &mut usize,
    selected_frame: &mut usize,
    source_root_from: &str,
    source_root_to: &str,
) -> Option<ScreenAction> {
    let mut module_request = None;
    section_title(
        ui,
        "Thread investigation",
        "Select a thread and stack frame to inspect the recovered crash path.",
    );
    filter(ui, query, r.threads.len());
    if r.threads.is_empty() {
        ui.label("This dump does not contain a thread list.");
        return None;
    }
    *selected_thread = (*selected_thread).min(r.threads.len() - 1);
    let normalized_query = query.to_ascii_lowercase();
    let visible_threads: Vec<_> = r
        .threads
        .iter()
        .enumerate()
        .filter(|(_, thread)| {
            normalized_query.is_empty()
                || thread.id.to_string().contains(&normalized_query)
                || thread.name.to_ascii_lowercase().contains(&normalized_query)
        })
        .collect();
    let available_height = ui.available_height().max(320.0);
    ui.columns(3, |columns| {
        columns[0].set_min_height(available_height);
        pane_heading(&mut columns[0], "Threads", "Captured execution contexts");
        egui::ScrollArea::vertical()
            .id_salt("thread_list")
            .show_rows(&mut columns[0], 58.0, visible_threads.len(), |ui, range| {
                for visible_index in range {
                    let (index, thread) = visible_threads[visible_index];
                    let title = if thread.name.is_empty() {
                        format!("Thread {}", thread.id)
                    } else {
                        thread.name.clone()
                    };
                    let caption = format!(
                        "{} · {} frame{} · {} stack at {}",
                        if thread.crashed {
                            "CRASHED"
                        } else {
                            "Captured"
                        },
                        thread.frames.len(),
                        if thread.frames.len() == 1 { "" } else { "s" },
                        dump::human_bytes(thread.stack_size),
                        thread.stack_start,
                    );
                    let width = ui.available_width();
                    let response = selection_row(ui, *selected_thread == index, title, 28.0);
                    ui.add_sized(
                        [width, 20.0],
                        egui::Label::new(RichText::new(caption).small().color(if thread.crashed {
                            ui.visuals().error_fg_color
                        } else {
                            ui.visuals().weak_text_color()
                        }))
                        .truncate(),
                    );
                    ui.separator();
                    if response.clicked() {
                        *selected_thread = index;
                        *selected_frame = 0;
                    }
                }
            });

        let thread = &r.threads[*selected_thread];
        let missing_symbols = thread
            .frames
            .iter()
            .filter(|frame| frame.missing_symbols)
            .count();
        let application_frames = thread
            .frames
            .iter()
            .filter(|frame| {
                r.modules.iter().any(|module| {
                    module.ownership == ModuleOwnership::ApplicationOrThirdParty
                        && (module.name == frame.module || module.name.ends_with(&frame.module))
                })
            })
            .count();
        pane_heading(
            &mut columns[1],
            "Call stack",
            &format!(
                "Thread {} · {} recovered frames",
                thread.id,
                thread.frames.len()
            ),
        );
        egui::ScrollArea::vertical()
            .id_salt("frame_list")
            .show_rows(&mut columns[1], 52.0, thread.frames.len(), |ui, range| {
                if thread.frames.is_empty() {
                    ui.label(
                        RichText::new("No stack frames could be recovered.")
                            .color(ui.visuals().weak_text_color()),
                    );
                }
                for index in range {
                    let frame = &thread.frames[index];
                    let name = if !frame.function.is_empty() {
                        frame.function.clone()
                    } else if !frame.module.is_empty() {
                        frame.module.clone()
                    } else {
                        frame.instruction.to_string()
                    };
                    let width = ui.available_width();
                    let response = selection_row(
                        ui,
                        *selected_frame == index,
                        format!("{}  {name}", frame.index),
                        27.0,
                    );
                    let location = if !frame.source_file.is_empty() {
                        format!(
                            "{}:{}",
                            frame.source_file,
                            frame.source_line.unwrap_or_default()
                        )
                    } else {
                        format!("{} {}", frame.module, frame.instruction)
                    };
                    ui.add_sized(
                        [width, 18.0],
                        egui::Label::new(
                            RichText::new(location)
                                .small()
                                .color(ui.visuals().weak_text_color()),
                        )
                        .truncate(),
                    );
                    ui.separator();
                    if response.clicked() {
                        *selected_frame = index;
                    }
                }
            });

        pane_heading(
            &mut columns[2],
            "Investigation",
            "Why this thread and frame matter",
        );
        egui::Frame::new()
            .fill(columns[2].visuals().faint_bg_color)
            .inner_margin(9)
            .show(&mut columns[2], |ui| {
                ui.label(
                    RichText::new(if thread.crashed {
                        "CRASHING THREAD"
                    } else {
                        "NON-CRASHING THREAD"
                    })
                    .small()
                    .strong()
                    .color(if thread.crashed {
                        ui.visuals().error_fg_color
                    } else {
                        ui.visuals().weak_text_color()
                    }),
                );
                ui.label(format!(
                    "{} application frame{} · {} frame{} missing symbols",
                    application_frames,
                    if application_frames == 1 { "" } else { "s" },
                    missing_symbols,
                    if missing_symbols == 1 { "" } else { "s" }
                ));
                ui.label(
                    RichText::new(if thread.crashed {
                        "Start at frame 0, then follow application-code callers."
                    } else {
                        "Use this thread to check concurrent work, locks, or related code paths."
                    })
                    .small()
                    .color(ui.visuals().weak_text_color()),
                );
            });
        columns[2].add_space(10.0);
        if let Some(frame) = thread.frames.get(*selected_frame) {
            if !frame.module.is_empty() && columns[2].button("Investigate this module →").clicked()
            {
                module_request = Some(ScreenAction::OpenModule(frame.module.clone()));
            }
            egui::Grid::new("frame_details")
                .num_columns(1)
                .spacing([8.0, 8.0])
                .show(&mut columns[2], |ui| {
                    evidence(ui, "Function", value_or_unknown(&frame.function));
                    evidence(ui, "Module", value_or_unknown(&frame.module));
                    evidence(ui, "Instruction", &frame.instruction.to_string());
                    evidence(
                        ui,
                        "Function offset",
                        value_or_unknown(&frame.function_offset),
                    );
                    evidence(ui, "Source file", value_or_unknown(&frame.source_file));
                    evidence(
                        ui,
                        "Source line",
                        &frame
                            .source_line
                            .map(|line| line.to_string())
                            .unwrap_or_else(|| "Not available".into()),
                    );
                    evidence(ui, "Unwind confidence", value_or_unknown(&frame.trust));
                    evidence(
                        ui,
                        "Symbols",
                        if frame.missing_symbols {
                            "Missing"
                        } else {
                            "Available / not required"
                        },
                    );
                });
            if !frame.registers.is_empty() {
                columns[2].add_space(14.0);
                columns[2].label(RichText::new("Registers").size(15.0).strong());
                egui::Grid::new("frame_registers")
                    .num_columns(2)
                    .striped(true)
                    .show(&mut columns[2], |ui| {
                        for (name, value) in &frame.registers {
                            ui.label(RichText::new(name).monospace().strong());
                            ui.label(RichText::new(value).monospace());
                            ui.end_row();
                        }
                    });
            }
            if !frame.source_file.is_empty() {
                let resolved_source = investigation::remap_source_path(
                    &frame.source_file,
                    source_root_from,
                    source_root_to,
                );
                columns[2].add_space(14.0);
                columns[2].label(RichText::new("Source context").size(15.0).strong());
                match investigation::source_excerpt(
                    &resolved_source,
                    frame.source_line.unwrap_or_default() as usize,
                    3,
                ) {
                    Ok(source) => {
                        egui::Frame::new()
                            .fill(columns[2].visuals().code_bg_color)
                            .inner_margin(8)
                            .show(&mut columns[2], |ui| {
                                ui.label(RichText::new(source).monospace());
                            });
                        if columns[2].button("Open source file").clicked() {
                            module_request = Some(ScreenAction::OpenPath(resolved_source));
                        }
                    }
                    Err(error) => {
                        columns[2].label(
                            RichText::new(format!("Source file is unavailable: {error}"))
                                .color(columns[2].visuals().weak_text_color()),
                        );
                    }
                }
            }
        } else {
            columns[2].label(
                RichText::new("Select a recovered frame to inspect it.")
                    .color(columns[2].visuals().weak_text_color()),
            );
        }
    });
    module_request
}

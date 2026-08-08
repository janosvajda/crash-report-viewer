mod analysis;
mod evidence;
mod map;
mod overview;
mod state;

use self::analysis::{region_references, region_relevance, stack_thread_for_region};
use self::map::memory_map_view;
pub use self::state::MemoryAnalysisCache;
use self::state::{BrowserDetail, MemoryMode};
use crate::{
    domain::DumpReport,
    services::{analyzer, memory as memory_analysis},
    ui::widgets::{pane_heading, section_title, selection_row},
};
use eframe::egui::{self, RichText};
use std::collections::BTreeMap;

pub struct MemoryAction {
    pub thread: usize,
    pub frame: usize,
}

pub fn memory(
    ui: &mut egui::Ui,
    report: &DumpReport,
    query: &mut String,
    selected_memory: &mut Option<usize>,
    cache: &mut MemoryAnalysisCache,
) -> Option<MemoryAction> {
    let mut action = None;
    section_title(
        ui,
        "Crash memory",
        "Follow the crash from register values to captured stacks, modules, strings, and pointers.",
    );
    ui.horizontal(|ui| {
        for (mode, label) in [
            (MemoryMode::Overview, "Overview"),
            (MemoryMode::Evidence, "Crash evidence"),
            (MemoryMode::Browser, "Memory browser"),
            (MemoryMode::Map, "Memory map"),
        ] {
            if ui
                .add_sized(
                    [170.0, 38.0],
                    egui::Button::selectable(cache.mode == mode, label),
                )
                .clicked()
            {
                cache.mode = mode;
            }
        }
    });
    ui.add_space(18.0);
    if cache.mode == MemoryMode::Map {
        return memory_map_view(ui, report, cache);
    }
    if cache.mode == MemoryMode::Overview {
        return overview::render(ui, report);
    }
    if cache.mode == MemoryMode::Evidence {
        return evidence::render(ui, report, selected_memory, cache);
    }
    pane_heading(
        ui,
        if cache.mode == MemoryMode::Browser {
            "All captured memory"
        } else {
            "Crash-linked memory"
        },
        if cache.mode == MemoryMode::Browser {
            "Search and inspect the complete memory content stored in this dump"
        } else {
            "Only memory directly connected to the fault or crashed thread"
        },
    );
    if cache.mode == MemoryMode::Browser {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Find region").strong());
            ui.add_sized(
                [360.0, 30.0],
                egui::TextEdit::singleline(query)
                    .hint_text("Search all captured memory by address or text"),
            );
            ui.label(
                RichText::new(format!("{} regions", report.memory.len()))
                    .small()
                    .color(ui.visuals().weak_text_color()),
            );
        });
    }
    if cache.refresh_filter(report, query, cache.mode == MemoryMode::Browser) {
        *selected_memory = None;
    }
    let regions: Vec<_> = cache
        .matches
        .iter()
        .map(|&index| &report.memory[index])
        .collect();
    if regions.is_empty() {
        ui.label("No captured memory regions match this filter.");
        return action;
    }
    let selected_index = selected_memory.map_or_else(
        || {
            regions
                .iter()
                .position(|region| {
                    memory_analysis::contains(
                        region,
                        memory_analysis::parse_address(&report.crash_address),
                    )
                })
                .or_else(|| {
                    regions
                        .iter()
                        .position(|region| !region_references(report, region).is_empty())
                })
                .unwrap_or(0)
        },
        |index| index.min(regions.len() - 1),
    );
    *selected_memory = Some(selected_index);
    ui.columns(1, |columns| {
        let show_selector = cache.mode == MemoryMode::Browser || regions.len() > 1;
        if show_selector {
        pane_heading(
            &mut columns[0],
            "Regions",
            "Crash-related regions are labelled; other captured regions remain searchable",
        );
        egui::ScrollArea::vertical()
            .id_salt("memory_regions")
            .max_height(230.0)
            .show_rows(&mut columns[0], 40.0, regions.len(), |ui, range| {
                for index in range {
                    let region = regions[index];
                    let contains_fault = memory_analysis::contains(
                        region,
                        memory_analysis::parse_address(&report.crash_address),
                    );
                    let reference_count = region_references(report, region).len();
                    let roles = memory_analysis::region_roles(report, region);
                    let purpose = if contains_fault {
                        "FAULT ADDRESS".into()
                    } else if roles.iter().any(|role| role.contains("(crashed)")) {
                        "CRASHED THREAD STACK".into()
                    } else if let Some(role) = roles.first() {
                        role.clone()
                    } else if reference_count > 0 {
                        format!("REFERENCED BY {reference_count} FRAME VALUES")
                    } else {
                        "OTHER CAPTURED MEMORY".into()
                    };
                    let label = format!(
                        "{}   ·   {}   ·   {}",
                        purpose,
                        analyzer::human_bytes(region.size),
                        region.start
                    );
                    if selection_row(ui, selected_index == index, label, 34.0).clicked() {
                        *selected_memory = Some(index);
                    }
                }
            });
        }

        let region = regions[selected_index];
        let references = region_references(report, region);
        let roles = memory_analysis::region_roles(report, region);
        cache.refresh_region(report, region);
        let pointers = &cache.pointers;
        let strings = &cache.strings;
        let (relevance, conclusion, explanation) =
            region_relevance(report, region, references.len(), &roles);
        let stack_thread = stack_thread_for_region(report, region);
        if show_selector {
            columns[0].add_space(22.0);
            columns[0].separator();
            columns[0].add_space(12.0);
        }
        columns[0].horizontal(|ui| {
            for (detail, label) in [
                (BrowserDetail::Connection, "Crash connection"),
                (BrowserDetail::Pointers, "Pointers"),
                (BrowserDetail::Text, "Text"),
                (BrowserDetail::Bytes, "Raw bytes"),
            ] {
                if ui
                    .add_sized(
                        [160.0, 34.0],
                        egui::Button::selectable(cache.browser_detail == detail, label),
                    )
                    .clicked()
                {
                    cache.browser_detail = detail;
                }
            }
        });
        columns[0].add_space(14.0);
        if cache.browser_detail == BrowserDetail::Connection {
        pane_heading(
            &mut columns[0],
            "Crash connection",
            &format!("{} bytes captured from {}", region.size, region.start),
        );
        egui::Frame::new()
            .fill(columns[0].visuals().faint_bg_color)
            .inner_margin(9)
            .show(&mut columns[0], |ui| {
                ui.label(RichText::new("INVESTIGATION VALUE").small().strong());
                ui.label(
                    RichText::new(format!("{relevance} · {conclusion}"))
                        .size(17.0)
                        .strong()
                        .color(if relevance == "HIGH" {
                            ui.visuals().error_fg_color
                        } else {
                            ui.visuals().text_color()
                        }),
                );
                ui.label(explanation);
                if cache.mode == MemoryMode::Browser
                    && let Some((thread_index, thread_id, crashed)) = stack_thread
                    && ui
                        .button(if crashed {
                            format!("Open crashed thread {thread_id}  →")
                        } else {
                            format!("Open thread {thread_id}  →")
                        })
                        .clicked()
                {
                    action = Some(MemoryAction {
                        thread: thread_index,
                        frame: 0,
                    });
                }
                ui.add_space(6.0);
                ui.label(format!(
                    "{} · {} · permissions {} · mapping {}",
                    region.start,
                    analyzer::human_bytes(region.size),
                    region.permissions,
                    region.mapping_type,
                ));
            });
        columns[0].add_space(12.0);
        for reference in &references {
            if columns[0]
                .button(format!("{}  →", reference.description))
                .clicked()
            {
                action = Some(MemoryAction {
                    thread: reference.thread,
                    frame: reference.frame,
                });
            }
        }
        }
        if cache.browser_detail == BrowserDetail::Connection {
            return;
        }
        let mut pointer_destinations =
            BTreeMap::<&memory_analysis::AddressTarget, usize>::new();
        for pointer in pointers {
            *pointer_destinations.entry(&pointer.target).or_default() += 1;
        }
        if cache.browser_detail == BrowserDetail::Pointers {
        columns[0].label(
            RichText::new(format!(
                "Where this region points ({})",
                pointer_destinations.len()
            ))
                .size(17.0)
                .strong(),
        );
        columns[0].label(
            RichText::new("Destinations found inside this region, grouped by stack, module, or captured memory area.")
                .small()
                .color(columns[0].visuals().weak_text_color()),
        );
        egui::Frame::new()
            .fill(columns[0].visuals().faint_bg_color)
            .inner_margin(12)
            .show(&mut columns[0], |ui| {
                if pointer_destinations.is_empty() {
                    ui.label("No links to another captured stack, module, or memory area.");
                }
                for (target, count) in &pointer_destinations {
                    ui.label(
                        RichText::new(format!("{count} pointer{} → {target}", if *count == 1 { "" } else { "s" }))
                            .strong(),
                    );
                }
            });
        }
        if cache.browser_detail == BrowserDetail::Text {
        columns[0].add_space(16.0);
        columns[0].label(
            RichText::new(format!("Text found in captured bytes ({})", strings.len()))
                .size(17.0)
                .strong(),
        );
        columns[0].label(
            RichText::new("Possible ASCII or UTF-16 text. This is supporting context, not proof of the crash cause, and may contain private data.")
                .small()
                .color(columns[0].visuals().weak_text_color()),
        );
        egui::Frame::new()
            .fill(columns[0].visuals().faint_bg_color)
            .inner_margin(12)
            .show(&mut columns[0], |ui| {
                if strings.is_empty() {
                    ui.label("No readable ASCII or UTF-16 strings of four or more characters.");
                }
                for string in strings {
                    ui.label(string);
                }
            });
        }
        if cache.browser_detail == BrowserDetail::Bytes {
        columns[0].add_space(14.0);
        egui::Frame::new()
            .fill(columns[0].visuals().code_bg_color)
            .inner_margin(12)
            .show(&mut columns[0], |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("ADDRESS").small().strong());
                        ui.label(RichText::new("HEX BYTES").small().strong());
                        ui.label(RichText::new("ASCII").small().strong());
                    });
                    ui.separator();
                    egui::ScrollArea::both().max_height(220.0).show(ui, |ui| {
                        ui.label(RichText::new(&region.preview).monospace());
                    });
            });
        columns[0].add_space(8.0);
        columns[0].label(
            RichText::new(
                "Only bytes captured in the minidump are shown; this is not live process memory.",
            )
            .small()
            .color(columns[0].visuals().weak_text_color()),
        );
        }
    });
    action
}

fn evidence_node(ui: &mut egui::Ui, step: &str, title: &str, detail: impl ToString) {
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .corner_radius(7)
        .inner_margin(12)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new(step).small().strong());
            ui.label(RichText::new(title).strong());
            ui.label(
                RichText::new(detail.to_string())
                    .small()
                    .color(ui.visuals().weak_text_color()),
            );
        });
}

fn evidence_tile(
    ui: &mut egui::Ui,
    title: &str,
    value: &str,
    detail: &str,
    actionable: bool,
) -> bool {
    let mut clicked = false;
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .corner_radius(7)
        .inner_margin(12)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new(title).small().strong());
            ui.label(RichText::new(value).size(17.0).strong());
            ui.label(
                RichText::new(detail)
                    .small()
                    .color(ui.visuals().weak_text_color()),
            );
            if actionable {
                clicked = ui.button("Inspect  →").clicked();
            }
        });
    clicked
}

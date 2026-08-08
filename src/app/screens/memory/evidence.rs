//! Compact evidence path from the crashed frame to relevant captured memory.

use super::{
    BrowserDetail, MemoryAction, MemoryAnalysisCache, MemoryMode, evidence_tile, region_references,
    region_relevance,
};
use crate::{
    domain::DumpReport,
    services::{analyzer, memory as memory_analysis},
};
use eframe::egui::{self, RichText};

pub(super) fn render(
    ui: &mut egui::Ui,
    report: &DumpReport,
    selected_memory: &mut Option<usize>,
    cache: &mut MemoryAnalysisCache,
) -> Option<MemoryAction> {
    let mut action = None;
    let crashed = report
        .threads
        .iter()
        .enumerate()
        .find(|(_, thread)| thread.crashed);
    let crash_region_index = report.memory.iter().position(|region| {
        memory_analysis::contains(
            region,
            memory_analysis::parse_address(&report.crash_address),
        ) || memory_analysis::region_roles(report, region)
            .iter()
            .any(|role| role.contains("(crashed)"))
    });
    let crash_region = crash_region_index.map(|index| &report.memory[index]);
    let (priority, conclusion, explanation) = crash_region
            .map(|region| {
                let roles = memory_analysis::region_roles(report, region);
                let references = region_references(report, region);
                region_relevance(report, region, references.len(), &roles)
            })
            .unwrap_or((
                "UNKNOWN",
                "No crash-linked memory was captured",
                "Use the crashing stack and modules; this dump cannot support memory-level conclusions."
                    .into(),
            ));
    let register_findings = memory_analysis::register_findings(report);
    let suspicious = register_findings
        .iter()
        .filter(|finding| finding.suspicious)
        .collect::<Vec<_>>();

    egui::Frame::new()
        .fill(egui::Color32::from_rgb(250, 239, 222))
        .corner_radius(9)
        .inner_margin(16)
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("INVESTIGATION VALUE").small().strong());
                    ui.label(
                        RichText::new(format!("{priority} · {conclusion}"))
                            .size(18.0)
                            .strong(),
                    );
                    ui.label(explanation);
                    ui.add_space(8.0);
                    if let Some((_, thread)) = crashed {
                        let frame = thread.frames.first();
                        ui.label(RichText::new(format!("Crashed thread {}", thread.id)).strong());
                        ui.label(format!(
                            "Top frame: {}",
                            frame
                                .map(|frame| {
                                    format!(
                                        "{}!{}",
                                        frame.module,
                                        if frame.function.is_empty() {
                                            frame.instruction.to_string()
                                        } else {
                                            frame.function.clone()
                                        }
                                    )
                                })
                                .unwrap_or_else(|| "Not recovered".into())
                        ));
                    } else {
                        ui.label("Crashed thread was not identified.");
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                    if let Some((thread_index, thread)) = crashed
                        && ui
                            .add_sized(
                                [230.0, 38.0],
                                egui::Button::new(format!("Open crashed thread {}  →", thread.id)),
                            )
                            .clicked()
                    {
                        action = Some(MemoryAction {
                            thread: thread_index,
                            frame: 0,
                        });
                    }
                });
            });
            if !suspicious.is_empty() {
                ui.add_space(10.0);
                ui.separator();
                ui.label(RichText::new("SUSPICIOUS REGISTERS").small().strong());
                for finding in &suspicious {
                    ui.label(
                        RichText::new(format!(
                            "{} = {} → {}",
                            finding.register, finding.value, finding.target
                        ))
                        .strong()
                        .color(ui.visuals().error_fg_color),
                    );
                }
            }
        });
    if let Some(region) = crash_region {
        cache.refresh_region(report, region);
    }
    let pointer_destinations = cache
        .pointers
        .iter()
        .map(|pointer| &pointer.target)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    ui.add_space(14.0);
    ui.columns(4, |columns| {
        if evidence_tile(
            &mut columns[0],
            "Registers",
            &format!("{} recovered", register_findings.len()),
            &format!("{} suspicious", suspicious.len()),
            !register_findings.is_empty(),
        ) {
            cache.show_registers = !cache.show_registers;
        }
        if evidence_tile(
            &mut columns[1],
            "Crash-linked memory",
            crash_region
                .map(|region| analyzer::human_bytes(region.size))
                .as_deref()
                .unwrap_or("Not captured"),
            "Region conclusion and thread relationship",
            crash_region.is_some(),
        ) {
            cache.mode = MemoryMode::Browser;
            cache.browser_detail = BrowserDetail::Connection;
            *selected_memory = None;
        }
        if evidence_tile(
            &mut columns[2],
            "Pointer destinations",
            &pointer_destinations.to_string(),
            "Stacks, modules, or captured regions",
            pointer_destinations > 0,
        ) {
            cache.mode = MemoryMode::Browser;
            cache.browser_detail = BrowserDetail::Pointers;
            *selected_memory = None;
        }
        if evidence_tile(
            &mut columns[3],
            "Text clues",
            &cache.strings.len().to_string(),
            "ASCII or UTF-16 strings",
            !cache.strings.is_empty(),
        ) {
            cache.mode = MemoryMode::Browser;
            cache.browser_detail = BrowserDetail::Text;
            *selected_memory = None;
        }
    });
    if cache.show_registers && !register_findings.is_empty() {
        ui.add_space(12.0);
        egui::Frame::new()
            .fill(ui.visuals().faint_bg_color)
            .inner_margin(12)
            .show(ui, |ui| {
                ui.label(RichText::new("Recovered register targets").strong());
                for finding in &register_findings {
                    ui.label(format!(
                        "{} = {} → {}",
                        finding.register, finding.value, finding.target
                    ));
                }
            });
    }
    action
}

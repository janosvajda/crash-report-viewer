//! Small set of entry points into the most useful memory evidence.

use super::{MemoryAction, evidence_node};
use crate::{domain::DumpReport, services::memory as memory_analysis, ui::widgets::pane_heading};
use eframe::egui::{self, RichText};

pub(super) fn render(ui: &mut egui::Ui, report: &DumpReport) -> Option<MemoryAction> {
    let mut action = None;
    let crashed_thread = report.threads.iter().find(|thread| thread.crashed);
    let fault_captured = report.memory.iter().any(|region| {
        memory_analysis::contains(
            region,
            memory_analysis::parse_address(&report.crash_address),
        )
    });
    let crashed_stack_captured = crashed_thread.is_some_and(|thread| {
        report.memory.iter().any(|region| {
            memory_analysis::contains(region, memory_analysis::parse_address(&thread.stack_start))
        })
    });
    egui::Frame::new()
        .fill(if fault_captured || crashed_stack_captured {
            egui::Color32::from_rgb(232, 244, 235)
        } else {
            egui::Color32::from_rgb(250, 239, 222)
        })
        .corner_radius(9)
        .inner_margin(18)
        .show(ui, |ui| {
            ui.label(RichText::new("MEMORY CONCLUSION").small().strong());
            ui.label(
                RichText::new(if fault_captured {
                    "The fault address is present in captured memory"
                } else if crashed_stack_captured {
                    "The crashed thread stack is available; the fault address is not captured"
                } else {
                    "No captured region is directly linked to the crash"
                })
                .size(19.0)
                .strong(),
            );
            ui.label(if fault_captured {
                "Use the evidence chain below to move from the exception to the responsible thread and memory region."
            } else if crashed_stack_captured {
                "Stack arguments and return addresses can still help, but the failing target cannot be inspected directly."
            } else {
                "This dump does not contain enough related memory for memory-level diagnosis. Start with the crashing stack instead."
            });
        });
    ui.add_space(18.0);
    pane_heading(
        ui,
        "Evidence chain",
        "How the recorded crash connects to execution and captured memory",
    );
    ui.columns(4, |columns| {
        evidence_node(
            &mut columns[0],
            "1 · EXCEPTION",
            &report.crash_reason,
            report.crash_address,
        );
        evidence_node(
            &mut columns[1],
            "2 · CRASHED THREAD",
            &crashed_thread
                .map(|thread| format!("Thread {}", thread.id))
                .unwrap_or_else(|| "Not identified".into()),
            if crashed_thread.is_some() {
                "Recovered"
            } else {
                "Missing"
            },
        );
        let top_frame = crashed_thread.and_then(|thread| thread.frames.first());
        evidence_node(
            &mut columns[2],
            "3 · TOP FRAME",
            top_frame
                .map(|frame| frame.module.as_str())
                .unwrap_or("Not recovered"),
            top_frame
                .map(|frame| frame.function.as_str())
                .filter(|value| !value.is_empty())
                .unwrap_or("No function name"),
        );
        evidence_node(
            &mut columns[3],
            "4 · CAPTURED MEMORY",
            if fault_captured {
                "Fault region"
            } else if crashed_stack_captured {
                "Crashed stack"
            } else {
                "No direct region"
            },
            if fault_captured || crashed_stack_captured {
                "Available"
            } else {
                "Missing"
            },
        );
    });
    if let Some((thread_index, thread)) = report
        .threads
        .iter()
        .enumerate()
        .find(|(_, thread)| thread.crashed)
        && ui
            .button(format!("Open crashed thread {}  →", thread.id))
            .clicked()
    {
        action = Some(MemoryAction {
            thread: thread_index,
            frame: 0,
        });
    }
    ui.add_space(24.0);
    action
}

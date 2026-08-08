use crate::{
    domain::DumpReport,
    services::analyzer,
    ui::widgets::{filter, section_title},
};
use eframe::egui::{self, RichText};
use egui_extras::{Column, TableBuilder};

pub fn streams(ui: &mut egui::Ui, report: &DumpReport, query: &mut String) {
    section_title(
        ui,
        "Dump internals",
        "Advanced view of the sections physically stored inside this minidump.",
    );
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .corner_radius(6)
        .inner_margin(10)
        .show(ui, |ui| {
            ui.label(RichText::new("Most investigations do not need this screen.").strong());
            ui.label("Use it to verify that expected sections—such as exception, threads, modules, system information, or memory—exist in the file, or when diagnosing an incomplete/corrupt dump.");
        });
    ui.add_space(12.0);
    filter(ui, query, report.streams.len());
    let normalized_query = query.to_ascii_lowercase();
    let streams: Vec<_> = report
        .streams
        .iter()
        .filter(|stream| {
            normalized_query.is_empty()
                || stream.kind.to_ascii_lowercase().contains(&normalized_query)
        })
        .collect();
    TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .column(Column::remainder().at_least(220.0))
        .column(Column::remainder().at_least(240.0))
        .column(Column::initial(140.0))
        .column(Column::initial(100.0))
        .header(26.0, |mut header| {
            for title in ["SECTION", "WHY IT MATTERS", "FILE OFFSET", "SIZE"] {
                header.col(|ui| {
                    ui.label(RichText::new(title).small().strong());
                });
            }
        })
        .body(|body| {
            body.rows(26.0, streams.len(), |mut row| {
                let stream = streams[row.index()];
                row.col(|ui| {
                    ui.label(&stream.kind);
                });
                row.col(|ui| {
                    ui.label(stream_purpose(&stream.kind));
                });
                row.col(|ui| {
                    ui.monospace(format!("0x{:08x}", stream.rva));
                });
                row.col(|ui| {
                    ui.label(analyzer::human_bytes(stream.size as u64));
                });
            })
        });
}

fn stream_purpose(kind: &str) -> &'static str {
    let kind = kind.to_ascii_lowercase();
    if kind.contains("exception") {
        "Crash reason, fault address, and crashing thread"
    } else if kind.contains("thread") {
        "Thread contexts and stack memory"
    } else if kind.contains("module") {
        "Loaded executables and libraries"
    } else if kind.contains("system") {
        "Operating system and CPU architecture"
    } else if kind.contains("memory") {
        "Captured process memory regions"
    } else {
        "Additional producer-specific dump data"
    }
}

#[cfg(test)]
mod tests {
    use super::stream_purpose;

    #[test]
    fn explains_common_internal_streams() {
        assert!(stream_purpose("ExceptionStream").contains("fault address"));
        assert!(stream_purpose("ModuleListStream").contains("libraries"));
        assert!(stream_purpose("Memory64ListStream").contains("memory"));
    }
}

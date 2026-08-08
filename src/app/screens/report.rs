use super::super::session::InvestigationStatus;
use crate::{
    domain::DumpReport,
    services::{export as report_export, platform},
    ui::widgets::section_title,
};
use eframe::egui::{self, Align, Layout, RichText};
use std::path::PathBuf;

pub fn report_tools(
    ui: &mut egui::Ui,
    report: &DumpReport,
    notes: &mut String,
    status: &mut InvestigationStatus,
    result: &mut Option<Result<PathBuf, String>>,
    tags: &mut String,
) {
    section_title(
        ui,
        "Prepare investigation files",
        "Add context for other people, then choose exactly which file to create.",
    );
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .corner_radius(6)
        .inner_margin(10)
        .show(ui, |ui| {
            ui.label(RichText::new("This page does not change the crash analysis.").strong());
            ui.label(
                "Status, tags, and notes are annotations written into the reports you export below.",
            );
        });
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Report status").strong());
        egui::ComboBox::from_id_salt("investigation_status")
            .selected_text(status.to_string())
            .show_ui(ui, |ui| {
                for value in InvestigationStatus::ALL {
                    ui.selectable_value(status, value, value.to_string());
                }
            });
        ui.add_space(16.0);
        ui.label(RichText::new("Report tags").strong());
        ui.add_sized(
            [320.0, 30.0],
            egui::TextEdit::singleline(tags)
                .hint_text("regression, startup, renderer, release-1.4"),
        );
    });
    ui.add_space(12.0);
    ui.label(RichText::new("Share or archive").size(17.0).strong());
    ui.label(
        RichText::new(
            "Choose exactly where each export is saved. The original dump is never modified.",
        )
        .color(ui.visuals().weak_text_color()),
    );
    ui.add_space(8.0);
    if export_action(
        ui,
        "Readable report",
        "Creates <dump>-report.md with crash facts, stacks, status, tags, and notes.",
        "Create report.md",
    ) && let Some(path) = choose_export_file(report, "-report.md", "Markdown", "md")
    {
        *result = Some(
            report_export::export_markdown_to(
                &path,
                report,
                notes,
                &status.to_string(),
                tags,
                false,
            )
            .map_err(|error| format!("{error:#}")),
        );
    }
    if export_action(
        ui,
        "Privacy-safe report",
        "Creates <dump>-sanitized.md; parent directories are removed before sharing.",
        "Create sanitized.md",
    ) && let Some(path) = choose_export_file(report, "-sanitized.md", "Markdown", "md")
    {
        *result = Some(
            report_export::export_markdown_to(
                &path,
                report,
                notes,
                &status.to_string(),
                tags,
                true,
            )
            .map_err(|error| format!("{error:#}")),
        );
    }
    if export_action(
        ui,
        "Investigation bundle",
        "Creates a folder with README.md, report.md, stack.txt, and analysis.json.",
        "Create bundle folder",
    ) && let Some(directory) = choose_bundle_directory(report)
    {
        *result = Some(
            report_export::export_bundle_to(&directory, report, notes, &status.to_string(), tags)
                .map_err(|error| format!("{error:#}")),
        );
    }
    ui.add_space(14.0);
    ui.label(
        RichText::new("Optional investigation notes")
            .size(16.0)
            .strong(),
    );
    ui.label(
        RichText::new("Included in reports and bundles; leave empty if you only want crash data.")
            .small()
            .color(ui.visuals().weak_text_color()),
    );
    ui.add_sized(
        [ui.available_width(), 72.0],
        egui::TextEdit::multiline(notes)
            .hint_text("Suspected cause, reproduction details, and next action…"),
    );
    ui.add_space(14.0);
    ui.label(RichText::new("Developer formats").size(16.0).strong());
    if export_action(
        ui,
        "Processor JSON",
        "Creates <dump>-analysis.json for scripts, automation, or another analysis tool.",
        "Create analysis.json",
    ) && let Some(path) = choose_export_file(report, "-analysis.json", "JSON", "json")
    {
        *result = Some(
            report_export::export_json_to(&path, report).map_err(|error| format!("{error:#}")),
        );
    }
    if export_action(
        ui,
        "Plain-text stack",
        "Creates <dump>-stack.txt containing only threads and recovered frames.",
        "Create stack.txt",
    ) && let Some(path) = choose_export_file(report, "-stack.txt", "Text", "txt")
    {
        *result = Some(
            report_export::export_stack_to(&path, report).map_err(|error| format!("{error:#}")),
        );
    }
    if let Some(result) = result {
        ui.add_space(20.0);
        let failed = result.is_err();
        egui::Frame::new()
            .fill(if failed {
                ui.visuals().error_fg_color.gamma_multiply(0.08)
            } else {
                ui.visuals().selection.bg_fill.gamma_multiply(0.35)
            })
            .inner_margin(12)
            .show(ui, |ui| {
                ui.label(
                    RichText::new(if failed {
                        "Export failed"
                    } else {
                        "Export complete"
                    })
                    .strong(),
                );
                ui.label(match result {
                    Ok(path) => format!("Saved {}", path.display()),
                    Err(error) => error.clone(),
                });
                let exported_folder = result.as_ref().ok().and_then(|path| {
                    if path.is_dir() {
                        Some(path.as_path())
                    } else {
                        path.parent()
                    }
                });
                if let Some(folder) = exported_folder
                    && ui.button("Show files in Finder").clicked()
                    && let Err(error) = platform::open_path(folder)
                {
                    *result = Err(format!("{error:#}"));
                }
            });
    }
}

fn export_action(ui: &mut egui::Ui, title: &str, description: &str, button: &str) -> bool {
    let mut clicked = false;
    let available = ui.available_width();
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .corner_radius(6)
        .inner_margin(10)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2((available - 220.0).max(180.0), 48.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.label(RichText::new(title).strong());
                        ui.label(
                            RichText::new(description)
                                .small()
                                .color(ui.visuals().weak_text_color()),
                        );
                    },
                );
                clicked = ui
                    .add_sized([190.0, 34.0], egui::Button::new(button))
                    .clicked();
            });
        });
    ui.add_space(6.0);
    clicked
}

fn choose_export_file(
    report: &DumpReport,
    suffix: &str,
    filter_name: &str,
    extension: &str,
) -> Option<PathBuf> {
    let stem = report.path.file_stem()?.to_string_lossy();
    let filename = format!("{stem}{suffix}");
    let mut dialog = rfd::FileDialog::new()
        .set_file_name(filename)
        .add_filter(filter_name, &[extension]);
    if let Some(parent) = report.path.parent() {
        dialog = dialog.set_directory(parent);
    }
    dialog.save_file()
}

fn choose_bundle_directory(report: &DumpReport) -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new();
    if let Some(parent) = report.path.parent() {
        dialog = dialog.set_directory(parent);
    }
    let parent = dialog.pick_folder()?;
    let stem = report.path.file_stem()?.to_string_lossy();
    Some(parent.join(format!("{stem}-investigation")))
}

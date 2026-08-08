use super::ScreenAction;
use crate::{domain::DumpReport, ui::widgets::section_title};
use eframe::egui::{self, FontId, RichText};
use egui_extras::{Column, TableBuilder};

pub fn symbol_settings(
    ui: &mut egui::Ui,
    local_paths: &mut String,
    server_urls: &mut String,
    report: &DumpReport,
    source_root_from: &mut String,
    source_root_to: &mut String,
) -> Option<ScreenAction> {
    section_title(
        ui,
        "Debug symbols",
        "Turn raw addresses into readable function names, source files, and line numbers.",
    );
    let missing = report
        .modules
        .iter()
        .filter(|module| module.symbol_status.is_missing())
        .count();
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .corner_radius(6)
        .inner_margin(10)
        .show(ui, |ui| {
            ui.label(RichText::new("When should I use this?").strong());
            ui.label(if missing > 0 {
                format!("{missing} module{} report missing symbols. Add the matching symbol files if stack frames show addresses instead of useful function names.", if missing == 1 { "" } else { "s" })
            } else {
                "No module currently reports missing symbols. You usually do not need to change anything here.".into()
            });
            ui.label(RichText::new("Applying symbols re-runs analysis; it does not modify the dump.").small().color(ui.visuals().weak_text_color()));
        });
    ui.add_space(14.0);
    ui.label(RichText::new("1. Local symbol folders").strong());
    ui.label(
        RichText::new("Folders containing Breakpad .sym files produced when the application was built. One folder per line.")
            .small()
            .color(ui.visuals().weak_text_color()),
    );
    ui.add_sized(
        [ui.available_width(), 90.0],
        egui::TextEdit::multiline(local_paths)
            .font(FontId::monospace(13.0))
            .hint_text("~/symbols\n/Volumes/build/debug-symbols"),
    );
    ui.add_space(12.0);
    ui.label(RichText::new("2. Symbol download servers").strong());
    ui.label(
        RichText::new(
            "Optional servers that host matching symbols. Downloaded files are cached under ~/Library/Caches/CrashLens.",
        )
        .small()
        .color(ui.visuals().weak_text_color()),
    );
    ui.add_sized(
        [ui.available_width(), 90.0],
        egui::TextEdit::multiline(server_urls)
            .font(FontId::monospace(13.0))
            .hint_text("https://symbols.mozilla.org/"),
    );
    ui.add_space(10.0);
    ui.label(RichText::new("3. Local source-code location (optional)").strong());
    ui.label(
        RichText::new("Symbols may contain a build-machine path. Map that recorded path to your local checkout so CrashLens can show source lines.")
            .small()
            .color(ui.visuals().weak_text_color()),
    );
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(source_root_from)
                .font(FontId::monospace(13.0))
                .hint_text("Recorded root, e.g. C:/build/project"),
        );
        ui.label("→");
        ui.add(
            egui::TextEdit::singleline(source_root_to)
                .font(FontId::monospace(13.0))
                .hint_text("Local root, e.g. ~/projects/app"),
        );
    });
    ui.add_space(10.0);
    let action = ui
        .button("Reanalyze using these symbols")
        .clicked()
        .then_some(ScreenAction::ReanalyseWithSymbols);
    ui.add_space(18.0);
    ui.label(
        RichText::new("Symbol availability by module")
            .size(17.0)
            .strong(),
    );
    TableBuilder::new(ui)
        .striped(true)
        .column(Column::remainder())
        .column(Column::initial(190.0))
        .header(26.0, |mut header| {
            header.col(|ui| {
                ui.strong("MODULE");
            });
            header.col(|ui| {
                ui.strong("STATUS");
            });
        })
        .body(|body| {
            body.rows(25.0, report.modules.len(), |mut table_row| {
                let module = &report.modules[table_row.index()];
                table_row.col(|ui| {
                    ui.label(&module.name);
                });
                table_row.col(|ui| {
                    ui.label(module.symbol_status.to_string());
                });
            });
        });
    action
}

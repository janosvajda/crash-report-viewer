use self::state::{AnalysisJob, LibraryState, set_comparison_selected, take_comparison_pair};
use crate::{
    domain::{DumpReport, SymbolConfig},
    services::{
        analyzer as dump,
        scanner::{self as discovery, FileEntry, ScanEvent},
    },
    ui::{
        theme,
        widgets::{header_row, selection_row},
    },
};
mod screens;
mod session;
mod state;
use self::session::{AnalysisViewState, ComparisonState, InvestigationState};
use eframe::egui::{self, Align, Color32, FontId, Layout, RichText};
use screens::{
    MemoryAction, ModuleAction, ScreenAction, compare_view, memory, modules, report_tools,
    search as global_search, streams, summary, symbol_settings, threads,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Page {
    Summary,
    Threads,
    Modules,
    Memory,
    Streams,
    Symbols,
    Report,
    Compare,
    Search,
}

pub struct CrashLens {
    report: Option<Arc<DumpReport>>,
    page: Page,
    error: Option<String>,
    directory: String,
    library: LibraryState,
    scan: Option<Receiver<ScanEvent>>,
    view: AnalysisViewState,
    investigation: InvestigationState,
    comparison: ComparisonState,
    crash_history: Vec<Arc<DumpReport>>,
    analysis_job: Option<AnalysisJob>,
}

impl CrashLens {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);
        Self {
            report: None,
            page: Page::Summary,
            error: None,
            directory: std::env::var("HOME").unwrap_or_else(|_| "/".into()),
            library: LibraryState::default(),
            scan: None,
            view: AnalysisViewState::default(),
            investigation: InvestigationState::default(),
            comparison: ComparisonState::default(),
            crash_history: Vec::new(),
            analysis_job: None,
        }
    }

    fn open(&mut self, path: PathBuf) {
        let symbols = self.symbol_config();
        self.analysis_job = Some(AnalysisJob::spawn(path, symbols));
        self.error = None;
    }

    fn receive_analysis(&mut self, ctx: &egui::Context) {
        let result = self.analysis_job.as_ref().and_then(AnalysisJob::poll);
        if let Some(result) = result {
            self.analysis_job = None;
            match result {
                Ok(report) => {
                    self.view.reset_for(&report);
                    self.crash_history
                        .retain(|existing| existing.path != report.path);
                    let report = Arc::new(report);
                    self.crash_history.push(Arc::clone(&report));
                    self.report = Some(report);
                    if let Some(other_path) = self.comparison.queued_path.take() {
                        self.comparison.path = other_path.display().to_string();
                        self.comparison.job =
                            Some(AnalysisJob::spawn(other_path, self.symbol_config()));
                        self.page = Page::Compare;
                    } else {
                        self.page = Page::Summary;
                    }
                    self.error = None;
                }
                Err(error) => {
                    self.comparison.queued_path = None;
                    self.error = Some(error);
                }
            }
        } else if self.analysis_job.is_some() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    fn receive_comparison(&mut self, ctx: &egui::Context) {
        let result = self.comparison.job.as_ref().and_then(AnalysisJob::poll);
        if let Some(result) = result {
            self.comparison.job = None;
            match result {
                Ok(report) => self.comparison.report = Some(Arc::new(report)),
                Err(error) => self.error = Some(error),
            }
        } else if self.comparison.job.is_some() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    fn symbol_config(&self) -> SymbolConfig {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        self.investigation.symbols.config(home.as_deref())
    }

    fn open_path_field(&mut self) {
        let path = expand_home(self.directory.trim());
        if path.is_file() {
            self.open(path);
        } else {
            self.directory = path.to_string_lossy().into_owned();
            self.refresh_directory();
        }
    }

    fn refresh_directory(&mut self) {
        self.library.files.clear();
        self.comparison.selected_paths.clear();
        let directory = expand_home(self.directory.trim());
        match std::fs::read_dir(&directory) {
            Ok(entries) => {
                let files = entries
                    .flatten()
                    .filter_map(|entry| {
                        let path = entry.path();
                        if !discovery::is_dump_path(&path) {
                            return None;
                        }
                        Some(FileEntry {
                            size: entry.metadata().ok()?.len(),
                            path,
                        })
                    })
                    .collect();
                self.library.replace(files);
                self.error = None;
            }
            Err(error) => {
                self.error = Some(format!("Cannot open {}: {error}", directory.display()));
            }
        }
    }

    fn start_system_scan(&mut self, ctx: &egui::Context) {
        let (sender, receiver) = mpsc::channel();
        let repaint = ctx.clone();
        self.library.begin_scan(Path::new("/"));
        self.comparison.selected_paths.clear();
        self.error = None;
        self.scan = Some(receiver);
        std::thread::spawn(move || {
            discovery::scan(Path::new("/"), &sender);
            repaint.request_repaint();
        });
    }

    fn compare_selected(&mut self) {
        let Some((current, comparison)) = take_comparison_pair(&mut self.comparison.selected_paths)
        else {
            return;
        };
        self.comparison.clear_result();
        self.comparison.queued_path = Some(comparison);
        self.open(current);
    }

    fn receive_scan_results(&mut self, ctx: &egui::Context) {
        let mut finished = false;
        if let Some(receiver) = &self.scan {
            while let Ok(event) = receiver.try_recv() {
                match event {
                    ScanEvent::FoundBatch(files) => self.library.found_batch(files),
                    ScanEvent::Progress { path, directories } => {
                        self.library.progress(path, directories);
                    }
                    ScanEvent::Finished => finished = true,
                }
            }
        }
        if finished {
            self.library.finish_scan();
            self.scan = None;
        } else if self.scan.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        egui::Frame::new()
            .fill(ui.visuals().panel_fill)
            .inner_margin(egui::Margin::symmetric(22, 14))
            .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("CrashLens").size(22.0).strong());
                        ui.label(
                            RichText::new("Crash dump explorer for Windows and Breakpad minidumps")
                                .small()
                                .color(ui.visuals().weak_text_color()),
                        );
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new("LOCAL · PRIVATE").small().strong());
                        if self.report.is_some() && ui.button("Back to dumps").clicked() {
                            self.report = None;
                        }
                    });
                });
                if self.report.is_none() {
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Location").strong());
                        let field_width = (ui.available_width() - 100.0).max(240.0);
                        let response = ui.add_sized(
                            [field_width, 34.0],
                            egui::TextEdit::singleline(&mut self.directory)
                                .font(FontId::monospace(13.0))
                                .hint_text("Directory or .dmp path"),
                        );
                        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            self.open_path_field();
                        }
                        if ui.button("Open").clicked() {
                            self.open_path_field();
                        }
                    });
                }
            });
    }

    fn browser(&mut self, ui: &mut egui::Ui) {
        let mut compare_requested = false;
        ui.label(RichText::new("CRASH LIBRARY").small().strong());
        ui.heading(RichText::new("Find and inspect crash dumps").size(28.0));
        ui.label(
            RichText::new(
                "Open a known location above, drop a dump onto this window, or scan this Mac for minidumps.",
            )
            .color(ui.visuals().weak_text_color()),
        );
        ui.add_space(18.0);
        egui::Frame::new()
            .fill(ui.visuals().faint_bg_color)
            .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
            .corner_radius(8)
            .inner_margin(16)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Automatic discovery").size(16.0).strong());
                        ui.label(
                            RichText::new("Search readable disks for .dmp and .mdmp files")
                                .color(ui.visuals().weak_text_color()),
                        );
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add_enabled(self.scan.is_none(), egui::Button::new("Scan this Mac"))
                            .clicked()
                        {
                            self.start_system_scan(ui.ctx());
                        }
                    });
                });
            });
        if let Some(location) = &self.library.scan_location {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    RichText::new(format!(
                        "Scanning system… {}+ folders checked · {}",
                        self.library.scanned_directories,
                        location.display()
                    ))
                    .small()
                    .color(ui.visuals().weak_text_color()),
                );
            });
        }
        ui.add_space(18.0);

        if let Some(error) = &self.error {
            egui::Frame::new()
                .fill(Color32::from_rgb(55, 29, 34))
                .inner_margin(10)
                .show(ui, |ui| {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                });
            ui.add_space(8.0);
        }

        egui::Frame::new()
            .fill(ui.visuals().panel_fill)
            .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
            .corner_radius(6)
            .inner_margin(12)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Discovered dumps").size(17.0).strong());
                        ui.label(
                            RichText::new(format!(
                                "{} minidump{} available",
                                self.library.files.len(),
                                if self.library.files.len() == 1 {
                                    ""
                                } else {
                                    "s"
                                }
                            ))
                            .color(ui.visuals().weak_text_color()),
                        );
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("Refresh current folder").clicked() {
                            self.refresh_directory();
                        }
                        if ui
                            .add_enabled(
                                self.comparison.selected_paths.len() == 2,
                                egui::Button::new(format!(
                                    "Compare selected ({}/2)",
                                    self.comparison.selected_paths.len()
                                )),
                            )
                            .clicked()
                        {
                            compare_requested = true;
                        }
                    });
                });
                ui.label(
                    RichText::new("Select exactly two dumps below to compare them.")
                        .small()
                        .color(ui.visuals().weak_text_color()),
                );
                ui.add_space(10.0);
                header_row(ui, &["COMPARE", "CRASH DUMP", "FILE SIZE", "ACTION"]);
                let mut selected = None;
                egui::ScrollArea::vertical().show_rows(
                    ui,
                    41.0,
                    self.library.files.len(),
                    |ui, range| {
                        for index in range {
                            let file = &self.library.files[index];
                            ui.horizontal(|ui| {
                                ui.set_min_height(34.0);
                                let is_selected =
                                    self.comparison.selected_paths.contains(&file.path);
                                let mut checked = is_selected;
                                let enabled =
                                    is_selected || self.comparison.selected_paths.len() < 2;
                                if ui
                                    .add_enabled(
                                        enabled,
                                        egui::Checkbox::without_text(&mut checked),
                                    )
                                    .on_hover_text("Select this dump for comparison")
                                    .changed()
                                {
                                    if checked {
                                        set_comparison_selected(
                                            &mut self.comparison.selected_paths,
                                            &file.path,
                                            true,
                                        );
                                    } else {
                                        set_comparison_selected(
                                            &mut self.comparison.selected_paths,
                                            &file.path,
                                            false,
                                        );
                                    }
                                }
                                ui.label(file.path.display().to_string());
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.small_button("Analyze").clicked() {
                                        selected = Some(file.path.clone());
                                    }
                                    ui.label(
                                        RichText::new(dump::human_bytes(file.size))
                                            .color(ui.visuals().weak_text_color()),
                                    );
                                });
                            });
                            ui.separator();
                        }
                    },
                );
                if self.library.files.is_empty() && self.error.is_none() {
                    ui.add_space(36.0);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new("No crash dumps found yet")
                                .size(17.0)
                                .strong(),
                        );
                        ui.label(
                            RichText::new(
                                "Choose a location, start a system scan, or drop a file here.",
                            )
                            .color(ui.visuals().weak_text_color()),
                        );
                    });
                    ui.add_space(36.0);
                }
                if let Some(path) = selected {
                    self.comparison.selected_paths.clear();
                    self.open(path);
                }
            });
        if compare_requested {
            self.compare_selected();
        }
    }

    fn navigation(&mut self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new("INVESTIGATE")
                .small()
                .strong()
                .color(ui.visuals().weak_text_color()),
        );
        nav_item(ui, &mut self.page, Page::Summary, "Overview");
        nav_item(ui, &mut self.page, Page::Threads, "Stack trace");
        nav_item(ui, &mut self.page, Page::Modules, "Code & modules");
        nav_item(ui, &mut self.page, Page::Memory, "Memory references");
        ui.add_space(16.0);
        ui.label(
            RichText::new("TOOLS")
                .small()
                .strong()
                .color(ui.visuals().weak_text_color()),
        );
        nav_item(ui, &mut self.page, Page::Search, "Search evidence");
        nav_item(ui, &mut self.page, Page::Compare, "Compare crashes");
        nav_item(ui, &mut self.page, Page::Report, "Notes & export");
        ui.add_space(16.0);
        ui.label(
            RichText::new("ADVANCED")
                .small()
                .strong()
                .color(ui.visuals().weak_text_color()),
        );
        nav_item(ui, &mut self.page, Page::Symbols, "Debug symbols");
        nav_item(ui, &mut self.page, Page::Streams, "Dump internals");
    }

    fn report(&mut self, ui: &mut egui::Ui) {
        let Some(report) = self.report.clone() else {
            return;
        };
        let available_height = ui.available_height();
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(176.0, available_height),
                Layout::top_down(Align::Min),
                |ui| self.navigation(ui),
            );
            ui.separator();
            let content_size = egui::vec2(ui.available_width(), available_height);
            ui.allocate_ui_with_layout(content_size, Layout::top_down(Align::Min), |ui| {
                if self.page != Page::Compare {
                    ui.label(RichText::new("CRASH ANALYSIS").small().strong());
                    ui.add_sized(
                        [ui.available_width(), 38.0],
                        egui::Label::new(
                            RichText::new(
                                report
                                    .path
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy(),
                            )
                            .size(27.0),
                        )
                        .truncate(),
                    );
                    ui.add_sized(
                        [ui.available_width(), 22.0],
                        egui::Label::new(
                            RichText::new(report.path.display().to_string())
                                .monospace()
                                .color(ui.visuals().weak_text_color()),
                        )
                        .truncate(),
                    );
                    ui.add_space(14.0);
                }
                let mut screen_action = None;
                let mut module_action = None;
                let mut memory_action = None;
                egui::ScrollArea::vertical()
                    .id_salt("analysis_content")
                    .auto_shrink([false, false])
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        match self.page {
                            Page::Summary => screen_action = summary(ui, &report),
                            Page::Threads => {
                                screen_action = threads(
                                    ui,
                                    &report,
                                    &mut self.view.filter,
                                    &mut self.view.selected_thread,
                                    &mut self.view.selected_frame,
                                    &self.investigation.symbols.source_root_from,
                                    &self.investigation.symbols.source_root_to,
                                );
                            }
                            Page::Modules => {
                                module_action = modules(
                                    ui,
                                    &report,
                                    &mut self.view.filter,
                                    &mut self.view.selected_module,
                                );
                            }
                            Page::Memory => {
                                memory_action = memory(
                                    ui,
                                    &report,
                                    &mut self.view.filter,
                                    &mut self.view.selected_memory,
                                    &mut self.view.memory,
                                );
                            }
                            Page::Streams => streams(ui, &report, &mut self.view.filter),
                            Page::Symbols => {
                                screen_action = symbol_settings(
                                    ui,
                                    &mut self.investigation.symbols.local_paths,
                                    &mut self.investigation.symbols.server_urls,
                                    &report,
                                    &mut self.investigation.symbols.source_root_from,
                                    &mut self.investigation.symbols.source_root_to,
                                );
                            }
                            Page::Report => report_tools(
                                ui,
                                &report,
                                &mut self.investigation.notes,
                                &mut self.investigation.status,
                                &mut self.investigation.export_result,
                                &mut self.investigation.tags,
                            ),
                            Page::Compare => {
                                screen_action = compare_view(
                                    ui,
                                    &report,
                                    &mut self.comparison.path,
                                    self.comparison.report.as_deref(),
                                );
                            }
                            Page::Search => global_search(ui, &report, &mut self.view.global_query),
                        }
                    });
                if let Some(action) = screen_action {
                    match action {
                        ScreenAction::OpenThreads => self.page = Page::Threads,
                        ScreenAction::OpenModules => self.page = Page::Modules,
                        ScreenAction::ConfigureSymbols => self.page = Page::Symbols,
                        ScreenAction::OpenModule(module_name) => {
                            if let Some(index) = self.report.as_ref().and_then(|report| {
                                report.modules.iter().position(|module| {
                                    module.name == module_name
                                        || module.name.ends_with(&module_name)
                                })
                            }) {
                                self.view.selected_module = index;
                                self.page = Page::Modules;
                            }
                        }
                        ScreenAction::ReanalyseWithSymbols => {
                            let path = report.path.clone();
                            self.open(path);
                            self.page = Page::Symbols;
                        }
                        ScreenAction::LoadComparison => {
                            let path = expand_home(self.comparison.path.trim());
                            let symbols = self.symbol_config();
                            self.comparison.job = Some(AnalysisJob::spawn(path, symbols));
                            self.page = Page::Compare;
                        }
                        ScreenAction::OpenPath(path) => {
                            if let Err(error) = crate::services::platform::open_path(&path) {
                                self.error = Some(format!("{error:#}"));
                            }
                        }
                    }
                }
                if let Some(action) = module_action {
                    match action {
                        ModuleAction::OpenFrame { thread, frame } => {
                            self.view.selected_thread = thread;
                            self.view.selected_frame = frame;
                            self.page = Page::Threads;
                        }
                        ModuleAction::ConfigureSymbols => self.page = Page::Symbols,
                    }
                }
                if let Some(MemoryAction { thread, frame }) = memory_action {
                    self.view.selected_thread = thread;
                    self.view.selected_frame = frame;
                    self.page = Page::Threads;
                }
            });
        });
    }
}

fn nav_item(ui: &mut egui::Ui, current: &mut Page, page: Page, label: &str) {
    if selection_row(ui, *current == page, label, 36.0).clicked() {
        *current = page;
    }
}

impl eframe::App for CrashLens {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // `App::ui` receives an unpainted root surface in eframe 0.35. Paint
        // the complete viewport first so unused vertical space is never the
        // renderer's black clear color.
        ui.painter()
            .rect_filled(ui.max_rect(), 0.0, ui.visuals().panel_fill);
        self.receive_scan_results(ui.ctx());
        self.receive_analysis(ui.ctx());
        self.receive_comparison(ui.ctx());
        if let Some(path) = ui
            .ctx()
            .input(|i| i.raw.dropped_files.iter().find_map(|f| f.path.clone()))
        {
            self.open(path);
        }
        ui.spacing_mut().item_spacing.y = 6.0;
        self.toolbar(ui);
        if let Some(path) = self.analysis_job.as_ref().map(|job| &job.path) {
            egui::Frame::new()
                .fill(ui.visuals().faint_bg_color)
                .inner_margin(egui::Margin::symmetric(18, 10))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(format!("Analyzing {}…", path.display()));
                        ui.label(
                            RichText::new("Parsing, unwinding and resolving symbols")
                                .small()
                                .color(ui.visuals().weak_text_color()),
                        );
                    });
                });
        }
        egui::Frame::new()
            .fill(ui.visuals().panel_fill)
            .inner_margin(egui::Margin::same(16))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.set_min_height(ui.available_height());
                if self.report.is_some() {
                    self.report(ui);
                } else {
                    self.browser(ui);
                }
            });
    }
}

fn expand_home(value: &str) -> PathBuf {
    if value == "~" {
        return std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
    }
    if let Some(rest) = value.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return Path::new(&home).join(rest);
    }
    PathBuf::from(value)
}

#[cfg(test)]
mod tests {
    use super::expand_home;

    #[test]
    fn leaves_absolute_paths_untouched() {
        assert_eq!(
            expand_home("/tmp/crashes"),
            std::path::PathBuf::from("/tmp/crashes")
        );
    }
}

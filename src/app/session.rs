use super::{
    screens::MemoryAnalysisCache,
    state::{AnalysisJob, SymbolSettings},
};
use crate::domain::DumpReport;
use std::fmt;
use std::{path::PathBuf, sync::Arc};

/// Transient state belonging to the currently displayed crash report.
/// Resetting this object cannot accidentally discard library or comparison state.
#[derive(Default)]
pub(super) struct AnalysisViewState {
    pub filter: String,
    pub selected_thread: usize,
    pub selected_frame: usize,
    pub selected_memory: Option<usize>,
    pub selected_module: usize,
    pub memory: MemoryAnalysisCache,
    pub global_query: String,
}

impl AnalysisViewState {
    pub fn reset_for(&mut self, report: &DumpReport) {
        self.filter.clear();
        self.selected_thread = report
            .threads
            .iter()
            .position(|thread| thread.crashed)
            .unwrap_or(0);
        self.selected_frame = 0;
        self.selected_memory = None;
        self.selected_module = 0;
        self.memory.clear();
        self.global_query.clear();
    }
}

/// User-authored investigation data and symbol/source configuration.
#[derive(Default)]
pub(super) struct InvestigationState {
    pub symbols: SymbolSettings,
    pub notes: String,
    pub status: InvestigationStatus,
    pub tags: String,
    pub export_result: Option<Result<PathBuf, String>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum InvestigationStatus {
    #[default]
    New,
    Investigating,
    Resolved,
    Ignored,
}

impl InvestigationStatus {
    pub const ALL: [Self; 4] = [
        Self::New,
        Self::Investigating,
        Self::Resolved,
        Self::Ignored,
    ];
}

impl fmt::Display for InvestigationStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::New => "New",
            Self::Investigating => "Investigating",
            Self::Resolved => "Resolved",
            Self::Ignored => "Ignored",
        })
    }
}

/// Everything needed for the optional second report and library pair selection.
#[derive(Default)]
pub(super) struct ComparisonState {
    pub path: String,
    pub report: Option<Arc<DumpReport>>,
    pub job: Option<AnalysisJob>,
    pub selected_paths: Vec<PathBuf>,
    pub queued_path: Option<PathBuf>,
}

impl ComparisonState {
    pub fn clear_result(&mut self) {
        self.report = None;
        self.job = None;
    }
}

#[cfg(test)]
mod tests {
    use super::AnalysisViewState;
    use crate::domain::{DumpReport, ThreadRow};

    #[test]
    fn reset_selects_crashed_thread_and_clears_transient_state() {
        let thread = |crashed| ThreadRow {
            id: 0,
            name: String::new(),
            stack_start: String::new().into(),
            stack_size: 0,
            crashed,
            frames: Vec::new(),
        };
        let report = DumpReport {
            threads: vec![thread(false), thread(true)],
            ..Default::default()
        };
        let mut state = AnalysisViewState {
            filter: "stale".into(),
            selected_frame: 7,
            global_query: "old".into(),
            ..Default::default()
        };

        state.reset_for(&report);

        assert_eq!(state.selected_thread, 1);
        assert_eq!(state.selected_frame, 0);
        assert_eq!(state.selected_memory, None);
        assert!(state.filter.is_empty());
        assert!(state.global_query.is_empty());
    }
}

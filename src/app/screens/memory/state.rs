use crate::{
    domain::{DumpReport, MemoryRow},
    services::memory as memory_analysis,
};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum MemoryMode {
    #[default]
    Overview,
    Evidence,
    Browser,
    Map,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum BrowserDetail {
    #[default]
    Connection,
    Pointers,
    Text,
    Bytes,
}

#[derive(Default)]
pub struct MemoryAnalysisCache {
    pub(super) query: String,
    pub(super) memory_count: usize,
    pub(super) matches: Vec<usize>,
    pub(super) selected_start: crate::domain::VirtualAddress,
    pub(super) pointers: Vec<memory_analysis::DecodedPointer>,
    pub(super) strings: Vec<String>,
    pub(super) mode: MemoryMode,
    pub(super) filter_all: bool,
    pub(super) map_summary: Option<MemoryMapSummary>,
    pub(super) browser_detail: BrowserDetail,
    pub(super) show_registers: bool,
}

impl MemoryAnalysisCache {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub(super) fn refresh_filter(
        &mut self,
        report: &DumpReport,
        query: &str,
        filter_all: bool,
    ) -> bool {
        if self.query == query
            && self.memory_count == report.memory.len()
            && self.filter_all == filter_all
        {
            return false;
        }
        self.query = query.to_owned();
        self.memory_count = report.memory.len();
        self.filter_all = filter_all;
        self.matches = report
            .memory
            .iter()
            .enumerate()
            .filter(|(_, region)| {
                if filter_all || !query.trim().is_empty() {
                    return memory_analysis::matches_query(region, query);
                }
                memory_analysis::contains(
                    region,
                    memory_analysis::parse_address(&report.crash_address),
                ) || memory_analysis::region_roles(report, region)
                    .iter()
                    .any(|role| role.contains("(crashed)"))
            })
            .map(|(index, _)| index)
            .collect();
        self.matches.sort_by_key(|&index| {
            let region = &report.memory[index];
            let roles = memory_analysis::region_roles(report, region);
            if memory_analysis::contains(
                region,
                memory_analysis::parse_address(&report.crash_address),
            ) {
                0
            } else if roles.iter().any(|role| role.contains("(crashed)")) {
                1
            } else if roles.is_empty() {
                3
            } else {
                2
            }
        });
        true
    }

    pub(super) fn refresh_region(&mut self, report: &DumpReport, region: &MemoryRow) {
        if self.selected_start == region.start {
            return;
        }
        self.selected_start = region.start;
        self.pointers = memory_analysis::decoded_pointers(report, region, 24);
        self.strings = memory_analysis::extract_strings(&region.bytes, 24);
    }

    pub(super) fn refresh_map(&mut self, report: &DumpReport) {
        if self.map_summary.is_some() {
            return;
        }
        let mut groups = [MemoryMapGroup::default(); 4];
        let mut crash_region = None;
        for (index, region) in report.memory.iter().enumerate() {
            let roles = memory_analysis::region_roles(report, region);
            let is_fault = memory_analysis::contains(
                region,
                memory_analysis::parse_address(&report.crash_address),
            );
            let group = if is_fault || roles.iter().any(|role| role.contains("(crashed)")) {
                crash_region.get_or_insert(index);
                0
            } else if roles.iter().any(|role| role.starts_with("Stack for")) {
                1
            } else if roles
                .iter()
                .any(|role| role.starts_with("Mapped to module"))
            {
                2
            } else {
                3
            };
            groups[group].regions += 1;
            groups[group].bytes = groups[group].bytes.saturating_add(region.size);
        }
        self.map_summary = Some(MemoryMapSummary {
            groups,
            crash_region,
        });
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct MemoryMapGroup {
    pub(super) regions: usize,
    pub(super) bytes: u64,
}

pub(super) struct MemoryMapSummary {
    pub(super) groups: [MemoryMapGroup; 4],
    pub(super) crash_region: Option<usize>,
}

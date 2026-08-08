//! Presentation-ready relationships derived from one or two domain reports.
//!
//! Keeping this logic outside egui makes comparisons and evidence ordering easy
//! to test without constructing a window.

use crate::{
    domain::{DumpReport, ModuleOwnership},
    services::investigation,
};
use std::{collections::BTreeMap, path::Path};

#[derive(Debug, PartialEq, Eq)]
/// One ordered link from the exception to actionable evidence.
pub struct EvidenceStep {
    pub title: &'static str,
    pub value: String,
    pub explanation: &'static str,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CrashEvidence {
    pub steps: Vec<EvidenceStep>,
    pub needs_symbols: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct InvestigationInsight {
    pub priority: &'static str,
    pub title: String,
    pub detail: String,
}

pub fn investigation_insights(report: &DumpReport) -> Vec<InvestigationInsight> {
    let mut insights = Vec::new();
    if let Some(module) = report.modules.iter().find(|module| module.contains_fault) {
        insights.push(InvestigationInsight {
            priority: "HIGH",
            title: format!("Fault address maps to {}", module.name),
            detail: format!(
                "{} code · symbols {}. Inspect its related frames and callers.",
                module.ownership, module.symbol_status
            ),
        });
    }
    let crashed = report.threads.iter().find(|thread| thread.crashed);
    if let Some(thread) = crashed {
        let missing = thread
            .frames
            .iter()
            .filter(|frame| frame.missing_symbols)
            .count();
        if missing > 0 {
            insights.push(InvestigationInsight {
                priority: "BLOCKER",
                title: format!(
                    "{missing} crashing-stack frame{} lack symbols",
                    if missing == 1 { "" } else { "s" }
                ),
                detail: "Load matching symbols before trusting function-level conclusions.".into(),
            });
        }
        if thread.frames.is_empty() {
            insights.push(InvestigationInsight {
                priority: "BLOCKER",
                title: "No crashing stack was recovered".into(),
                detail: "Check dump completeness, architecture support, and unwind symbols.".into(),
            });
        }
    } else {
        insights.push(InvestigationInsight {
            priority: "BLOCKER",
            title: "Crashing thread was not identified".into(),
            detail: "The dump may be manual or missing its exception stream.".into(),
        });
    }
    if report.diagnostics.is_empty() && insights.is_empty() {
        insights.push(InvestigationInsight {
            priority: "INFO",
            title: "No immediate data-quality blockers detected".into(),
            detail: "Start with the crashing thread and validate the top recovered frames.".into(),
        });
    }
    insights
}

#[derive(Debug, PartialEq, Eq)]
/// One report field and its value in each side of a comparison.
pub struct FieldDelta {
    pub field: &'static str,
    pub current: String,
    pub comparison: String,
}

pub struct ComparisonAnalysis {
    pub same_signature: bool,
    pub changed: Vec<FieldDelta>,
    pub unchanged: Vec<FieldDelta>,
    pub current_stack: Vec<String>,
    pub comparison_stack: Vec<String>,
    pub only_current_modules: Vec<ModulePresence>,
    pub only_comparison_modules: Vec<ModulePresence>,
    pub changed_modules: Vec<ModuleDelta>,
}

pub struct ModuleDelta {
    pub name: String,
    pub current: String,
    pub comparison: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModulePresence {
    pub name: String,
    pub path: String,
    pub ownership: ModuleOwnership,
}

impl ComparisonAnalysis {
    pub fn new(current: &DumpReport, comparison: &DumpReport) -> Self {
        let fields = [
            delta(
                "Crash reason",
                &current.crash_reason,
                &comparison.crash_reason,
            ),
            delta(
                "Fault address",
                current.crash_address,
                comparison.crash_address,
            ),
            delta(
                "Operating system",
                current.operating_system,
                comparison.operating_system,
            ),
            delta(
                "Architecture",
                current.architecture,
                comparison.architecture,
            ),
            delta(
                "Thread count",
                current.threads.len().to_string(),
                comparison.threads.len().to_string(),
            ),
            delta(
                "Module count",
                current.modules.len().to_string(),
                comparison.modules.len().to_string(),
            ),
        ];
        let (changed, unchanged) = fields
            .into_iter()
            .partition(|field| field.current != field.comparison);
        let current_modules = modules_by_identity(current);
        let comparison_modules = modules_by_identity(comparison);
        let changed_modules = current_modules
            .iter()
            .filter_map(|(name, current_module)| {
                let comparison_module = comparison_modules.get(name)?;
                (current_module.path != comparison_module.path).then(|| ModuleDelta {
                    name: name.clone(),
                    current: current_module.path.clone(),
                    comparison: comparison_module.path.clone(),
                })
            })
            .collect();
        Self {
            same_signature: investigation::crash_signature(current)
                == investigation::crash_signature(comparison),
            changed,
            unchanged,
            current_stack: top_stack(current),
            comparison_stack: top_stack(comparison),
            only_current_modules: current_modules
                .iter()
                .filter(|(name, _)| !comparison_modules.contains_key(*name))
                .map(|(_, module)| module.clone())
                .collect(),
            only_comparison_modules: comparison_modules
                .iter()
                .filter(|(name, _)| !current_modules.contains_key(*name))
                .map(|(_, module)| module.clone())
                .collect(),
            changed_modules,
        }
    }
}

fn modules_by_identity(report: &DumpReport) -> BTreeMap<String, ModulePresence> {
    report
        .modules
        .iter()
        .map(|module| {
            let identity = Path::new(&module.name)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            (
                identity.clone(),
                ModulePresence {
                    name: identity,
                    path: module.name.clone(),
                    ownership: module.ownership,
                },
            )
        })
        .collect()
}

fn delta(field: &'static str, current: impl ToString, comparison: impl ToString) -> FieldDelta {
    FieldDelta {
        field,
        current: current.to_string(),
        comparison: comparison.to_string(),
    }
}

fn top_stack(report: &DumpReport) -> Vec<String> {
    report
        .threads
        .iter()
        .find(|thread| thread.crashed)
        .into_iter()
        .flat_map(|thread| thread.frames.iter().take(6))
        .map(|frame| {
            let name = if frame.function.is_empty() {
                frame.instruction.to_string()
            } else {
                frame.function.clone()
            };
            format!("{}  {}!{}", frame.index, frame.module, name)
        })
        .collect()
}

impl CrashEvidence {
    pub fn from_report(report: &DumpReport) -> Self {
        let thread = report.threads.iter().find(|thread| thread.crashed);
        let frame = thread.and_then(|thread| thread.frames.first());
        let module = frame.and_then(|frame| {
            report.modules.iter().find(|module| {
                module.contains_fault
                    || (!frame.module.is_empty()
                        && (module.name == frame.module || module.name.ends_with(&frame.module)))
            })
        });
        let needs_symbols = frame.is_some_and(|frame| frame.missing_symbols);
        let steps = vec![
            EvidenceStep {
                title: "Exception",
                value: format!("{} at {}", report.crash_reason, report.crash_address),
                explanation: "The event that stopped the process.",
            },
            EvidenceStep {
                title: "Crashed thread",
                value: thread
                    .map(|thread| {
                        if thread.name.is_empty() {
                            format!("Thread {}", thread.id)
                        } else {
                            format!("{} (thread {})", thread.name, thread.id)
                        }
                    })
                    .unwrap_or_else(|| "Not identified".into()),
                explanation: "This execution context owns the failing stack.",
            },
            EvidenceStep {
                title: "Frame",
                value: frame
                    .map(|frame| {
                        let name = if !frame.function.is_empty() {
                            frame.function.clone()
                        } else if !frame.module.is_empty() {
                            frame.module.clone()
                        } else {
                            frame.instruction.to_string()
                        };
                        let trust = if frame.trust.is_empty() {
                            "unknown"
                        } else {
                            &frame.trust
                        };
                        format!("{name} · confidence {trust}")
                    })
                    .unwrap_or_else(|| "No frame recovered".into()),
                explanation: "The best available candidate for the active failing code.",
            },
            EvidenceStep {
                title: "Module and symbols",
                value: module
                    .map(|module| {
                        format!(
                            "{} · {} · symbols {}",
                            module.name, module.ownership, module.symbol_status
                        )
                    })
                    .unwrap_or_else(|| "No matching loaded module".into()),
                explanation: "Ownership and symbol quality determine how actionable the stack is.",
            },
            EvidenceStep {
                title: "Source location",
                value: frame
                    .filter(|frame| !frame.source_file.is_empty())
                    .map(|frame| {
                        format!(
                            "{}:{}",
                            frame.source_file,
                            frame.source_line.unwrap_or_default()
                        )
                    })
                    .unwrap_or_else(|| "Source unavailable — add matching symbols".into()),
                explanation: "The closest point to inspect in source code.",
            },
        ];
        Self {
            steps,
            needs_symbols,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{CpuArchitecture, FrameRow, ModuleRow, SymbolStatus, ThreadRow};

    #[test]
    fn builds_complete_evidence_chain_from_related_records() {
        let report = DumpReport {
            crash_reason: "EXCEPTION_ACCESS_VIOLATION".into(),
            crash_address: "0x1010".into(),
            threads: vec![ThreadRow {
                id: 7,
                name: "render".into(),
                stack_start: String::new().into(),
                stack_size: 0,
                crashed: true,
                frames: vec![FrameRow {
                    function: "draw".into(),
                    module: "app.dll".into(),
                    source_file: "src/render.rs".into(),
                    source_line: Some(88),
                    trust: "context".into(),
                    ..Default::default()
                }],
            }],
            modules: vec![ModuleRow {
                name: "app.dll".into(),
                base: String::new().into(),
                size: 0,
                code_id: String::new(),
                symbol_status: SymbolStatus::Loaded,
                ownership: ModuleOwnership::ApplicationOrThirdParty,
                contains_fault: false,
            }],
            ..Default::default()
        };
        let evidence = CrashEvidence::from_report(&report);
        assert_eq!(evidence.steps.len(), 5);
        assert_eq!(evidence.steps[1].value, "render (thread 7)");
        assert!(evidence.steps[2].value.contains("draw"));
        assert!(evidence.steps[3].value.contains("app.dll · Application"));
        assert_eq!(evidence.steps[4].value, "src/render.rs:88");
        assert!(!evidence.needs_symbols);
    }

    #[test]
    fn describes_missing_evidence_without_panicking() {
        let evidence = CrashEvidence::from_report(&DumpReport::default());
        assert_eq!(evidence.steps[1].value, "Not identified");
        assert_eq!(evidence.steps[2].value, "No frame recovered");
        assert_eq!(evidence.steps[3].value, "No matching loaded module");
    }

    #[test]
    fn insights_expose_fault_module_and_missing_symbols() {
        let mut report = DumpReport::default();
        report.modules.push(ModuleRow {
            name: "app.dll".into(),
            base: String::new().into(),
            size: 0,
            code_id: String::new(),
            symbol_status: SymbolStatus::Missing,
            ownership: ModuleOwnership::ApplicationOrThirdParty,
            contains_fault: true,
        });
        report.threads.push(ThreadRow {
            id: 1,
            name: String::new(),
            stack_start: String::new().into(),
            stack_size: 0,
            crashed: true,
            frames: vec![FrameRow {
                missing_symbols: true,
                ..Default::default()
            }],
        });
        let insights = investigation_insights(&report);
        assert!(insights.iter().any(|item| item.title.contains("app.dll")));
        assert!(insights.iter().any(|item| item.priority == "BLOCKER"));
    }

    #[test]
    fn comparison_separates_changed_fields_and_module_sets() {
        let mut left = DumpReport {
            crash_reason: "A".into(),
            architecture: CpuArchitecture::X86_64,
            ..Default::default()
        };
        left.modules.push(ModuleRow {
            name: "left.dll".into(),
            base: String::new().into(),
            size: 0,
            code_id: String::new(),
            symbol_status: SymbolStatus::NotReferenced,
            ownership: ModuleOwnership::ApplicationOrThirdParty,
            contains_fault: false,
        });
        let mut right = DumpReport {
            crash_reason: "B".into(),
            architecture: CpuArchitecture::X86_64,
            ..Default::default()
        };
        right.modules.push(ModuleRow {
            name: "right.dll".into(),
            base: String::new().into(),
            size: 0,
            code_id: String::new(),
            symbol_status: SymbolStatus::NotReferenced,
            ownership: ModuleOwnership::ApplicationOrThirdParty,
            contains_fault: false,
        });
        let comparison = ComparisonAnalysis::new(&left, &right);
        assert!(
            comparison
                .changed
                .iter()
                .any(|field| field.field == "Crash reason")
        );
        assert!(
            comparison
                .unchanged
                .iter()
                .any(|field| field.field == "Architecture")
        );
        assert_eq!(comparison.only_current_modules[0].name, "left.dll");
        assert_eq!(comparison.only_current_modules[0].path, "left.dll");
        assert_eq!(comparison.only_comparison_modules[0].name, "right.dll");
        assert_eq!(comparison.only_comparison_modules[0].path, "right.dll");
    }

    #[test]
    fn comparison_treats_same_module_at_a_new_path_as_changed() {
        let module = |name: &str| ModuleRow {
            name: name.into(),
            base: String::new().into(),
            size: 0,
            code_id: String::new(),
            symbol_status: SymbolStatus::NotReferenced,
            ownership: ModuleOwnership::ApplicationOrThirdParty,
            contains_fault: false,
        };
        let left = DumpReport {
            modules: vec![module("/Framework/147/App")],
            ..Default::default()
        };
        let right = DumpReport {
            modules: vec![module("/Framework/145/App")],
            ..Default::default()
        };
        let comparison = ComparisonAnalysis::new(&left, &right);
        assert_eq!(comparison.changed_modules.len(), 1);
        assert_eq!(comparison.changed_modules[0].name, "App");
        assert!(comparison.only_current_modules.is_empty());
        assert!(comparison.only_comparison_modules.is_empty());
    }
}

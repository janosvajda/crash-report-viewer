//! Presentation-oriented relationships between captured regions and crash data.

use crate::{
    domain::{DumpReport, MemoryRow},
    services::memory as memory_analysis,
};

pub(super) fn region_relevance(
    report: &DumpReport,
    region: &MemoryRow,
    reference_count: usize,
    roles: &[String],
) -> (&'static str, &'static str, String) {
    if memory_analysis::contains(
        region,
        memory_analysis::parse_address(&report.crash_address),
    ) {
        return (
            "HIGH",
            "Contains the recorded fault address",
            "Inspect pointers and nearby bytes because the processor reported the failure inside this address range.".into(),
        );
    }
    if roles.iter().any(|role| role.contains("(crashed)")) {
        return (
            "HIGH",
            "Stack memory for the crashed thread",
            "This region contains the failing thread's saved stack and may hold arguments, return addresses, and local pointer values.".into(),
        );
    }
    if reference_count > 0 {
        return (
            "RELEVANT",
            "Referenced by recovered execution state",
            format!(
                "{reference_count} recovered instruction or register value points into this region. Use the frame links below to inspect the relationship."
            ),
        );
    }
    if roles
        .iter()
        .any(|role| role.starts_with("Mapped to module"))
    {
        return (
            "CONTEXT",
            "Loaded module memory",
            "This region belongs to loaded code, but no recovered crash register or instruction directly references it.".into(),
        );
    }
    if roles
        .iter()
        .any(|role| role.starts_with("Stack for thread"))
    {
        return (
            "LOW",
            "Stack for a non-crashed thread",
            "This region describes another thread's state and currently has no direct link to the crash. It is usually safe to deprioritize.".into(),
        );
    }
    (
        "UNKNOWN",
        "No known relationship to the crash",
        "The dump captured these bytes, but the available fault, stack, module, and register evidence does not explain why they matter.".into(),
    )
}

pub(super) fn stack_thread_for_region(
    report: &DumpReport,
    region: &MemoryRow,
) -> Option<(usize, u32, bool)> {
    report
        .threads
        .iter()
        .enumerate()
        .find(|(_, thread)| {
            memory_analysis::contains(region, memory_analysis::parse_address(&thread.stack_start))
        })
        .map(|(index, thread)| (index, thread.id, thread.crashed))
}

pub(super) struct RegionReference {
    pub(super) thread: usize,
    pub(super) frame: usize,
    pub(super) description: String,
}

pub(super) fn region_references(report: &DumpReport, region: &MemoryRow) -> Vec<RegionReference> {
    let mut references = Vec::new();
    for (thread_index, thread) in report.threads.iter().enumerate() {
        for (frame_index, frame) in thread.frames.iter().enumerate() {
            if memory_analysis::contains(region, memory_analysis::parse_address(&frame.instruction))
            {
                references.push(RegionReference {
                    thread: thread_index,
                    frame: frame_index,
                    description: format!(
                        "Thread {} frame {} instruction {}",
                        thread.id, frame.index, frame.instruction
                    ),
                });
            }
            for (register, value) in &frame.registers {
                if memory_analysis::contains(region, memory_analysis::parse_address(value)) {
                    references.push(RegionReference {
                        thread: thread_index,
                        frame: frame_index,
                        description: format!(
                            "Thread {} frame {} register {} = {}",
                            thread.id, frame.index, register, value
                        ),
                    });
                }
            }
        }
    }
    references
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{FrameRow, ThreadRow};

    #[test]
    fn finds_instruction_and_register_references_inside_region() {
        let region = MemoryRow {
            start: "0x1000".into(),
            size: 0x100,
            preview: String::new(),
            bytes: vec![0; 0x100],
            permissions: crate::domain::MemoryPermissions::Recorded {
                readable: true,
                writable: true,
                executable: false,
            },
            mapping_type: "Private".into(),
        };
        let report = DumpReport {
            threads: vec![ThreadRow {
                id: 9,
                name: String::new(),
                stack_start: String::new().into(),
                stack_size: 0,
                crashed: true,
                frames: vec![FrameRow {
                    instruction: "0x1010".into(),
                    registers: vec![("rsp".into(), "0x1020".into())],
                    ..Default::default()
                }],
            }],
            ..Default::default()
        };
        assert_eq!(region_references(&report, &region).len(), 2);
        assert!(memory_analysis::contains(&region, Some(0x10ff)));
        assert!(!memory_analysis::contains(&region, Some(0x1100)));
    }
}

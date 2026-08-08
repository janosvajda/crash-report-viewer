use crate::domain::DumpReport;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchHit {
    pub kind: &'static str,
    pub value: String,
}

pub fn likely_cause(report: &DumpReport) -> &'static str {
    let reason = report.crash_reason.to_ascii_lowercase();
    let near_null = report
        .crash_address
        .value()
        .is_some_and(|value| value < 4096);
    if near_null && (reason.contains("access") || reason.contains("segv")) {
        "probable null-pointer access"
    } else if reason.contains("stack_overflow") || reason.contains("stack overflow") {
        "probable recursion or stack exhaustion"
    } else if reason.contains("illegal_instruction") || reason.contains("illegal instruction") {
        "invalid instruction or CPU incompatibility"
    } else if reason.contains("divide") {
        "arithmetic fault"
    } else if reason.contains("access") || reason.contains("segv") {
        "invalid memory access"
    } else {
        "cause requires frame-level investigation"
    }
}

pub fn crash_signature(report: &DumpReport) -> String {
    let frames = report
        .threads
        .iter()
        .find(|thread| thread.crashed)
        .into_iter()
        .flat_map(|thread| thread.frames.iter().take(3))
        .map(|frame| {
            if frame.function.is_empty() {
                format!("{}+{}", frame.module, frame.instruction)
            } else {
                format!("{}!{}", frame.module, frame.function)
            }
        })
        .collect::<Vec<_>>()
        .join(" → ");
    format!("{} · {frames}", report.crash_reason)
}

pub fn remap_source_path(path: &str, from: &str, to: &str) -> PathBuf {
    if !from.is_empty() && !to.is_empty() && path.starts_with(from) {
        return PathBuf::from(to).join(path.trim_start_matches(from).trim_start_matches('/'));
    }
    PathBuf::from(path)
}

pub fn source_excerpt(path: &Path, target_line: usize, radius: usize) -> std::io::Result<String> {
    let source = std::fs::read_to_string(path)?;
    let first = target_line.saturating_sub(radius).max(1);
    let last = target_line.saturating_add(radius);
    Ok(source
        .lines()
        .enumerate()
        .filter(|(index, _)| (first..=last).contains(&(index + 1)))
        .map(|(index, line)| {
            format!(
                "{} {:>5}  {}",
                if index + 1 == target_line { ">" } else { " " },
                index + 1,
                line
            )
        })
        .collect::<Vec<_>>()
        .join("\n"))
}

pub fn search(report: &DumpReport, query: &str) -> Vec<SearchHit> {
    let needle = query.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    let contains = |value: &str| value.to_ascii_lowercase().contains(&needle);
    let mut matches = Vec::new();
    for thread in &report.threads {
        if contains(&thread.name) || thread.id.to_string().contains(&needle) {
            matches.push(SearchHit {
                kind: "Thread",
                value: format!("{} · {}", thread.id, thread.name),
            });
        }
        for frame in &thread.frames {
            if contains(&frame.function)
                || contains(&frame.module)
                || contains(&frame.source_file)
                || contains(&frame.instruction.to_string())
                || contains(&frame.function_offset)
            {
                matches.push(SearchHit {
                    kind: "Stack frame",
                    value: format!(
                        "Thread {} · {}!{} · {}:{}",
                        thread.id,
                        frame.module,
                        if frame.function.is_empty() {
                            "Not available"
                        } else {
                            &frame.function
                        },
                        frame.source_file,
                        frame.source_line.unwrap_or_default()
                    ),
                });
            }
        }
    }
    for module in &report.modules {
        if contains(&module.name) || contains(&module.base.to_string()) || contains(&module.code_id)
        {
            matches.push(SearchHit {
                kind: "Module",
                value: format!("{} · {}", module.name, module.base),
            });
        }
    }
    for memory in &report.memory {
        if contains(&memory.start.to_string()) {
            matches.push(SearchHit {
                kind: "Memory",
                value: format!("{} · {} bytes", memory.start, memory.size),
            });
        }
    }
    for stream in &report.streams {
        if contains(&stream.kind) {
            matches.push(SearchHit {
                kind: "Stream",
                value: stream.kind.clone(),
            });
        }
    }
    matches
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{DumpReport, FrameRow, ThreadRow};

    fn report(reason: &str, address: &str) -> DumpReport {
        DumpReport {
            crash_reason: reason.into(),
            crash_address: address.into(),
            ..Default::default()
        }
    }

    #[test]
    fn identifies_null_pointer_access() {
        assert_eq!(
            likely_cause(&report("EXCEPTION_ACCESS_VIOLATION", "0x00000010")),
            "probable null-pointer access"
        );
    }

    #[test]
    fn identifies_stack_overflow() {
        assert_eq!(
            likely_cause(&report("EXCEPTION_STACK_OVERFLOW", "0x10000")),
            "probable recursion or stack exhaustion"
        );
    }

    #[test]
    fn signature_uses_top_three_crashed_frames() {
        let mut report = report("SIGSEGV", "0x0");
        report.threads.push(ThreadRow {
            id: 7,
            name: String::new(),
            stack_start: String::new().into(),
            stack_size: 0,
            crashed: true,
            frames: (0..4)
                .map(|index| FrameRow {
                    module: "app".into(),
                    function: format!("fn{index}"),
                    ..Default::default()
                })
                .collect(),
        });
        let signature = crash_signature(&report);
        assert!(signature.contains("app!fn0 → app!fn1 → app!fn2"));
        assert!(!signature.contains("fn3"));
    }

    #[test]
    fn remaps_only_matching_source_roots() {
        assert_eq!(
            remap_source_path("C:/build/app/src/main.rs", "C:/build/app", "/local/app"),
            PathBuf::from("/local/app/src/main.rs")
        );
        assert_eq!(
            remap_source_path("/other/main.rs", "C:/build/app", "/local/app"),
            PathBuf::from("/other/main.rs")
        );
    }

    #[test]
    fn source_excerpt_marks_target_and_respects_radius() {
        let path = std::env::temp_dir().join(format!(
            "crashlens-source-{}-{}.rs",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        std::fs::write(&path, "one\ntwo\nthree\nfour\nfive\n").unwrap();
        let excerpt = source_excerpt(&path, 3, 1).unwrap();
        let _ = std::fs::remove_file(path);
        assert!(!excerpt.contains("one"));
        assert!(excerpt.contains("two"));
        assert!(excerpt.contains(">     3  three"));
        assert!(excerpt.contains("four"));
        assert!(!excerpt.contains("five"));
    }

    #[test]
    fn search_matches_frames_case_insensitively() {
        let mut report = report("SIGSEGV", "0x0");
        report.threads.push(ThreadRow {
            id: 1,
            name: "Main Thread".into(),
            stack_start: String::new().into(),
            stack_size: 0,
            crashed: true,
            frames: vec![FrameRow {
                module: "Application.dll".into(),
                function: "RestoreSession".into(),
                ..Default::default()
            }],
        });
        let hits = search(&report, "restore");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, "Stack frame");
        assert!(
            search(&report, "MAIN THREAD")
                .iter()
                .any(|hit| hit.kind == "Thread")
        );
        assert!(search(&report, "   ").is_empty());
    }
}

use crate::{domain::DumpReport, services::investigation};
use anyhow::{Context, Result};
use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
};

pub fn export_markdown_to(
    path: &Path,
    report: &DumpReport,
    notes: &str,
    status: &str,
    tags: &str,
    sanitized: bool,
) -> Result<PathBuf> {
    write_export(
        path,
        render_markdown(report, notes, status, tags, sanitized).as_bytes(),
    )
}

pub fn export_json_to(path: &Path, report: &DumpReport) -> Result<PathBuf> {
    write_export(path, report.processor_json.as_bytes())
}

pub fn export_stack_to(path: &Path, report: &DumpReport) -> Result<PathBuf> {
    write_export(path, render_stack(report).as_bytes())
}

pub fn export_bundle_to(
    directory: &Path,
    report: &DumpReport,
    notes: &str,
    status: &str,
    tags: &str,
) -> Result<PathBuf> {
    std::fs::create_dir_all(directory)
        .with_context(|| format!("Could not create {}", directory.display()))?;
    let readme = format!(
        "# Crash investigation bundle\n\n- Status: {status}\n- Tags: {}\n- Signature: `{}`\n- Reason: `{}`\n- Address: `{}`\n\n## Contents\n\n- `report.md`: readable crash report and notes\n- `stack.txt`: plain call stacks\n- `analysis.json`: raw processor output\n\n## Notes\n\n{notes}\n",
        display_tags(tags),
        investigation::crash_signature(report),
        report.crash_reason,
        report.crash_address
    );
    let markdown = render_markdown(report, notes, status, tags, false);
    let stack = render_stack(report);
    for (name, contents) in [
        ("README.md", readme.as_bytes()),
        ("report.md", markdown.as_bytes()),
        ("stack.txt", stack.as_bytes()),
        ("analysis.json", report.processor_json.as_bytes()),
    ] {
        write_file(&directory.join(name), contents)?;
    }
    Ok(directory.to_owned())
}

pub fn render_markdown(
    report: &DumpReport,
    notes: &str,
    status: &str,
    tags: &str,
    sanitized: bool,
) -> String {
    let display_path = if sanitized {
        report
            .path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    } else {
        report.path.display().to_string()
    };
    let mut text = format!(
        "# Crash report\n\n- Dump: `{display_path}`\n- Report status: {status}\n- Tags: {}\n- Crash reason: `{}`\n- Fault address: `{}`\n- Platform: {} / {}\n\n## Call stacks\n",
        display_tags(tags),
        report.crash_reason,
        report.crash_address,
        report.operating_system,
        report.architecture
    );
    for thread in &report.threads {
        let _ = writeln!(
            text,
            "\n### Thread {}{}\n\n```text",
            thread.id,
            if thread.crashed { " (crashed)" } else { "" }
        );
        for frame in &thread.frames {
            let source = if sanitized {
                Path::new(&frame.source_file)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            } else {
                frame.source_file.as_str().into()
            };
            let function = if frame.function.is_empty() {
                "Not available"
            } else {
                &frame.function
            };
            let _ = writeln!(
                text,
                "{:>3} {}!{} {} {}:{} [{}]",
                frame.index,
                frame.module,
                function,
                frame.instruction,
                source,
                frame.source_line.unwrap_or_default(),
                frame.trust
            );
        }
        text.push_str("```\n");
    }
    let _ = write!(text, "\n## Investigator notes\n\n{notes}\n");
    text
}

fn display_tags(tags: &str) -> &str {
    if tags.trim().is_empty() {
        "None"
    } else {
        tags.trim()
    }
}

pub fn render_stack(report: &DumpReport) -> String {
    let mut text = String::new();
    for thread in &report.threads {
        let _ = writeln!(
            text,
            "Thread {}{}",
            thread.id,
            if thread.crashed { " (crashed)" } else { "" }
        );
        for frame in &thread.frames {
            let function = if frame.function.is_empty() {
                "Not available"
            } else {
                &frame.function
            };
            let _ = writeln!(
                text,
                "  {:>3} {}!{} {}",
                frame.index, frame.module, function, frame.instruction
            );
        }
        text.push('\n');
    }
    text
}

fn write_export(path: &Path, contents: &[u8]) -> Result<PathBuf> {
    write_file(path, contents)?;
    Ok(path.to_owned())
}

fn write_file(path: &Path, contents: &[u8]) -> Result<()> {
    std::fs::write(path, contents).with_context(|| format!("Could not write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{FrameRow, ThreadRow};

    fn report() -> DumpReport {
        DumpReport {
            path: PathBuf::from("/private/build/secret/crash.dmp"),
            crash_reason: "SIGSEGV".into(),
            crash_address: "0x0".into(),
            operating_system: crate::domain::OperatingSystem::Linux,
            architecture: crate::domain::CpuArchitecture::X86_64,
            threads: vec![ThreadRow {
                id: 1,
                name: "main".into(),
                stack_start: "0x1000".into(),
                stack_size: 32,
                crashed: true,
                frames: vec![FrameRow {
                    module: "app".into(),
                    function: "crash".into(),
                    source_file: "/private/build/secret/src/main.rs".into(),
                    source_line: Some(42),
                    trust: "cfi".into(),
                    ..Default::default()
                }],
            }],
            ..Default::default()
        }
    }

    #[test]
    fn sanitized_markdown_removes_parent_paths() {
        let text = render_markdown(&report(), "notes", "New", "renderer", true);
        assert!(text.contains("`crash.dmp`"));
        assert!(text.contains("main.rs:42"));
        assert!(!text.contains("/private/build/secret"));
    }

    #[test]
    fn plain_stack_marks_crashing_thread() {
        let text = render_stack(&report());
        assert!(text.contains("Thread 1 (crashed)"));
        assert!(text.contains("app!crash"));
    }

    #[test]
    fn bundle_contains_readme_and_processor_json() {
        let root = std::env::temp_dir().join(format!("crashlens-export-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let mut report = report();
        report.path = root.join("sample.dmp");
        report.processor_json = "{\"status\":\"OK\"}".into();
        let bundle = root.join("sample-investigation");
        let exported =
            export_bundle_to(&bundle, &report, "investigated", "Resolved", "renderer").unwrap();
        assert_eq!(exported, bundle);
        assert!(
            std::fs::read_to_string(bundle.join("README.md"))
                .unwrap()
                .contains("Resolved")
        );
        assert_eq!(
            std::fs::read_to_string(bundle.join("analysis.json")).unwrap(),
            report.processor_json
        );
        assert!(bundle.join("report.md").is_file());
        assert!(bundle.join("stack.txt").is_file());
        assert!(
            std::fs::read_to_string(bundle.join("README.md"))
                .unwrap()
                .contains("renderer")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn markdown_export_writes_rendered_report() {
        let directory = tempfile::tempdir().unwrap();
        let mut report = report();
        report.path = directory.path().join("sample.dmp");
        let destination = directory.path().join("chosen-name.md");
        let path = export_markdown_to(
            &destination,
            &report,
            "important note",
            "Investigating",
            "startup, renderer",
            false,
        )
        .unwrap();
        let contents = std::fs::read_to_string(&path).unwrap();
        assert_eq!(path, destination);
        assert!(contents.contains("important note"));
        assert!(contents.contains("Investigating"));
        assert!(contents.contains("startup, renderer"));
    }

    #[test]
    fn export_reports_unwritable_destination() {
        let mut report = report();
        report.path = PathBuf::from("/path/that/does/not/exist/sample.dmp");
        let destination = PathBuf::from("/path/that/does/not/exist/sample-analysis.json");
        let error = export_json_to(&destination, &report)
            .unwrap_err()
            .to_string();
        assert!(error.contains("Could not write"));
        assert!(error.contains("sample-analysis.json"));
    }
}

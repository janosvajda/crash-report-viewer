//! Minidump ingestion and stack walking.
//!
//! Parser-specific values stop at this module. The application receives a typed
//! report and does not depend on minidump crate structures.

use crate::domain::{
    CpuArchitecture, DumpReport, FrameRow, MemoryPermissions, MemoryRow, ModuleOwnership,
    ModuleRow, OperatingSystem, PointerSize, StreamRow, SymbolConfig, SymbolStatus, ThreadRow,
    VirtualAddress,
};
use anyhow::{Context, Result};
use minidump::system_info::{Cpu, Os, PointerWidth};
use minidump::{
    Minidump, MinidumpException, MinidumpMemoryInfoList, MinidumpMemoryList, MinidumpModuleList,
    MinidumpSystemInfo, MinidumpThreadList, Module,
};
use std::path::Path;

/// Parse one dump and enrich it with stacks using the configured symbol sources.
///
/// This performs blocking I/O and stack walking, so callers must keep it off the
/// egui update thread.
pub fn analyse(path: &Path, symbols: &SymbolConfig) -> Result<DumpReport> {
    let dump = Minidump::read_path(path)
        .with_context(|| format!("Could not parse {} as a minidump", path.display()))?;
    let mut report = DumpReport {
        path: path.to_owned(),
        file_size: std::fs::metadata(path)?.len(),
        format: format!("{:?} endian minidump", dump.endian),
        architecture: CpuArchitecture::Unknown,
        operating_system: OperatingSystem::Unknown,
        crash_reason: "No exception stream".into(),
        crash_address: VirtualAddress::UNKNOWN,
        ..Default::default()
    };

    let system = dump.get_stream::<MinidumpSystemInfo>().ok();
    if let Some(info) = &system {
        report.architecture = match info.cpu {
            Cpu::X86 => CpuArchitecture::X86,
            Cpu::X86_64 => CpuArchitecture::X86_64,
            Cpu::Ppc => CpuArchitecture::Ppc,
            Cpu::Ppc64 => CpuArchitecture::Ppc64,
            Cpu::Sparc => CpuArchitecture::Sparc,
            Cpu::Arm => CpuArchitecture::Arm,
            Cpu::Arm64 => CpuArchitecture::Arm64,
            Cpu::Mips => CpuArchitecture::Mips,
            Cpu::Mips64 => CpuArchitecture::Mips64,
            Cpu::Unknown(_) => CpuArchitecture::Unknown,
            _ => CpuArchitecture::Unknown,
        };
        report.pointer_size = match info.cpu.pointer_width() {
            PointerWidth::Bits32 => Some(PointerSize::Bytes4),
            PointerWidth::Bits64 => Some(PointerSize::Bytes8),
            PointerWidth::Unknown => None,
        };
        report.operating_system = match info.os {
            Os::Windows => OperatingSystem::Windows,
            Os::MacOs => OperatingSystem::MacOs,
            Os::Ios => OperatingSystem::Ios,
            Os::Linux => OperatingSystem::Linux,
            Os::Solaris => OperatingSystem::Solaris,
            Os::Android => OperatingSystem::Android,
            Os::Ps3 => OperatingSystem::Ps3,
            Os::NaCl => OperatingSystem::NaCl,
            Os::Unknown(_) => OperatingSystem::Unknown,
        };
        if report.pointer_size.is_none() {
            report.diagnostics.push(format!(
                "Pointer decoding is unavailable for architecture {}",
                report.architecture
            ));
        }
    } else {
        report
            .diagnostics
            .push("System information stream is missing".into());
    }

    if let Ok(exception) = dump.get_stream::<MinidumpException>() {
        report.crash_thread = Some(exception.get_crashing_thread_id());
        if let Some(info) = &system {
            report.crash_reason = exception.get_crash_reason(info.os, info.cpu).to_string();
            report.crash_address =
                VirtualAddress::new(exception.get_crash_address(info.os, info.cpu));
        }
    } else {
        report
            .diagnostics
            .push("No exception stream; this may be a manually captured dump".into());
    }

    if let Ok(threads) = dump.get_stream::<MinidumpThreadList>() {
        report.threads = threads
            .threads
            .iter()
            .map(|thread| {
                let start = thread.raw.stack.start_of_memory_range;
                ThreadRow {
                    id: thread.raw.thread_id,
                    name: String::new(),
                    stack_start: VirtualAddress::new(start),
                    stack_size: thread.raw.stack.memory.data_size as u64,
                    crashed: report.crash_thread == Some(thread.raw.thread_id),
                    frames: Vec::new(),
                }
            })
            .collect();
    } else {
        report
            .diagnostics
            .push("Thread list stream is missing".into());
    }

    if let Ok(modules) = dump.get_stream::<MinidumpModuleList>() {
        report.modules = modules
            .iter()
            .map(|module| ModuleRow {
                name: module.code_file().to_string(),
                base: VirtualAddress::new(module.raw.base_of_image),
                size: module.raw.size_of_image as u64,
                code_id: module
                    .code_identifier()
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "—".into()),
                symbol_status: SymbolStatus::NotReferenced,
                ownership: classify_module(module.code_file().as_ref()),
                contains_fault: false,
            })
            .collect();
    } else {
        report
            .diagnostics
            .push("Module list stream is missing".into());
    }

    let memory_info = dump.get_stream::<MinidumpMemoryInfoList>().ok();
    if let Ok(memory) = dump.get_stream::<MinidumpMemoryList>() {
        report.memory = memory
            .iter()
            .map(|region| {
                let info = memory_info
                    .as_ref()
                    .and_then(|list| list.memory_info_at_address(region.base_address));
                MemoryRow {
                    start: VirtualAddress::new(region.base_address),
                    size: region.size,
                    preview: hex_preview(region.bytes, region.base_address),
                    bytes: region.bytes.to_vec(),
                    permissions: info.map_or(MemoryPermissions::NotRecorded, |info| {
                        MemoryPermissions::Recorded {
                            readable: info.is_readable(),
                            writable: info.is_writable(),
                            executable: info.is_executable(),
                        }
                    }),
                    mapping_type: info
                        .map(|info| format!("{:?} · {:?}", info.ty, info.state))
                        .unwrap_or_else(|| "Not recorded".into()),
                }
            })
            .collect();
    }

    report.streams = dump
        .all_streams()
        .map(|entry| StreamRow {
            kind: format!("{:?}", entry.stream_type),
            size: entry.location.data_size,
            rva: entry.location.rva,
        })
        .collect();
    add_call_stacks(&dump, &mut report, symbols);
    if let Some(fault) = report.crash_address.value() {
        for module in &mut report.modules {
            if let Some(base) = module.base.value() {
                module.contains_fault = (base..base.saturating_add(module.size)).contains(&fault);
            }
        }
    }
    Ok(report)
}

fn classify_module(name: &str) -> ModuleOwnership {
    let lower = name.to_ascii_lowercase();
    if lower.starts_with("/system/")
        || lower.contains("\\windows\\system32\\")
        || lower.starts_with("/usr/lib/")
    {
        ModuleOwnership::System
    } else {
        ModuleOwnership::ApplicationOrThirdParty
    }
}

fn hex_preview(bytes: &[u8], base: u64) -> String {
    // Bound the inline preview; complete captured bytes remain in `MemoryRow`
    // for on-demand inspection by the memory feature.
    use std::fmt::Write as _;
    let mut output = String::with_capacity(bytes.len().min(128) * 4);
    for (line, chunk) in bytes.chunks(16).take(8).enumerate() {
        if line > 0 {
            output.push('\n');
        }
        let _ = write!(output, "{:016x}  ", base + (line * 16) as u64);
        for index in 0..16 {
            if let Some(byte) = chunk.get(index) {
                let _ = write!(output, "{byte:02x} ");
            } else {
                output.push_str("   ");
            }
        }
        output.push(' ');
        for byte in chunk {
            output.push(if byte.is_ascii_graphic() {
                *byte as char
            } else {
                '.'
            });
        }
    }
    output
}

fn add_call_stacks<T>(dump: &Minidump<'_, T>, report: &mut DumpReport, config: &SymbolConfig)
where
    T: std::ops::Deref<Target = [u8]>,
{
    use minidump_unwind::{Symbolizer, http_symbol_supplier, simple_symbol_supplier};

    // Analysis already runs on a dedicated worker. A current-thread runtime is
    // sufficient for the async stack walker and avoids a process-wide runtime.
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread().build() else {
        report
            .diagnostics
            .push("Could not start the stack unwinder".into());
        return;
    };
    let state = if config.server_urls.is_empty() {
        let symbolizer = Symbolizer::new(simple_symbol_supplier(config.local_paths.clone()));
        runtime.block_on(minidump_processor::process_minidump(dump, &symbolizer))
    } else {
        let cache = config.cache_dir.clone();
        let temporary = cache.join("tmp");
        let _ = std::fs::create_dir_all(&temporary);
        let symbolizer = Symbolizer::new(http_symbol_supplier(
            config.local_paths.clone(),
            config.server_urls.clone(),
            cache,
            temporary,
            std::time::Duration::from_secs(30),
        ));
        runtime.block_on(minidump_processor::process_minidump(dump, &symbolizer))
    };
    let Ok(state) = state else {
        report
            .diagnostics
            .push("Stack unwinding was not available for this dump".into());
        return;
    };
    let mut output = Vec::new();
    if state.print_json(&mut output, false).is_err() {
        return;
    }
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(&output) else {
        return;
    };
    report.processor_json = String::from_utf8_lossy(&output).into_owned();
    let Some(threads) = json.get("threads").and_then(|value| value.as_array()) else {
        return;
    };
    for thread in threads {
        let Some(id) = thread.get("thread_id").and_then(|value| value.as_u64()) else {
            continue;
        };
        let Some(row) = report.threads.iter_mut().find(|row| row.id == id as u32) else {
            continue;
        };
        row.name = thread
            .get("thread_name")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_owned();
        row.frames = thread
            .get("frames")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
            .map(frame_from_json)
            .collect();
    }
    for module in &mut report.modules {
        let frames: Vec<_> = report
            .threads
            .iter()
            .flat_map(|thread| &thread.frames)
            .filter(|frame| frame.module == module.name || module.name.ends_with(&frame.module))
            .collect();
        module.symbol_status = if frames.is_empty() {
            SymbolStatus::NotReferenced
        } else if frames.iter().any(|frame| frame.missing_symbols) {
            SymbolStatus::Missing
        } else if frames.iter().any(|frame| !frame.function.is_empty()) {
            SymbolStatus::Loaded
        } else {
            SymbolStatus::AddressesOnly
        };
    }
}

fn frame_from_json(frame: &serde_json::Value) -> FrameRow {
    let text = |name: &str| {
        frame
            .get(name)
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_owned()
    };
    FrameRow {
        index: frame
            .get("frame")
            .and_then(|value| value.as_u64())
            .unwrap_or(0),
        instruction: text("offset").into(),
        module: text("module"),
        function: text("function"),
        function_offset: text("function_offset"),
        source_file: text("file"),
        source_line: frame.get("line").and_then(|value| value.as_u64()),
        trust: text("trust"),
        missing_symbols: frame
            .get("missing_symbols")
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
        registers: frame
            .get("registers")
            .and_then(|value| value.as_object())
            .map(|registers| {
                registers
                    .iter()
                    .map(|(name, value)| {
                        (name.clone(), value.as_str().unwrap_or_default().to_owned())
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

pub fn human_bytes(value: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut number = value as f64;
    let mut unit = 0;
    while number >= 1024.0 && unit < UNITS.len() - 1 {
        number /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", value, UNITS[unit])
    } else {
        format!("{number:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_file_sizes() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(1023), "1023 B");
        assert_eq!(human_bytes(1024), "1.0 KB");
        assert_eq!(human_bytes(1_572_864), "1.5 MB");
        assert_eq!(human_bytes(1_073_741_824), "1.0 GB");
    }

    #[test]
    fn classifies_system_and_application_modules() {
        assert_eq!(
            classify_module("/System/Library/AppKit"),
            ModuleOwnership::System
        );
        assert_eq!(
            classify_module("C:\\Windows\\System32\\kernel32.dll"),
            ModuleOwnership::System
        );
        assert_eq!(
            classify_module("/Applications/MyApp/app.dll"),
            ModuleOwnership::ApplicationOrThirdParty
        );
    }

    #[test]
    fn hex_preview_contains_addresses_hex_and_ascii() {
        let preview = hex_preview(b"Hello\0world", 0x1000);
        assert!(preview.starts_with("0000000000001000"));
        assert!(preview.contains("48 65 6c 6c 6f"));
        assert!(preview.contains("Hello.world"));
    }

    #[test]
    fn parses_processor_frames_with_optional_fields() {
        let value = serde_json::json!({
            "frame": 2,
            "offset": "0x1234",
            "module": "app",
            "function": "crash",
            "file": "src/main.rs",
            "line": 42,
            "trust": "cfi",
            "registers": { "rip": "0x1234" }
        });
        let frame = frame_from_json(&value);
        assert_eq!(frame.index, 2);
        assert_eq!(frame.function, "crash");
        assert_eq!(frame.source_line, Some(42));
        assert_eq!(frame.registers, vec![("rip".into(), "0x1234".into())]);
    }

    #[test]
    fn rejects_non_minidump_files() {
        let path =
            std::env::temp_dir().join(format!("crashlens-invalid-{}.dmp", std::process::id()));
        std::fs::write(&path, b"not a minidump").unwrap();
        let result = analyse(&path, &SymbolConfig::default());
        let _ = std::fs::remove_file(path);
        assert!(result.is_err());
    }
}

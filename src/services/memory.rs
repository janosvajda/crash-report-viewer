//! Deterministic analysis of bytes actually captured in a minidump.
//!
//! This is evidence analysis, not heap reconstruction. Ordinary minidumps often
//! omit allocator metadata and much of the address space, so results are limited
//! to recorded regions, stacks, modules, and registers.

use crate::domain::{DumpReport, MemoryRow, VirtualAddress};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
/// A pointer-sized value decoded from a captured memory region.
pub struct DecodedPointer {
    pub address: u64,
    pub value: u64,
    pub target: AddressTarget,
    pub chain: Vec<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// A crashed-frame register related to known dump data.
pub struct RegisterFinding {
    pub register: String,
    pub value: String,
    pub target: RegisterTarget,
    pub suspicious: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
/// The strongest known owner of an address found in captured memory.
pub enum AddressTarget {
    Null,
    ThreadStack { thread_id: u32 },
    Module { name: String, offset: u64 },
    CapturedRegion { start: VirtualAddress },
}

impl fmt::Display for AddressTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => formatter.write_str("NULL pointer"),
            Self::ThreadStack { thread_id } => write!(formatter, "thread {thread_id} stack"),
            Self::Module { name, offset } => write!(formatter, "{name} + 0x{offset:x}"),
            Self::CapturedRegion { start } => write!(formatter, "captured region {start}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegisterTarget {
    Known(AddressTarget),
    Unmapped,
    Invalid,
}

impl fmt::Display for RegisterTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Known(target) => target.fmt(formatter),
            Self::Unmapped => formatter.write_str("Not mapped in captured memory or modules"),
            Self::Invalid => formatter.write_str("Not a hexadecimal address"),
        }
    }
}

/// Input accepted by address-analysis helpers.
///
/// New domain code should use `VirtualAddress`; string implementations remain
/// for values originating in third-party processor output.
pub trait AddressInput {
    fn address(self) -> Option<u64>;
}

impl AddressInput for &str {
    fn address(self) -> Option<u64> {
        self.parse::<VirtualAddress>().ok()?.value()
    }
}

impl AddressInput for &String {
    fn address(self) -> Option<u64> {
        self.as_str().address()
    }
}

impl AddressInput for &VirtualAddress {
    fn address(self) -> Option<u64> {
        self.value()
    }
}

pub fn parse_address(value: impl AddressInput) -> Option<u64> {
    value.address()
}

pub fn contains(region: &MemoryRow, address: Option<u64>) -> bool {
    let Some(start) = parse_address(&region.start) else {
        return false;
    };
    address.is_some_and(|address| address >= start && address < start.saturating_add(region.size))
}

pub fn region_roles(report: &DumpReport, region: &MemoryRow) -> Vec<String> {
    let mut roles = Vec::new();
    if contains(region, parse_address(&report.crash_address)) {
        roles.push("Contains fault address".into());
    }
    for thread in &report.threads {
        if ranges_overlap(
            region,
            parse_address(&thread.stack_start),
            thread.stack_size,
        ) {
            roles.push(format!(
                "Stack for thread {}{}",
                thread.id,
                if thread.crashed { " (crashed)" } else { "" }
            ));
        }
    }
    for module in &report.modules {
        if ranges_overlap(region, parse_address(&module.base), module.size) {
            roles.push(format!("Mapped to module {}", module.name));
        }
    }
    roles
}

pub fn extract_strings(bytes: &[u8], limit: usize) -> Vec<String> {
    let mut found = Vec::new();
    let mut ascii = Vec::new();
    for &byte in bytes {
        if byte.is_ascii_graphic() || byte == b' ' {
            ascii.push(byte);
        } else {
            push_string(&mut found, &ascii, "ASCII", limit);
            ascii.clear();
        }
        if found.len() >= limit {
            return found;
        }
    }
    push_string(&mut found, &ascii, "ASCII", limit);

    let mut utf16 = Vec::new();
    for pair in bytes.chunks_exact(2) {
        let value = u16::from_le_bytes([pair[0], pair[1]]);
        if (0x20..=0x7e).contains(&value) {
            utf16.push(value as u8);
        } else {
            push_string(&mut found, &utf16, "UTF-16", limit);
            utf16.clear();
        }
        if found.len() >= limit {
            return found;
        }
    }
    push_string(&mut found, &utf16, "UTF-16", limit);
    found.truncate(limit);
    found
}

fn push_string(output: &mut Vec<String>, bytes: &[u8], encoding: &str, limit: usize) {
    if bytes.len() >= 4 && output.len() < limit {
        output.push(format!("{encoding} · {}", String::from_utf8_lossy(bytes)));
    }
}

pub fn decoded_pointers(
    report: &DumpReport,
    region: &MemoryRow,
    limit: usize,
) -> Vec<DecodedPointer> {
    // Never infer width from the host: a 32-bit dump may be inspected on a
    // 64-bit machine, and an unknown width makes every decoded value suspect.
    let Some(width) = report.pointer_size.map(|size| size.bytes()) else {
        return Vec::new();
    };
    let Some(start) = parse_address(&region.start) else {
        return Vec::new();
    };
    region
        .bytes
        .chunks_exact(width)
        .enumerate()
        .filter_map(|(index, bytes)| {
            let value = decode_little_endian(bytes);
            if value == 0 {
                return None;
            }
            let target = classify_address(report, value)?;
            Some(DecodedPointer {
                address: start + (index * width) as u64,
                value,
                target,
                chain: follow_pointer_chain(report, value, width, 3),
            })
        })
        .take(limit)
        .collect()
}

fn follow_pointer_chain(
    report: &DumpReport,
    first_target: u64,
    width: usize,
    max_depth: usize,
) -> Vec<u64> {
    let mut chain = vec![first_target];
    let mut address = first_target;
    // Corrupted structures commonly contain cycles. Bound depth and stop when
    // an address repeats so a single region cannot monopolise analysis time.
    for _ in 1..max_depth {
        let Some(next) = read_pointer(report, address, width) else {
            break;
        };
        if next == 0 || chain.contains(&next) || classify_address(report, next).is_none() {
            break;
        }
        chain.push(next);
        address = next;
    }
    chain
}

fn read_pointer(report: &DumpReport, address: u64, width: usize) -> Option<u64> {
    let region = report
        .memory
        .iter()
        .find(|region| contains(region, Some(address)))?;
    let start = parse_address(&region.start)?;
    let offset = usize::try_from(address - start).ok()?;
    let bytes = region.bytes.get(offset..offset.checked_add(width)?)?;
    Some(decode_little_endian(bytes))
}

pub fn classify_address(report: &DumpReport, address: u64) -> Option<AddressTarget> {
    if address == 0 {
        return Some(AddressTarget::Null);
    }
    for thread in &report.threads {
        let Some(start) = parse_address(&thread.stack_start) else {
            continue;
        };
        if (start..start.saturating_add(thread.stack_size)).contains(&address) {
            return Some(AddressTarget::ThreadStack {
                thread_id: thread.id,
            });
        }
    }
    for module in &report.modules {
        let Some(base) = parse_address(&module.base) else {
            continue;
        };
        if (base..base.saturating_add(module.size)).contains(&address) {
            let name = std::path::Path::new(&module.name)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            return Some(AddressTarget::Module {
                name: name.into_owned(),
                offset: address - base,
            });
        }
    }
    report
        .memory
        .iter()
        .find(|region| contains(region, Some(address)))
        .map(|region| AddressTarget::CapturedRegion {
            start: region.start,
        })
}

pub fn register_findings(report: &DumpReport) -> Vec<RegisterFinding> {
    let Some(frame) = report
        .threads
        .iter()
        .find(|thread| thread.crashed)
        .and_then(|thread| thread.frames.first())
    else {
        return Vec::new();
    };
    frame
        .registers
        .iter()
        .map(|(register, value)| {
            let address = parse_address(value);
            let target = match address {
                Some(address) => classify_address(report, address)
                    .map(RegisterTarget::Known)
                    .unwrap_or(RegisterTarget::Unmapped),
                None => RegisterTarget::Invalid,
            };
            let suspicious = address.is_some_and(|address| address < 4096)
                || matches!(target, RegisterTarget::Unmapped | RegisterTarget::Invalid)
                || matches!(target, RegisterTarget::Known(AddressTarget::Null));
            RegisterFinding {
                register: register.clone(),
                value: value.clone(),
                target,
                suspicious,
            }
        })
        .collect()
}

pub fn matches_query(region: &MemoryRow, query: &str) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return true;
    }
    if region
        .start
        .to_string()
        .to_ascii_lowercase()
        .contains(&query.to_ascii_lowercase())
    {
        return true;
    }
    if let Some(address) = parse_address(query)
        && contains(region, Some(address))
    {
        return true;
    }
    let needle = query.as_bytes();
    region
        .bytes
        .windows(needle.len().max(1))
        .any(|window| window.eq_ignore_ascii_case(needle))
}

fn ranges_overlap(region: &MemoryRow, other_start: Option<u64>, other_size: u64) -> bool {
    let (Some(start), Some(other_start)) = (parse_address(&region.start), other_start) else {
        return false;
    };
    start < other_start.saturating_add(other_size)
        && other_start < start.saturating_add(region.size)
}

fn decode_little_endian(bytes: &[u8]) -> u64 {
    // The analyzer currently supports little-endian pointer encodings. Keeping
    // this conversion local makes a future byte-order field straightforward.
    bytes.iter().enumerate().fold(0, |value, (shift, byte)| {
        value | ((*byte as u64) << (shift * 8))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        FrameRow, ModuleOwnership, ModuleRow, PointerSize, SymbolStatus, ThreadRow,
    };

    fn region(start: &str, bytes: Vec<u8>) -> MemoryRow {
        MemoryRow {
            start: start.into(),
            size: bytes.len() as u64,
            preview: String::new(),
            bytes,
            permissions: crate::domain::MemoryPermissions::Recorded {
                readable: true,
                writable: true,
                executable: false,
            },
            mapping_type: "Private".into(),
        }
    }

    #[test]
    fn extracts_ascii_and_utf16_strings() {
        let mut bytes = b"hello world\0".to_vec();
        bytes.extend("wide".encode_utf16().flat_map(u16::to_le_bytes));
        let strings = extract_strings(&bytes, 10);
        assert!(strings.iter().any(|value| value.contains("hello world")));
        assert!(strings.iter().any(|value| value.contains("wide")));
    }

    #[test]
    fn decodes_and_classifies_pointer_targets() {
        let pointer = 0x2008_u64.to_le_bytes().to_vec();
        let report = DumpReport {
            architecture: crate::domain::CpuArchitecture::X86_64,
            pointer_size: Some(PointerSize::Bytes8),
            memory: vec![
                region("0x1000", pointer.clone()),
                region("0x2000", vec![0; 32]),
            ],
            ..Default::default()
        };
        let pointers = decoded_pointers(&report, &report.memory[0], 10);
        assert_eq!(pointers[0].value, 0x2008);
        assert!(matches!(
            pointers[0].target,
            AddressTarget::CapturedRegion { start } if start == VirtualAddress::new(0x2000)
        ));
        assert_eq!(pointers[0].chain, vec![0x2008]);
    }

    #[test]
    fn decodes_32_bit_pointers_using_report_metadata() {
        let report = DumpReport {
            pointer_size: Some(PointerSize::Bytes4),
            memory: vec![
                region("0x1000", 0x2008_u32.to_le_bytes().to_vec()),
                region("0x2000", vec![0; 32]),
            ],
            ..Default::default()
        };

        let pointers = decoded_pointers(&report, &report.memory[0], 10);

        assert_eq!(pointers.len(), 1);
        assert_eq!(pointers[0].value, 0x2008);
    }

    #[test]
    fn unknown_pointer_width_disables_decoding() {
        let report = DumpReport {
            memory: vec![region("0x1000", 0x2008_u64.to_le_bytes().to_vec())],
            ..Default::default()
        };

        assert!(decoded_pointers(&report, &report.memory[0], 10).is_empty());
    }

    #[test]
    fn zero_filled_memory_is_not_reported_as_pointers() {
        let report = DumpReport {
            architecture: crate::domain::CpuArchitecture::Arm64,
            pointer_size: Some(PointerSize::Bytes8),
            memory: vec![region("0x1000", vec![0; 64])],
            ..Default::default()
        };
        assert!(decoded_pointers(&report, &report.memory[0], 10).is_empty());
    }

    #[test]
    fn identifies_stack_and_module_roles() {
        let memory = region("0x1000", vec![0; 64]);
        let report = DumpReport {
            threads: vec![ThreadRow {
                id: 7,
                name: String::new(),
                stack_start: "0x1000".into(),
                stack_size: 32,
                crashed: true,
                frames: vec![FrameRow::default()],
            }],
            modules: vec![ModuleRow {
                name: "app".into(),
                base: "0x1020".into(),
                size: 32,
                code_id: String::new(),
                symbol_status: SymbolStatus::NotReferenced,
                ownership: ModuleOwnership::ApplicationOrThirdParty,
                contains_fault: false,
            }],
            ..Default::default()
        };
        let roles = region_roles(&report, &memory);
        assert!(roles.iter().any(|role| role.contains("thread 7")));
        assert!(roles.iter().any(|role| role.contains("module app")));
    }

    #[test]
    fn searches_addresses_and_text_bytes() {
        let memory = region("0x1000", b"secret token".to_vec());
        assert!(matches_query(&memory, "0x1005"));
        assert!(matches_query(&memory, "TOKEN"));
        assert!(!matches_query(&memory, "missing"));
    }
}

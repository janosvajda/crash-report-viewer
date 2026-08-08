use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Clone, Debug, Default)]
pub struct SymbolConfig {
    pub local_paths: Vec<PathBuf>,
    pub server_urls: Vec<String>,
    pub cache_dir: PathBuf,
}

#[derive(Clone, Debug, Default)]
pub struct DumpReport {
    pub path: PathBuf,
    pub file_size: u64,
    pub format: String,
    pub architecture: CpuArchitecture,
    pub pointer_size: Option<PointerSize>,
    pub operating_system: OperatingSystem,
    pub crash_reason: String,
    pub crash_address: VirtualAddress,
    pub crash_thread: Option<u32>,
    pub threads: Vec<ThreadRow>,
    pub modules: Vec<ModuleRow>,
    pub memory: Vec<MemoryRow>,
    pub streams: Vec<StreamRow>,
    pub diagnostics: Vec<String>,
    pub processor_json: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OperatingSystem {
    Windows,
    MacOs,
    Ios,
    Linux,
    Solaris,
    Android,
    Ps3,
    NaCl,
    #[default]
    Unknown,
}

impl fmt::Display for OperatingSystem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Windows => "Windows",
            Self::MacOs => "macOS",
            Self::Ios => "iOS",
            Self::Linux => "Linux",
            Self::Solaris => "Solaris",
            Self::Android => "Android",
            Self::Ps3 => "PlayStation 3",
            Self::NaCl => "Native Client",
            Self::Unknown => "Unknown",
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CpuArchitecture {
    X86,
    X86_64,
    Ppc,
    Ppc64,
    Sparc,
    Arm,
    Arm64,
    Mips,
    Mips64,
    #[default]
    Unknown,
}

impl fmt::Display for CpuArchitecture {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::X86 => "x86",
            Self::X86_64 => "x86-64",
            Self::Ppc => "PowerPC",
            Self::Ppc64 => "PowerPC 64-bit",
            Self::Sparc => "SPARC",
            Self::Arm => "ARM",
            Self::Arm64 => "ARM64",
            Self::Mips => "MIPS",
            Self::Mips64 => "MIPS64",
            Self::Unknown => "Unknown",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerSize {
    Bytes4,
    Bytes8,
}

impl PointerSize {
    pub const fn bytes(self) -> usize {
        match self {
            Self::Bytes4 => 4,
            Self::Bytes8 => 8,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ThreadRow {
    pub id: u32,
    pub name: String,
    pub stack_start: VirtualAddress,
    pub stack_size: u64,
    pub crashed: bool,
    pub frames: Vec<FrameRow>,
}

#[derive(Clone, Debug, Default)]
pub struct FrameRow {
    pub index: u64,
    pub instruction: VirtualAddress,
    pub module: String,
    pub function: String,
    pub function_offset: String,
    pub source_file: String,
    pub source_line: Option<u64>,
    pub trust: String,
    pub missing_symbols: bool,
    pub registers: Vec<(String, String)>,
}

#[derive(Clone, Debug)]
pub struct ModuleRow {
    pub name: String,
    pub base: VirtualAddress,
    pub size: u64,
    pub code_id: String,
    pub symbol_status: SymbolStatus,
    pub ownership: ModuleOwnership,
    pub contains_fault: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ModuleOwnership {
    System,
    #[default]
    ApplicationOrThirdParty,
}

impl fmt::Display for ModuleOwnership {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::System => "System",
            Self::ApplicationOrThirdParty => "Application / third-party",
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SymbolStatus {
    #[default]
    NotReferenced,
    Missing,
    Loaded,
    AddressesOnly,
}

impl SymbolStatus {
    pub const fn is_missing(self) -> bool {
        matches!(self, Self::Missing)
    }
}

impl fmt::Display for SymbolStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotReferenced => "Not used by recovered frames",
            Self::Missing => "Missing symbols",
            Self::Loaded => "Symbols loaded",
            Self::AddressesOnly => "Addresses only",
        })
    }
}

#[derive(Clone, Debug)]
pub struct MemoryRow {
    pub start: VirtualAddress,
    pub size: u64,
    pub preview: String,
    pub bytes: Vec<u8>,
    pub permissions: MemoryPermissions,
    pub mapping_type: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MemoryPermissions {
    Recorded {
        readable: bool,
        writable: bool,
        executable: bool,
    },
    #[default]
    NotRecorded,
}

impl fmt::Display for MemoryPermissions {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Recorded {
                readable,
                writable,
                executable,
            } => write!(
                formatter,
                "{}{}{}",
                if *readable { 'R' } else { '-' },
                if *writable { 'W' } else { '-' },
                if *executable { 'X' } else { '-' }
            ),
            Self::NotRecorded => formatter.write_str("Not recorded"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct StreamRow {
    pub kind: String,
    pub size: u32,
    pub rva: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct VirtualAddress(Option<u64>);

impl VirtualAddress {
    pub const UNKNOWN: Self = Self(None);

    pub const fn new(value: u64) -> Self {
        Self(Some(value))
    }

    pub const fn value(self) -> Option<u64> {
        self.0
    }
}

impl fmt::Display for VirtualAddress {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(value) => write!(formatter, "0x{value:016x}"),
            None => formatter.write_str("—"),
        }
    }
}

impl FromStr for VirtualAddress {
    type Err = std::num::ParseIntError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        u64::from_str_radix(value.trim().trim_start_matches("0x"), 16).map(Self::new)
    }
}

impl From<&str> for VirtualAddress {
    fn from(value: &str) -> Self {
        value.parse().unwrap_or(Self::UNKNOWN)
    }
}

impl From<String> for VirtualAddress {
    fn from(value: String) -> Self {
        Self::from(value.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::{MemoryPermissions, VirtualAddress};

    #[test]
    fn virtual_address_round_trips_without_losing_numeric_value() {
        let address: VirtualAddress = "0x1234abcd".into();

        assert_eq!(address.value(), Some(0x1234_abcd));
        assert_eq!(address.to_string(), "0x000000001234abcd");
        assert_eq!(VirtualAddress::from("invalid"), VirtualAddress::UNKNOWN);
    }

    #[test]
    fn memory_permissions_have_a_stable_display_form() {
        let permissions = MemoryPermissions::Recorded {
            readable: true,
            writable: false,
            executable: true,
        };

        assert_eq!(permissions.to_string(), "R-X");
        assert_eq!(MemoryPermissions::NotRecorded.to_string(), "Not recorded");
    }
}

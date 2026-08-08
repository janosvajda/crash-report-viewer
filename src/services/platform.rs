//! Small operating-system integration points kept out of egui screens.

use anyhow::{Context, Result, bail};
use std::{path::Path, process::Command};

/// Ask the host desktop to open or reveal a filesystem path.
pub fn open_path(path: &Path) -> Result<()> {
    let mut command = platform_open_command(path);
    let status = command
        .status()
        .with_context(|| format!("Could not open {}", path.display()))?;
    if !status.success() {
        bail!("The operating system could not open {}", path.display());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn platform_open_command(path: &Path) -> Command {
    let mut command = Command::new("open");
    command.arg(path);
    command
}

#[cfg(target_os = "windows")]
fn platform_open_command(path: &Path) -> Command {
    let mut command = Command::new("explorer");
    command.arg(path);
    command
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn platform_open_command(path: &Path) -> Command {
    let mut command = Command::new("xdg-open");
    command.arg(path);
    command
}

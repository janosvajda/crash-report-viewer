//! Navigation and cross-feature requests returned by otherwise local screens.

use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
/// Screens request effects instead of mutating the application coordinator or
/// performing navigation themselves.
pub enum ScreenAction {
    OpenThreads,
    OpenModules,
    ConfigureSymbols,
    OpenModule(String),
    ReanalyseWithSymbols,
    LoadComparison,
    OpenPath(PathBuf),
}

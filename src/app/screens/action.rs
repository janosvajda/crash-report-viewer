#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScreenAction {
    OpenThreads,
    OpenModules,
    ConfigureSymbols,
    OpenModule(String),
    ReanalyseWithSymbols,
    LoadComparison,
    OpenPath(PathBuf),
}
use std::path::PathBuf;

use crate::{
    domain::{DumpReport, SymbolConfig},
    services::{analyzer, scanner::FileEntry},
};
use std::{
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
};

pub type AnalysisResult = Result<DumpReport, String>;

/// Owns the lifecycle of one background analysis. Keeping the channel and its
/// subject together prevents stale paths and receivers from drifting apart.
pub struct AnalysisJob {
    pub path: PathBuf,
    receiver: Receiver<AnalysisResult>,
}

impl AnalysisJob {
    pub fn new(path: PathBuf, receiver: Receiver<AnalysisResult>) -> Self {
        Self { path, receiver }
    }

    pub fn poll(&self) -> Option<AnalysisResult> {
        self.receiver.try_recv().ok()
    }

    pub fn spawn(path: PathBuf, symbols: SymbolConfig) -> Self {
        let (sender, receiver) = mpsc::channel();
        let worker_path = path.clone();
        std::thread::spawn(move || {
            let result =
                analyzer::analyse(&worker_path, &symbols).map_err(|error| format!("{error:#}"));
            let _ = sender.send(result);
        });
        Self::new(path, receiver)
    }
}

#[derive(Default)]
pub struct LibraryState {
    pub files: Vec<FileEntry>,
    pub scan_location: Option<PathBuf>,
    pub scanned_directories: usize,
}

impl LibraryState {
    pub fn replace(&mut self, mut files: Vec<FileEntry>) {
        files.sort_by(|left, right| right.path.cmp(&left.path));
        self.files = files;
    }

    pub fn begin_scan(&mut self, root: &Path) {
        self.files.clear();
        self.scan_location = Some(root.to_owned());
        self.scanned_directories = 0;
    }

    pub fn found_batch(&mut self, files: Vec<FileEntry>) {
        self.files.extend(files);
    }

    pub fn progress(&mut self, path: PathBuf, directories: usize) {
        self.scan_location = Some(path);
        self.scanned_directories = directories;
    }

    pub fn finish_scan(&mut self) {
        self.files.sort_by(|left, right| left.path.cmp(&right.path));
        self.files.dedup_by(|left, right| left.path == right.path);
        self.scan_location = None;
    }
}

#[derive(Default)]
pub struct SymbolSettings {
    pub local_paths: String,
    pub server_urls: String,
    pub source_root_from: String,
    pub source_root_to: String,
}

impl SymbolSettings {
    pub fn config(&self, home: Option<&Path>) -> SymbolConfig {
        let cache_root = home.map(Path::to_owned).unwrap_or_else(std::env::temp_dir);
        SymbolConfig {
            local_paths: nonempty_lines(&self.local_paths)
                .map(|path| expand_home(path, home))
                .collect(),
            server_urls: nonempty_lines(&self.server_urls)
                .map(str::to_owned)
                .collect(),
            cache_dir: cache_root.join("Library/Caches/CrashLens/symbols"),
        }
    }
}

pub fn expand_home(value: &str, home: Option<&Path>) -> PathBuf {
    if value == "~" {
        return home.map(Path::to_owned).unwrap_or_default();
    }
    if let Some(rest) = value.strip_prefix("~/")
        && let Some(home) = home
    {
        return home.join(rest);
    }
    PathBuf::from(value)
}

fn nonempty_lines(value: &str) -> impl Iterator<Item = &str> {
    value.lines().map(str::trim).filter(|line| !line.is_empty())
}

pub fn set_comparison_selected(selection: &mut Vec<PathBuf>, path: &Path, selected: bool) {
    if selected {
        if selection.len() < 2 && !selection.iter().any(|existing| existing == path) {
            selection.push(path.to_owned());
        }
    } else {
        selection.retain(|existing| existing != path);
    }
}

pub fn take_comparison_pair(selection: &mut Vec<PathBuf>) -> Option<(PathBuf, PathBuf)> {
    if selection.len() != 2 {
        return None;
    }
    let second = selection.pop()?;
    let first = selection.pop()?;
    Some((first, second))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn entry(path: &str) -> FileEntry {
        FileEntry {
            path: path.into(),
            size: 1,
        }
    }

    #[test]
    fn analysis_job_keeps_subject_and_delivers_once() {
        let (sender, receiver) = mpsc::channel();
        let job = AnalysisJob::new("crash.dmp".into(), receiver);
        assert_eq!(job.path, PathBuf::from("crash.dmp"));
        assert!(job.poll().is_none());
        sender.send(Ok(DumpReport::default())).unwrap();
        assert!(job.poll().unwrap().is_ok());
        assert!(job.poll().is_none());
    }

    #[test]
    fn spawned_analysis_reports_parser_failure() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("missing.dmp");
        let job = AnalysisJob::spawn(path.clone(), SymbolConfig::default());
        assert_eq!(job.path, path);
        let result = (0..100)
            .find_map(|_| {
                let result = job.poll();
                if result.is_none() {
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
                result
            })
            .expect("analysis worker did not finish");
        assert!(result.unwrap_err().contains("Could not parse"));
    }

    #[test]
    fn library_scan_deduplicates_and_sorts_results() {
        let mut library = LibraryState::default();
        library.begin_scan(Path::new("/"));
        library.found_batch(vec![entry("/z.dmp"), entry("/a.dmp"), entry("/z.dmp")]);
        library.progress("/Users".into(), 42);
        assert_eq!(library.scanned_directories, 42);
        library.finish_scan();
        assert_eq!(
            library.files.iter().map(|f| &f.path).collect::<Vec<_>>(),
            vec![&PathBuf::from("/a.dmp"), &PathBuf::from("/z.dmp")]
        );
        assert!(library.scan_location.is_none());
    }

    #[test]
    fn batched_scan_results_are_deduplicated_when_finished() {
        let mut library = LibraryState::default();
        library.begin_scan(Path::new("/"));
        library.found_batch(vec![entry("/b.dmp"), entry("/a.dmp"), entry("/b.dmp")]);
        library.finish_scan();
        assert_eq!(library.files.len(), 2);
        assert_eq!(library.files[0].path, PathBuf::from("/a.dmp"));
    }

    #[test]
    fn symbol_config_trims_entries_and_expands_home() {
        let settings = SymbolSettings {
            local_paths: " ~/symbols \n\n /opt/symbols ".into(),
            server_urls: " https://one/ \n\nhttps://two/ ".into(),
            ..Default::default()
        };
        let config = settings.config(Some(Path::new("/Users/test")));
        assert_eq!(
            config.local_paths,
            vec![
                PathBuf::from("/Users/test/symbols"),
                PathBuf::from("/opt/symbols")
            ]
        );
        assert_eq!(config.server_urls, vec!["https://one/", "https://two/"]);
        assert_eq!(
            config.cache_dir,
            PathBuf::from("/Users/test/Library/Caches/CrashLens/symbols")
        );
    }

    #[test]
    fn expand_home_does_not_rewrite_unrelated_paths() {
        let home = Some(Path::new("/Users/test"));
        assert_eq!(
            expand_home("~other/file", home),
            PathBuf::from("~other/file")
        );
        assert_eq!(expand_home("/tmp/file", home), PathBuf::from("/tmp/file"));
    }

    #[test]
    fn comparison_selection_is_unique_and_limited_to_two() {
        let mut selection = Vec::new();
        set_comparison_selected(&mut selection, Path::new("a.dmp"), true);
        set_comparison_selected(&mut selection, Path::new("a.dmp"), true);
        set_comparison_selected(&mut selection, Path::new("b.dmp"), true);
        set_comparison_selected(&mut selection, Path::new("c.dmp"), true);
        assert_eq!(
            selection,
            vec![PathBuf::from("a.dmp"), PathBuf::from("b.dmp")]
        );
        set_comparison_selected(&mut selection, Path::new("a.dmp"), false);
        assert_eq!(selection, vec![PathBuf::from("b.dmp")]);
    }

    #[test]
    fn taking_comparison_pair_clears_selection_for_next_comparison() {
        let mut selection = vec![PathBuf::from("first.dmp"), PathBuf::from("second.dmp")];
        let pair = take_comparison_pair(&mut selection).unwrap();
        assert_eq!(
            pair,
            (PathBuf::from("first.dmp"), PathBuf::from("second.dmp"))
        );
        assert!(selection.is_empty());
    }
}

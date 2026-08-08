use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::Sender,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileEntry {
    pub path: PathBuf,
    pub size: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScanEvent {
    FoundBatch(Vec<FileEntry>),
    Progress { path: PathBuf, directories: usize },
    Finished,
}

struct Work {
    directories: VecDeque<PathBuf>,
    active: usize,
    finished: bool,
}

/// Recursively scans with a bounded worker pool. Directory I/O benefits from
/// limited concurrency, while the cap avoids overwhelming disks and network mounts.
pub fn scan(root: &Path, sender: &Sender<ScanEvent>) {
    scan_roots(&[root.to_owned()], sender, worker_count());
}

fn scan_roots(roots: &[PathBuf], sender: &Sender<ScanEvent>, workers: usize) {
    let work = Arc::new((
        Mutex::new(Work {
            directories: roots.iter().cloned().collect(),
            active: 0,
            finished: roots.is_empty(),
        }),
        Condvar::new(),
    ));
    let visited = Arc::new(AtomicUsize::new(0));
    let cancelled = Arc::new(AtomicBool::new(false));

    std::thread::scope(|scope| {
        for _ in 0..workers.max(1) {
            let work = Arc::clone(&work);
            let visited = Arc::clone(&visited);
            let cancelled = Arc::clone(&cancelled);
            let sender = sender.clone();
            scope.spawn(move || worker(&work, &visited, &cancelled, &sender));
        }
    });
    if !cancelled.load(Ordering::Relaxed) {
        let _ = sender.send(ScanEvent::Finished);
    }
}

fn worker(
    shared: &(Mutex<Work>, Condvar),
    visited: &AtomicUsize,
    cancelled: &AtomicBool,
    sender: &Sender<ScanEvent>,
) {
    loop {
        let directory = {
            let (lock, ready) = shared;
            let mut work = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            loop {
                if cancelled.load(Ordering::Relaxed) || work.finished {
                    return;
                }
                if let Some(directory) = work.directories.pop_front() {
                    work.active += 1;
                    break directory;
                }
                if work.active == 0 {
                    work.finished = true;
                    ready.notify_all();
                    return;
                }
                work = ready
                    .wait(work)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
        };

        let count = visited.fetch_add(1, Ordering::Relaxed) + 1;
        if count.is_multiple_of(128)
            && sender
                .send(ScanEvent::Progress {
                    path: directory.clone(),
                    directories: count,
                })
                .is_err()
        {
            cancelled.store(true, Ordering::Relaxed);
        }

        let mut child_directories = Vec::new();
        let mut found = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&directory) {
            for entry in entries.flatten() {
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                let path = entry.path();
                if file_type.is_dir() {
                    child_directories.push(path);
                } else if file_type.is_file() && is_dump_path(&path) {
                    found.push(FileEntry {
                        size: entry.metadata().map(|metadata| metadata.len()).unwrap_or(0),
                        path,
                    });
                }
            }
        }
        if !found.is_empty() && sender.send(ScanEvent::FoundBatch(found)).is_err() {
            cancelled.store(true, Ordering::Relaxed);
        }

        let (lock, ready) = shared;
        let mut work = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        work.directories.extend(child_directories);
        work.active -= 1;
        if work.directories.is_empty() && work.active == 0 {
            work.finished = true;
        }
        ready.notify_all();
    }
}

fn worker_count() -> usize {
    std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(4)
        .clamp(2, 8)
}

pub fn is_dump_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("dmp") || extension.eq_ignore_ascii_case("mdmp")
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn recognizes_dump_extensions_case_insensitively() {
        assert!(is_dump_path(Path::new("crash.dmp")));
        assert!(is_dump_path(Path::new("crash.MDMP")));
        assert!(!is_dump_path(Path::new("crash.txt")));
        assert!(!is_dump_path(Path::new("dmp")));
    }

    #[test]
    fn parallel_scan_finds_nested_dumps_once() {
        let root = tempfile::tempdir().unwrap();
        let nested = root.path().join("a/b/c");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(root.path().join("one.dmp"), b"one").unwrap();
        std::fs::write(nested.join("two.MDMP"), b"two").unwrap();
        std::fs::write(nested.join("ignore.txt"), b"no").unwrap();
        let (sender, receiver) = mpsc::channel();
        scan_roots(&[root.path().to_owned()], &sender, 4);
        let mut found: Vec<_> = receiver
            .try_iter()
            .flat_map(|event| match event {
                ScanEvent::FoundBatch(files) => files,
                _ => Vec::new(),
            })
            .collect();
        found.sort_by(|left, right| left.path.cmp(&right.path));
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|file| file.size == 3));
    }

    #[test]
    fn empty_root_set_finishes_without_deadlock() {
        let (sender, receiver) = mpsc::channel();
        scan_roots(&[], &sender, 4);
        assert_eq!(receiver.recv().unwrap(), ScanEvent::Finished);
    }
}

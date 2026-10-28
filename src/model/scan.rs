//! Non-following source traversal and immediate-folder derived categories.
use super::Source;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    time::{Instant, SystemTime},
};
#[derive(Debug, Clone)]
pub struct PreviewMetadata {
    pub path: PathBuf,
    pub mtime: SystemTime,
    pub size: u64,
}
#[derive(Debug, Clone)]
pub struct ImageEntry {
    pub path: PathBuf,
    pub mtime: SystemTime,
    pub size: u64,
    pub parent: PathBuf,
    pub source: PathBuf,
    pub preview: Option<PreviewMetadata>,
    pub title: Option<String>,
}
#[derive(Debug, Clone)]
pub struct ScanIssue {
    pub path: PathBuf,
    pub reason: String,
}
#[derive(Debug, Clone, Default)]
pub struct ScanResult {
    pub pool: Vec<ImageEntry>,
    pub auto_categories: BTreeMap<String, Vec<PathBuf>>,
    pub skipped: Vec<ScanIssue>,
    pub duration: std::time::Duration,
}
pub fn supported(path: &Path) -> bool {
    crate::media::supported(path)
}
pub fn belongs(path: &Path, source: &Source) -> bool {
    path.starts_with(&source.path)
        && (source.recursive || path.parent() == Some(source.path.as_path()))
}
pub fn scan(sources: &[Source]) -> ScanResult {
    let start = Instant::now();
    let mut result = ScanResult::default();
    let mut seen = BTreeSet::new();
    for source in sources {
        let mut walker = walkdir::WalkDir::new(&source.path)
            .follow_links(false)
            .max_depth(if source.recursive { usize::MAX } else { 1 })
            .into_iter();
        while let Some(item) = walker.next() {
            let item = match item {
                Ok(i) => i,
                Err(e) => {
                    result.skipped.push(ScanIssue {
                        path: e.path().unwrap_or(&source.path).into(),
                        reason: e.to_string(),
                    });
                    continue;
                }
            };
            let mut candidate = item.path().to_path_buf();
            if item.file_type().is_dir() {
                let manifest = item.path().join("project.json");
                if !(source.recursive || item.depth() == 0) || !manifest.is_file() {
                    continue;
                }
                walker.skip_current_dir();
                candidate = manifest;
            }
            if !supported(&candidate)
                || !candidate
                    .symlink_metadata()
                    .is_ok_and(|meta| meta.is_file())
            {
                continue;
            }
            if crate::media::is_project(&candidate)
                && let Err(error) = crate::media::load_project(&candidate)
            {
                result.skipped.push(ScanIssue {
                    path: candidate,
                    reason: error.to_string(),
                });
                continue;
            }
            let entry = (|| -> std::io::Result<ImageEntry> {
                let path = candidate.canonicalize()?;
                let meta = path.metadata()?;
                Ok(ImageEntry {
                    parent: path.parent().unwrap_or(&source.path).into(),
                    path,
                    mtime: meta.modified()?,
                    size: meta.len(),
                    source: source.path.clone(),
                    preview: if crate::media::is_project(&candidate) {
                        crate::media::project_preview(&candidate)
                            .ok()
                            .and_then(|path| {
                                let meta = path.metadata().ok()?;
                                Some(PreviewMetadata {
                                    mtime: meta.modified().ok()?,
                                    size: meta.len(),
                                    path,
                                })
                            })
                    } else {
                        None
                    },
                    title: if crate::media::is_project(&candidate) {
                        crate::media::load_project(&candidate)
                            .ok()
                            .map(|p| p.title)
                            .filter(|title| !title.is_empty())
                    } else {
                        None
                    },
                })
            })();
            match entry {
                Ok(entry) => {
                    if source.auto_categories
                        && let Ok(rel) = candidate.strip_prefix(&source.path)
                    {
                        let parts: Vec<_> = rel.components().collect();
                        if parts.len() > 1 {
                            let name = parts[0].as_os_str().to_string_lossy().to_string();
                            let paths = result.auto_categories.entry(name).or_default();
                            if !paths.contains(&entry.path) {
                                paths.push(entry.path.clone())
                            }
                        }
                    }
                    if seen.insert(entry.path.clone()) {
                        result.pool.push(entry)
                    }
                }
                Err(e) => result.skipped.push(ScanIssue {
                    path: candidate,
                    reason: e.to_string(),
                }),
            }
        }
    }
    result.pool.sort_by(|a, b| a.path.cmp(&b.path));
    result.duration = start.elapsed();
    result
}

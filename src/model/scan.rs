//! Non-following source traversal and immediate-folder derived categories.
use super::Source;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    time::{Instant, SystemTime},
};
#[derive(Debug, Clone)]
pub struct ImageEntry {
    pub path: PathBuf,
    pub mtime: SystemTime,
    pub size: u64,
    pub parent: PathBuf,
    pub source: PathBuf,
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
    path.extension().and_then(|s| s.to_str()).is_some_and(|s| {
        matches!(
            s.to_ascii_lowercase().as_str(),
            "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "tif" | "tiff"
        )
    })
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
        for item in walkdir::WalkDir::new(&source.path)
            .follow_links(false)
            .max_depth(if source.recursive { usize::MAX } else { 1 })
        {
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
            if !item.file_type().is_file() || !supported(item.path()) {
                continue;
            }
            let entry = (|| -> std::io::Result<ImageEntry> {
                let path = item.path().canonicalize()?;
                let meta = path.metadata()?;
                Ok(ImageEntry {
                    parent: path.parent().unwrap_or(&source.path).into(),
                    path,
                    mtime: meta.modified()?,
                    size: meta.len(),
                    source: source.path.clone(),
                })
            })();
            match entry {
                Ok(entry) => {
                    if source.auto_categories
                        && let Ok(rel) = item.path().strip_prefix(&source.path)
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
                    path: item.path().into(),
                    reason: e.to_string(),
                }),
            }
        }
    }
    result.pool.sort_by(|a, b| a.path.cmp(&b.path));
    result.duration = start.elapsed();
    result
}

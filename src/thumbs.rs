//! Bounded background thumbnail workers; full-size decoding never occurs in the UI.
use crate::{
    config::Thumbs,
    model::{ImageEntry, store::atomic_write},
};
use anyhow::{Context, Result};
use image::{ImageDecoder, ImageReader, codecs::jpeg::JpegEncoder};
use std::{
    collections::{HashMap, HashSet},
    os::unix::ffi::OsStrExt,
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    time::UNIX_EPOCH,
};

#[derive(Debug, Clone)]
pub enum ThumbnailRequest {
    Cached(PathBuf),
    Pending,
    Failed(String),
}
#[derive(Debug, Clone)]
pub struct ThumbnailEvent {
    pub path: PathBuf,
    pub result: std::result::Result<PathBuf, String>,
}
#[derive(Default)]
struct Status {
    pending: HashSet<String>,
    failed: HashMap<String, String>,
    priority: Option<HashSet<String>>,
}
struct Inner {
    cache: PathBuf,
    config: Thumbs,
    workers: rayon::ThreadPool,
    status: Mutex<Status>,
    tx: mpsc::Sender<ThumbnailEvent>,
    rx: Mutex<mpsc::Receiver<ThumbnailEvent>>,
}
#[derive(Clone)]
pub struct Thumbnailer {
    inner: Arc<Inner>,
}
impl Thumbnailer {
    pub fn new(cache: PathBuf, config: Thumbs) -> Result<Self> {
        anyhow::ensure!(
            config.workers > 0
                && config.workers <= 32
                && config.width > 0
                && config.height > 0
                && config.width <= 4096
                && config.height <= 4096
                && (1..=100).contains(&config.quality),
            "Invalid thumbnail configuration"
        );
        std::fs::create_dir_all(&cache)?;
        let (tx, rx) = mpsc::channel();
        Ok(Self {
            inner: Arc::new(Inner {
                cache,
                config: config.clone(),
                workers: rayon::ThreadPoolBuilder::new()
                    .num_threads(config.workers)
                    .thread_name(|i| format!("thumbnail-{i}"))
                    .build()?,
                status: Mutex::new(Status::default()),
                tx,
                rx: Mutex::new(rx),
            }),
        })
    }
    pub fn key(&self, entry: &ImageEntry) -> String {
        let mut hash = blake3::Hasher::new();
        hash.update(entry.path.as_os_str().as_bytes());
        hash.update(&[0]);
        let time = entry.mtime.duration_since(UNIX_EPOCH).unwrap_or_default();
        hash.update(&time.as_secs().to_le_bytes());
        hash.update(&time.subsec_nanos().to_le_bytes());
        hash.update(&entry.size.to_le_bytes());
        hash.update(&self.inner.config.width.to_le_bytes());
        hash.update(&self.inner.config.height.to_le_bytes());
        hash.update(&[self.inner.config.quality]);
        hash.finalize().to_hex().to_string()
    }
    fn target(&self, entry: &ImageEntry) -> PathBuf {
        self.inner.cache.join(format!("{}.jpg", self.key(entry)))
    }
    pub fn cached(&self, entry: &ImageEntry) -> Option<PathBuf> {
        let target = self.target(entry);
        target.is_file().then_some(target)
    }
    /// Keep queued work relevant to the viewport. In-progress decodes finish normally.
    /// Call before requesting the visible entries plus overscan; ordinary consumers
    /// that never call this method continue to process every requested thumbnail.
    pub fn prioritize(&self, entries: &[ImageEntry]) {
        let keys = entries.iter().map(|entry| self.key(entry)).collect();
        if let Ok(mut status) = self.inner.status.lock() {
            status.priority = Some(keys);
        }
    }
    pub fn request(&self, entry: &ImageEntry) -> ThumbnailRequest {
        if let Some(path) = self.cached(entry) {
            return ThumbnailRequest::Cached(path);
        }
        let key = self.key(entry);
        let Ok(mut status) = self.inner.status.lock() else {
            return ThumbnailRequest::Failed("Thumbnail worker state poisoned".into());
        };
        if let Some(error) = status.failed.get(&key) {
            return ThumbnailRequest::Failed(error.clone());
        }
        if !status.pending.insert(key.clone()) {
            return ThumbnailRequest::Pending;
        }
        drop(status);
        let service = self.clone();
        let entry = entry.clone();
        self.inner.workers.spawn(move || {
            if let Ok(mut status) = service.inner.status.lock()
                && status
                    .priority
                    .as_ref()
                    .is_some_and(|keys| !keys.contains(&key))
            {
                status.pending.remove(&key);
                return;
            }
            let result = service.generate(&entry).map_err(|e| format!("{e:#}"));
            if let Ok(mut status) = service.inner.status.lock() {
                status.pending.remove(&key);
                if let Err(error) = &result {
                    status.failed.insert(key, error.clone());
                }
            }
            let _ = service.inner.tx.send(ThumbnailEvent {
                path: entry.path,
                result,
            });
        });
        ThumbnailRequest::Pending
    }
    pub fn drain(&self) -> Vec<ThumbnailEvent> {
        match self.inner.rx.lock() {
            Ok(rx) => rx.try_iter().collect(),
            Err(_) => vec![],
        }
    }
    /// Synchronous generation for a worker or a headless cache operation.
    pub fn generate(&self, entry: &ImageEntry) -> Result<PathBuf> {
        if let Some(path) = self.cached(entry) {
            return Ok(path);
        }
        let mut decoder = ImageReader::open(&entry.path)?
            .with_guessed_format()?
            .into_decoder()
            .with_context(|| format!("Decoding {}", entry.path.display()))?;
        let orientation = decoder.orientation()?;
        let mut bitmap = image::DynamicImage::from_decoder(decoder)?;
        bitmap.apply_orientation(orientation);
        let width = self.inner.config.width;
        let height = self.inner.config.height;
        // Crop before shrinking: area averaging visits each source pixel once, unlike
        // a full-resolution Lanczos convolution. Keep intermediate size bounded even
        // for very tall/wide images and retain the exact cover-crop aspect ratio.
        let (source_width, source_height) = (bitmap.width(), bitmap.height());
        let (crop_width, crop_height) = if u64::from(source_width) * u64::from(height)
            > u64::from(source_height) * u64::from(width)
        {
            (
                ((u64::from(source_height) * u64::from(width) / u64::from(height)) as u32).max(1),
                source_height,
            )
        } else {
            (
                source_width,
                ((u64::from(source_width) * u64::from(height) / u64::from(width)) as u32).max(1),
            )
        };
        let cropped = bitmap.crop_imm(
            (source_width - crop_width) / 2,
            (source_height - crop_height) / 2,
            crop_width,
            crop_height,
        );
        let bitmap = if crop_width >= width && crop_height >= height {
            cropped.thumbnail_exact(width, height)
        } else {
            cropped.resize_exact(width, height, image::imageops::FilterType::Triangle)
        }
        .to_rgb8();
        let mut bytes = Vec::new();
        JpegEncoder::new_with_quality(&mut bytes, self.inner.config.quality)
            .encode_image(&bitmap)?;
        let target = self.target(entry);
        atomic_write(&target, &bytes)?;
        Ok(target)
    }
    /// Explicit maintenance action; removes only this service's orphaned hash JPEGs.
    pub fn cleanup(&self, entries: &[ImageEntry]) -> Result<usize> {
        let live: HashSet<_> = entries.iter().map(|e| self.target(e)).collect();
        let mut count = 0;
        for item in std::fs::read_dir(&self.inner.cache)? {
            let item = item?;
            let path = item.path();
            let own = path
                .file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit()))
                && path.extension().is_some_and(|s| s == "jpg");
            if own && !live.contains(&path) && item.file_type()?.is_file() {
                std::fs::remove_file(path)?;
                count += 1;
            }
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Source, scan};
    fn fixture() -> (tempfile::TempDir, Thumbnailer, ImageEntry) {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        std::fs::create_dir(&source).unwrap();
        image::RgbImage::new(3, 2)
            .save(source.join("tiny.png"))
            .unwrap();
        let entry = scan(&[Source {
            path: source.canonicalize().unwrap(),
            recursive: true,
            auto_categories: true,
        }])
        .pool
        .remove(0);
        let thumbs = Thumbnailer::new(dir.path().join("thumbs"), Thumbs::default()).unwrap();
        (dir, thumbs, entry)
    }
    #[test]
    fn cache_hits_invalidate_and_crop() {
        let (_dir, thumbs, mut entry) = fixture();
        assert!(thumbs.cached(&entry).is_none());
        let target = thumbs.generate(&entry).unwrap();
        assert_eq!(image::image_dimensions(&target).unwrap(), (400, 225));
        assert_eq!(thumbs.cached(&entry), Some(target));
        entry.size += 1;
        assert!(thumbs.cached(&entry).is_none());
        assert_eq!(thumbs.cleanup(&[entry]).unwrap(), 1);
    }
    #[test]
    fn unreadable_images_fail_without_cache() {
        let (_dir, thumbs, entry) = fixture();
        std::fs::write(&entry.path, b"broken").unwrap();
        assert!(thumbs.generate(&entry).is_err());
        assert!(thumbs.cached(&entry).is_none());
    }
    #[test]
    fn background_delivery_and_restart_hit() {
        let (dir, thumbs, entry) = fixture();
        assert!(matches!(thumbs.request(&entry), ThumbnailRequest::Pending));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if let Some(event) = thumbs.drain().first() {
                assert_eq!(event.path, entry.path);
                assert!(event.result.is_ok());
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let second = Thumbnailer::new(dir.path().join("thumbs"), Thumbs::default()).unwrap();
        assert!(matches!(
            second.request(&entry),
            ThumbnailRequest::Cached(_)
        ));
    }
    #[test]
    fn failed_requests_are_cached_until_metadata_changes() {
        let (_dir, thumbs, mut entry) = fixture();
        std::fs::write(&entry.path, b"not an image").unwrap();
        assert!(matches!(thumbs.request(&entry), ThumbnailRequest::Pending));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if let Some(event) = thumbs.drain().first() {
                assert!(event.result.is_err());
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(matches!(
            thumbs.request(&entry),
            ThumbnailRequest::Failed(_)
        ));
        image::RgbImage::new(3, 2).save(&entry.path).unwrap();
        entry.size += 1;
        assert!(matches!(thumbs.request(&entry), ThumbnailRequest::Pending));
        loop {
            if let Some(event) = thumbs.drain().first() {
                assert!(event.result.is_ok());
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    #[test]
    fn jpeg_exif_orientation_is_applied_before_resize() {
        use image::ImageEncoder;
        let (dir, _thumbs, _entry) = fixture();
        let source = dir.path().join("source");
        let path = source.join("oriented.jpg");
        let pixels = image::RgbImage::from_fn(3, 2, |x, y| {
            image::Rgb([(x * 90) as u8, (y * 180) as u8, ((x + y) * 40) as u8])
        });
        // Minimal little-endian TIFF IFD: Orientation (0x0112), SHORT, value 6 (90° clockwise).
        let exif = vec![
            b'I', b'I', 42, 0, 8, 0, 0, 0, 1, 0, 0x12, 1, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0,
        ];
        let mut encoded = Vec::new();
        let mut encoder = JpegEncoder::new_with_quality(&mut encoded, 100);
        encoder.set_exif_metadata(exif).unwrap();
        encoder.encode_image(&pixels).unwrap();
        std::fs::write(&path, encoded).unwrap();
        let entry = scan(&[Source {
            path: source.canonicalize().unwrap(),
            recursive: true,
            auto_categories: true,
        }])
        .pool
        .into_iter()
        .find(|entry| entry.path == path)
        .unwrap();
        let thumbs = Thumbnailer::new(
            dir.path().join("oriented-cache"),
            Thumbs {
                width: 2,
                height: 3,
                quality: 100,
                ..Thumbs::default()
            },
        )
        .unwrap();
        let output = thumbs.generate(&entry).unwrap();
        let expected = image::open(&path).unwrap().rotate90().to_rgb8();
        let mut expected_jpeg = Vec::new();
        JpegEncoder::new_with_quality(&mut expected_jpeg, 100)
            .encode_image(&expected)
            .unwrap();
        assert_eq!(std::fs::read(output).unwrap(), expected_jpeg);
    }
    #[test]
    fn cover_crop_keeps_center_of_non_widescreen_image() {
        let (dir, _thumbs, entry) = fixture();
        let pixels = image::RgbImage::from_fn(3, 2, |x, _| match x {
            0 => image::Rgb([255, 0, 0]),
            1 => image::Rgb([0, 255, 0]),
            _ => image::Rgb([0, 0, 255]),
        });
        pixels.save(&entry.path).unwrap();
        let thumbs = Thumbnailer::new(
            dir.path().join("portrait-cache"),
            Thumbs {
                width: 1,
                height: 2,
                quality: 100,
                ..Thumbs::default()
            },
        )
        .unwrap();
        let output = thumbs.generate(&entry).unwrap();
        let result = image::open(output).unwrap().to_rgb8();
        assert_eq!(result.dimensions(), (1, 2));
        for pixel in result.pixels() {
            assert!(pixel[1] > 240 && pixel[0] < 15 && pixel[2] < 15);
        }
    }
    #[test]
    fn stale_viewport_jobs_are_skipped_and_can_be_requested_again() {
        let (dir, _thumbs, entry) = fixture();
        let thumbs = Thumbnailer::new(
            dir.path().join("priority-cache"),
            Thumbs {
                workers: 1,
                ..Thumbs::default()
            },
        )
        .unwrap();
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        thumbs.inner.workers.spawn(move || {
            started_tx.send(()).unwrap();
            release_rx.recv().unwrap();
        });
        started_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        thumbs.prioritize(&[]);
        assert!(matches!(thumbs.request(&entry), ThumbnailRequest::Pending));
        release_tx.send(()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if thumbs.inner.status.lock().unwrap().pending.is_empty() {
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(thumbs.cached(&entry).is_none());
        assert!(thumbs.drain().is_empty());
        thumbs.prioritize(std::slice::from_ref(&entry));
        assert!(matches!(thumbs.request(&entry), ThumbnailRequest::Pending));
        loop {
            if let Some(event) = thumbs.drain().first() {
                assert!(event.result.is_ok());
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}

//! Bounded background thumbnail workers; full-size decoding never occurs in the UI.
use crate::{
    config::Thumbs,
    media::{self, MediaKind},
    model::{ImageEntry, store::atomic_write},
};
use anyhow::{Context, Result};
use image::{ImageDecoder, ImageReader, codecs::jpeg::JpegEncoder};
use std::{
    collections::{HashMap, HashSet},
    fs::OpenOptions,
    os::unix::ffi::OsStrExt,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
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
    ffmpeg: PathBuf,
    video_timeout: Duration,
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
        Self::with_ffmpeg(cache, config, PathBuf::from("ffmpeg"))
    }
    /// Injectable optional video decoder. Image/project previews do not need FFmpeg.
    pub fn with_ffmpeg(cache: PathBuf, config: Thumbs, ffmpeg: PathBuf) -> Result<Self> {
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
                ffmpeg,
                video_timeout: Duration::from_secs(10),
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
        if media::kind(&entry.path) == MediaKind::WallpaperEngine {
            if let Some(preview) = &entry.preview {
                hash.update(preview.path.as_os_str().as_bytes());
                hash.update(&preview.size.to_le_bytes());
                let mtime = preview.mtime.duration_since(UNIX_EPOCH).unwrap_or_default();
                hash.update(&mtime.as_secs().to_le_bytes());
                hash.update(&mtime.subsec_nanos().to_le_bytes());
            } else {
                hash.update(b"missing project preview; rescan required");
            }
        }
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
        let temporary;
        let input = match media::kind(&entry.path) {
            MediaKind::Image => entry.path.clone(),
            MediaKind::WallpaperEngine => entry
                .preview
                .as_ref()
                .context("Project preview unavailable; add a preview image and rescan sources")?
                .path
                .clone(),
            MediaKind::Video => {
                temporary = self.video_frame(&entry.path)?;
                temporary.0.clone()
            }
        };
        let mut decoder = ImageReader::open(&input)?
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
    fn video_frame(&self, path: &Path) -> Result<TemporaryFrame> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let output = self
            .inner
            .cache
            .join(format!(".video-{}-{stamp}.png", std::process::id()));
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&output)?;
        drop(file);
        let temporary = TemporaryFrame(output);
        let filter = format!(
            "scale={}:{}:force_original_aspect_ratio=increase,crop={}:{}",
            self.inner.config.width,
            self.inner.config.height,
            self.inner.config.width,
            self.inner.config.height
        );
        let mut child = Command::new(&self.inner.ffmpeg)
            .args([
                "-nostdin",
                "-loglevel",
                "error",
                "-y",
                "-threads",
                "1",
                "-i",
            ])
            .arg(path)
            .args(["-frames:v", "1", "-vf", &filter, "-an", "-threads", "1"])
            .arg(&temporary.0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("Video previews require ffmpeg; install it or choose an image wallpaper")?;
        let deadline = Instant::now() + self.inner.video_timeout;
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    anyhow::ensure!(
                        status.success(),
                        "ffmpeg could not decode the first video frame"
                    );
                    break;
                }
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20))
                }
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    anyhow::bail!(
                        "Video preview decoding timed out after {} seconds",
                        self.inner.video_timeout.as_secs()
                    );
                }
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(error.into());
                }
            }
        }
        Ok(temporary)
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

/// Owns only a decoder-generated file inside the thumbnail cache.
struct TemporaryFrame(PathBuf);
impl Drop for TemporaryFrame {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
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
    #[test]
    fn thumbnails_preserve_neutral_and_saturated_srgb_colors() {
        let (dir, thumbs, mut entry) = fixture();
        // A gamma lift or a tint changes these flat fields even without image detail.
        // JPEG's RGB↔YCbCr quantization can change a channel by a few code values.
        let colors = [
            [0, 0, 0],
            [32, 32, 32],
            [64, 64, 64],
            [128, 128, 128],
            [192, 192, 192],
            [255, 255, 255],
            [255, 0, 0],
            [0, 255, 0],
            [0, 0, 255],
            [105, 212, 205],
            [21, 32, 38],
        ];
        for (index, color) in colors.into_iter().enumerate() {
            let path = dir.path().join("source").join(format!("color-{index}.png"));
            image::RgbImage::from_pixel(3, 2, image::Rgb(color))
                .save(&path)
                .unwrap();
            entry.path = path.canonicalize().unwrap();
            let metadata = entry.path.metadata().unwrap();
            entry.mtime = metadata.modified().unwrap();
            entry.size = metadata.len();
            let cached = thumbs.generate(&entry).unwrap();
            let actual = image::open(cached).unwrap().to_rgb8();
            for pixel in actual.pixels() {
                for channel in 0..3 {
                    assert!(
                        pixel[channel].abs_diff(color[channel]) <= 4,
                        "source {color:?}, thumbnail {:?}",
                        pixel.0
                    );
                }
            }
        }
    }
    fn mock_tool(path: &Path, body: &str) {
        use std::{io::Write, os::unix::fs::PermissionsExt};
        let mut temporary = tempfile::NamedTempFile::new_in(path.parent().unwrap()).unwrap();
        write!(temporary, "#!/bin/sh\n{body}\n").unwrap();
        temporary
            .as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o700))
            .unwrap();
        temporary.into_temp_path().persist(path).unwrap();
    }
    fn entry_for(path: &Path) -> ImageEntry {
        let path = path.canonicalize().unwrap();
        let metadata = path.metadata().unwrap();
        let parent = path.parent().unwrap().to_path_buf();
        ImageEntry {
            path,
            mtime: metadata.modified().unwrap(),
            size: metadata.len(),
            parent: parent.clone(),
            source: parent,
            preview: None,
            title: None,
        }
    }
    #[test]
    fn video_first_frame_uses_optional_decoder_and_cleans_temporary_output() {
        let (dir, _thumbs, image) = fixture();
        let video = dir.path().join("video with spaces.mp4");
        std::fs::write(&video, b"mock video").unwrap();
        let tool = dir.path().join("ffmpeg");
        let args = dir.path().join("args");
        mock_tool(
            &tool,
            &format!(
                "printf '%s\\n' \"$@\" > '{}'; for last; do :; done; cp '{}' \"$last\"",
                args.display(),
                image.path.display()
            ),
        );
        let cache = dir.path().join("video-cache");
        let thumbs = Thumbnailer::with_ffmpeg(cache.clone(), Thumbs::default(), tool).unwrap();
        let output = thumbs.generate(&entry_for(&video)).unwrap();
        assert_eq!(image::image_dimensions(output).unwrap(), (400, 225));
        let args = std::fs::read_to_string(args).unwrap();
        assert!(args.contains(video.to_str().unwrap()));
        assert!(args.contains("-frames:v\n1"));
        assert!(!std::fs::read_dir(cache).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".video-")
        }));
    }
    #[test]
    fn missing_video_decoder_fails_actionably_and_does_not_cache() {
        let (dir, _thumbs, _image) = fixture();
        let video = dir.path().join("video.webm");
        std::fs::write(&video, b"mock video").unwrap();
        let thumbs = Thumbnailer::with_ffmpeg(
            dir.path().join("video-cache"),
            Thumbs::default(),
            dir.path().join("missing-ffmpeg"),
        )
        .unwrap();
        let entry = entry_for(&video);
        let error = thumbs.generate(&entry).unwrap_err();
        assert!(error.to_string().contains("ffmpeg"));
        assert!(thumbs.cached(&entry).is_none());
    }
    #[test]
    fn video_preview_timeout_kills_decoder() {
        let (dir, _thumbs, _image) = fixture();
        let video = dir.path().join("video.mkv");
        std::fs::write(&video, b"mock video").unwrap();
        let tool = dir.path().join("ffmpeg");
        mock_tool(&tool, "exec sleep 30");
        let mut thumbs =
            Thumbnailer::with_ffmpeg(dir.path().join("video-cache"), Thumbs::default(), tool)
                .unwrap();
        Arc::get_mut(&mut thumbs.inner).unwrap().video_timeout = Duration::from_millis(20);
        let start = Instant::now();
        let error = thumbs.generate(&entry_for(&video)).unwrap_err();
        assert!(error.to_string().contains("timed out"));
        assert!(start.elapsed() < Duration::from_secs(2));
    }
    #[test]
    fn project_preview_metadata_invalidates_cached_thumbnail() {
        use std::io::Write;
        let (dir, _thumbs, _image) = fixture();
        let project = dir.path().join("project.json");
        let preview = dir.path().join("preview.png");
        std::fs::write(
            &project,
            r#"{"type":"scene","file":"scene.pkg","preview":"preview.png"}"#,
        )
        .unwrap();
        image::RgbImage::new(3, 2).save(&preview).unwrap();
        let thumbs = Thumbnailer::new(dir.path().join("project-cache"), Thumbs::default()).unwrap();
        let mut entry = entry_for(&project);
        let metadata = preview.metadata().unwrap();
        entry.preview = Some(crate::model::scan::PreviewMetadata {
            path: preview.clone(),
            mtime: metadata.modified().unwrap(),
            size: metadata.len(),
        });
        let first_key = thumbs.key(&entry);
        thumbs.generate(&entry).unwrap();
        assert!(thumbs.cached(&entry).is_some());
        std::fs::OpenOptions::new()
            .append(true)
            .open(&preview)
            .unwrap()
            .write_all(b"new preview metadata")
            .unwrap();
        let metadata = preview.metadata().unwrap();
        entry.preview = Some(crate::model::scan::PreviewMetadata {
            path: preview.clone(),
            mtime: metadata.modified().unwrap(),
            size: metadata.len(),
        });
        assert_ne!(first_key, thumbs.key(&entry));
        assert!(thumbs.cached(&entry).is_none());
        thumbs.generate(&entry).unwrap();
    }
}

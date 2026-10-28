//! A sleeping headless rotation loop; no scans or preview work between changes.
use crate::{
    config::Config,
    media::{self, MediaKind},
    model::{CategoryRef, Library, scan},
    paths::Paths,
    wallpaper::{Backend, select_random},
};
use anyhow::{Context, Result, bail};
use std::{
    fs::{self, OpenOptions},
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub interval: Option<String>,
    pub category: Option<String>,
    pub include_live: bool,
    pub quiet: bool,
}
/// Human intervals: 30m, 2h, 45s, 1d; unitless values are seconds.
pub fn parse_interval(value: &str) -> Result<Duration> {
    let input = value.trim();
    let (digits, multiplier) = if let Some(n) = input.strip_suffix('s') {
        (n, 1)
    } else if let Some(n) = input.strip_suffix('m') {
        (n, 60)
    } else if let Some(n) = input.strip_suffix('h') {
        (n, 3600)
    } else if let Some(n) = input.strip_suffix('d') {
        (n, 86400)
    } else {
        (input, 1)
    };
    let seconds = digits
        .parse::<u64>()
        .ok()
        .and_then(|n| n.checked_mul(multiplier));
    let seconds = seconds
        .filter(|seconds| (1..=365 * 86400).contains(seconds))
        .context("Interval must be 1 second to 365 days (examples: 30m, 2h, 45s)")?;
    Ok(Duration::from_secs(seconds))
}
fn configured(paths: &Paths, options: &Options) -> Result<(Config, Duration)> {
    let mut config = Config::load(&paths.config)?;
    if let Some(interval) = &options.interval {
        config.rotation.interval = interval.clone();
    }
    if let Some(category) = &options.category {
        config.rotation.category = Some(category.clone());
    }
    config.rotation.include_live |= options.include_live;
    config.behavior.notify = config.rotation.notify && !options.quiet;
    config.validate()?;
    let interval = parse_interval(&config.rotation.interval)?;
    Ok((config, interval))
}
/// Resolve at change time so the last GUI category and new files take effect.
pub fn candidates(
    library: &Library,
    category: &CategoryRef,
    include_live: bool,
) -> Vec<std::path::PathBuf> {
    let scanned = scan(&library.sources);
    let eligible = |path: &std::path::Path| include_live || media::kind(path) == MediaKind::Image;
    let mut paths = library
        .category_paths(category, &scanned)
        .into_iter()
        .filter(|path| eligible(path))
        .collect::<Vec<_>>();
    if paths.is_empty() {
        paths = library
            .category_paths(&CategoryRef::All, &scanned)
            .into_iter()
            .filter(|path| eligible(path))
            .collect();
    }
    paths
}
pub fn change_once(paths: &Paths, options: &Options) -> Result<std::path::PathBuf> {
    let (config, _) = configured(paths, options)?;
    let library = Library::load(&paths.library)?;
    let category = if let Some(name) = &config.rotation.category {
        library.resolve_category(name)?
    } else {
        library.last_category.clone()
    };
    let choices = candidates(&library, &category, config.rotation.include_live);
    let backend = Backend::new(paths.clone(), config.clone());
    let chosen = select_random(
        &choices,
        backend.current().as_deref(),
        config.behavior.avoid_repeat,
    )?;
    backend.apply(&chosen)?;
    Ok(chosen)
}
/// Wait before the first swap. A service restart is how to apply interval changes
/// immediately; otherwise config/library are refreshed on each scheduled change.
pub fn run(paths: &Paths, options: &Options) -> Result<()> {
    fs::create_dir_all(&paths.cache)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(paths.cache.join("rotation.lock"))?;
    match lock.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => {
            bail!("A wallpaper rotation process is already running")
        }
        Err(std::fs::TryLockError::Error(error)) => return Err(error.into()),
    }
    loop {
        let (_, interval) = configured(paths, options)?;
        let deadline = Instant::now() + interval;
        std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
        if let Err(error) = change_once(paths, options) {
            log::warn!("Wallpaper rotation failed; retrying at the next interval: {error:#}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Source;
    #[test]
    fn intervals_are_checked_and_overflow_never_reaches_sleep() {
        for (input, seconds) in [
            ("30m", 1800),
            ("2h", 7200),
            ("45s", 45),
            ("1d", 86400),
            ("30", 30),
        ] {
            assert_eq!(parse_interval(input).unwrap().as_secs(), seconds)
        }
        for input in [
            "",
            "0s",
            "-3m",
            "1.5h",
            "366d",
            "18446744073709551615d",
            "30 ms",
        ] {
            assert!(parse_interval(input).is_err(), "{input}");
        }
    }
    #[test]
    fn default_rotation_skips_live_and_empty_filtered_category_falls_back() -> Result<()> {
        let root = tempfile::tempdir()?;
        let image = root.path().join("still.png");
        image::RgbImage::new(2, 3).save(&image)?;
        let video = root.path().join("loop.mp4");
        fs::write(&video, b"fixture")?;
        let mut library = Library::default();
        library.add_source(Source {
            path: root.path().into(),
            recursive: true,
            auto_categories: true,
        })?;
        let category = library.add_category("Motion")?;
        library.add_images(category, std::slice::from_ref(&video))?;
        assert_eq!(
            candidates(&library, &CategoryRef::Manual { id: category }, false),
            vec![image.canonicalize()?]
        );
        assert_eq!(
            candidates(&library, &CategoryRef::Manual { id: category }, true),
            vec![video.canonicalize()?]
        );
        Ok(())
    }
    #[test]
    fn config_changes_are_reloaded_and_cli_overrides_are_explicit() -> Result<()> {
        let root = tempfile::tempdir()?;
        let paths = Paths::under(root.path());
        let mut config = Config::default();
        config.rotation.interval = "1h".into();
        config.rotation.notify = true;
        config.save(&paths.config)?;
        let options = Options {
            interval: Some("10s".into()),
            quiet: true,
            ..Default::default()
        };
        let (loaded, interval) = configured(&paths, &options)?;
        assert_eq!(interval.as_secs(), 10);
        assert!(!loaded.behavior.notify);
        config.rotation.interval = "5m".into();
        config.save(&paths.config)?;
        assert_eq!(configured(&paths, &Options::default())?.1.as_secs(), 300);
        Ok(())
    }
}

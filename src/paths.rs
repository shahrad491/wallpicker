//! Injectable XDG paths. Construction has no filesystem side effects.
use anyhow::{Context, Result};
use std::{env, path::PathBuf};
#[derive(Debug, Clone)]
pub struct Paths {
    pub config: PathBuf,
    pub library: PathBuf,
    pub cache: PathBuf,
    pub state: PathBuf,
    pub default_source: PathBuf,
}
impl Paths {
    pub fn discover() -> Result<Self> {
        let home = PathBuf::from(env::var_os("HOME").context("HOME is unset")?);
        let dir = |key, fallback: &str| {
            env::var_os(key)
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .unwrap_or_else(|| home.join(fallback))
        };
        Ok(Self {
            config: dir("XDG_CONFIG_HOME", ".config").join("wallpicker/config.toml"),
            library: dir("XDG_DATA_HOME", ".local/share").join("wallpicker/library.json"),
            cache: dir("XDG_CACHE_HOME", ".cache").join("wallpicker"),
            state: dir("XDG_STATE_HOME", ".local/state").join("dotfiles/wallpaper/current"),
            default_source: home.join("Pictures/wallpaper"),
        })
    }
    pub fn under(root: &std::path::Path) -> Self {
        Self {
            config: root.join("config/config.toml"),
            library: root.join("data/library.json"),
            cache: root.join("cache"),
            state: root.join("state/current"),
            default_source: root.join("pictures"),
        }
    }
}

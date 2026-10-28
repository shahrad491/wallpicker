//! TOML defaults, validation, and explicit saves.
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::path::Path;
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub behavior: Behavior,
    pub live: Live,
    pub rotation: Rotation,
    pub thumbs: Thumbs,
    pub window: Window,
    pub theme: Palette,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Live {
    pub output: String,
    pub fps: u32,
    pub muted: bool,
    pub assets_dir: Option<std::path::PathBuf>,
}
impl Default for Live {
    fn default() -> Self {
        Self {
            output: "ALL".into(),
            fps: 30,
            muted: true,
            assets_dir: None,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Rotation {
    pub interval: String,
    pub category: Option<String>,
    pub include_live: bool,
    pub notify: bool,
}
impl Default for Rotation {
    fn default() -> Self {
        Self {
            interval: "30m".into(),
            category: None,
            include_live: false,
            notify: false,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Behavior {
    pub close_on_select: bool,
    pub notify: bool,
    pub avoid_repeat: bool,
    pub transition: String,
    pub duration: f32,
}
impl Default for Behavior {
    fn default() -> Self {
        Self {
            close_on_select: true,
            notify: true,
            avoid_repeat: true,
            transition: "random".into(),
            duration: 1.2,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Thumbs {
    pub width: u32,
    pub height: u32,
    pub quality: u8,
    pub workers: usize,
}
impl Default for Thumbs {
    fn default() -> Self {
        Self {
            width: 400,
            height: 225,
            quality: 82,
            workers: 4,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Window {
    pub width_percent: u32,
    pub height_percent: u32,
}
impl Default for Window {
    fn default() -> Self {
        Self {
            width_percent: 85,
            height_percent: 85,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Palette {
    pub accent: String,
    pub background: String,
    pub surface: String,
    pub foreground: String,
    pub muted: String,
}
impl Default for Palette {
    fn default() -> Self {
        Self {
            accent: "#69d4cd".into(),
            background: "#152026".into(),
            surface: "#203139".into(),
            foreground: "#e4edef".into(),
            muted: "#9bafb6".into(),
        }
    }
}
impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                let value: Self = match toml::from_str(&text) {
                    Ok(v) => v,
                    Err(e) => {
                        log::warn!("Invalid config, using defaults: {e}");
                        return Ok(Self::default());
                    }
                };
                if let Err(e) = value.validate() {
                    log::warn!("Invalid config, using defaults: {e}");
                    Ok(Self::default())
                } else {
                    Ok(value)
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        super::model::store::atomic_write(path, toml::to_string_pretty(self)?.as_bytes())
    }
    pub fn validate(&self) -> Result<()> {
        crate::rotation::parse_interval(&self.rotation.interval)?;
        if self
            .rotation
            .category
            .as_ref()
            .is_some_and(|name| name.trim().is_empty())
        {
            bail!("Rotation category cannot be empty; omit it to use the last selected category")
        }
        if !(1..=144).contains(&self.live.fps)
            || self.live.output.trim().is_empty()
            || self.live.output.len() > 128
            || self.live.output.chars().any(char::is_control)
        {
            bail!("Live wallpapers need a monitor name or ALL, and FPS between 1 and 144")
        }
        if self
            .live
            .assets_dir
            .as_ref()
            .is_some_and(|path| !path.is_absolute())
        {
            bail!("Wallpaper Engine assets_dir must be an absolute path")
        }

        if self.thumbs.width == 0
            || self.thumbs.height == 0
            || self.thumbs.width > 4096
            || self.thumbs.height > 4096
            || !(1..=100).contains(&self.thumbs.quality)
            || !(1..=32).contains(&self.thumbs.workers)
        {
            bail!("Invalid thumbnail settings")
        };
        if !(1..=100).contains(&self.window.width_percent)
            || !(1..=100).contains(&self.window.height_percent)
        {
            bail!("Invalid window percentages")
        };
        if !self.behavior.duration.is_finite()
            || !(0.0..=60.0).contains(&self.behavior.duration)
            || ![
                "random", "left", "right", "top", "bottom", "center", "outer", "fade", "none",
            ]
            .contains(&self.behavior.transition.as_str())
        {
            bail!("Invalid transition")
        };
        for color in [
            &self.theme.accent,
            &self.theme.background,
            &self.theme.surface,
            &self.theme.foreground,
            &self.theme.muted,
        ] {
            if color.len() != 7
                || !color.starts_with('#')
                || u32::from_str_radix(&color[1..], 16).is_err()
            {
                bail!("Colors must be #RRGGBB")
            }
        }
        Ok(())
    }
}

//! Media classification and read-only Wallpaper Engine project metadata.
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Image,
    Video,
    WallpaperEngine,
}

pub fn is_image(path: &Path) -> bool {
    extension(path).is_some_and(|ext| {
        matches!(
            ext.as_str(),
            "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "tif" | "tiff"
        )
    })
}
pub fn is_video(path: &Path) -> bool {
    extension(path)
        .is_some_and(|ext| matches!(ext.as_str(), "mp4" | "webm" | "mkv" | "mov" | "m4v" | "avi"))
}
fn extension(path: &Path) -> Option<String> {
    path.extension()?.to_str().map(str::to_ascii_lowercase)
}
pub fn is_project(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == "project.json")
}
pub fn supported(path: &Path) -> bool {
    is_image(path) || is_video(path) || is_project(path)
}
pub fn kind(path: &Path) -> MediaKind {
    if is_project(path) {
        MediaKind::WallpaperEngine
    } else if is_video(path) {
        MediaKind::Video
    } else {
        MediaKind::Image
    }
}
pub fn display_name(path: &Path) -> String {
    let name = if is_project(path) {
        path.parent().and_then(Path::file_name)
    } else {
        path.file_name()
    };
    name.unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}
/// A project is stored by canonical manifest path, preserving the existing path-only schema.
pub fn canonical_wallpaper(path: &Path) -> Result<PathBuf> {
    let path = if path.is_dir() {
        path.join("project.json")
    } else {
        path.into()
    };
    let path = path
        .canonicalize()
        .with_context(|| format!("Wallpaper missing: {}", path.display()))?;
    anyhow::ensure!(
        path.is_file(),
        "Wallpaper must be a file or a Wallpaper Engine project directory"
    );
    if is_project(&path) {
        load_project(&path)?;
    }
    Ok(path)
}
#[derive(Debug, Deserialize)]
pub struct Project {
    #[serde(rename = "type")]
    pub project_type: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub preview: Option<String>,
}
pub fn load_project(path: &Path) -> Result<Project> {
    let path = if path.is_dir() {
        path.join("project.json")
    } else {
        path.into()
    };
    let meta = fs::metadata(&path)?;
    anyhow::ensure!(
        meta.is_file() && meta.len() <= 4 * 1024 * 1024,
        "Project manifest must be a regular JSON file up to 4 MiB"
    );
    let project: Project = serde_json::from_slice(&fs::read(&path)?)
        .context("Invalid Wallpaper Engine project.json")?;
    anyhow::ensure!(
        ["scene", "video", "web"].contains(&project.project_type.as_str()),
        "Unsupported Wallpaper Engine project type: {} (scene/video/web only)",
        project.project_type
    );
    Ok(project)
}
pub fn project_directory(path: &Path) -> Result<PathBuf> {
    let manifest = canonical_wallpaper(path)?;
    anyhow::ensure!(
        is_project(&manifest),
        "Expected project.json or a Wallpaper Engine project directory"
    );
    Ok(manifest
        .parent()
        .context("Project has no parent directory")?
        .into())
}
pub fn project_preview(path: &Path) -> Result<PathBuf> {
    let directory = project_directory(path)?;
    let project = load_project(&directory)?;
    let candidates = project.preview.into_iter().chain(
        [
            "preview.jpg",
            "preview.png",
            "preview.jpeg",
            "preview.webp",
            "preview.gif",
        ]
        .map(String::from),
    );
    for name in candidates {
        let candidate = directory.join(name);
        if let Ok(canonical) = candidate.canonicalize()
            && canonical.starts_with(&directory)
            && canonical.is_file()
            && is_image(&canonical)
        {
            return Ok(canonical);
        }
    }
    bail!(
        "Wallpaper Engine project has no readable preview image in {}",
        directory.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn media_types_and_project_paths_are_explicit() {
        assert_eq!(kind(Path::new("clip.WEBM")), MediaKind::Video);
        assert!(supported(Path::new("project.json")));
        assert!(!supported(Path::new("settings.json")));
        assert_eq!(
            display_name(Path::new("/wallpapers/123/project.json")),
            "123"
        );
    }
    #[test]
    fn preview_stays_inside_project_and_manifest_is_canonical() -> Result<()> {
        let root = tempfile::tempdir()?;
        let dir = root.path().join("project");
        fs::create_dir(&dir)?;
        fs::write(
            dir.join("project.json"),
            r#"{"type":"scene","preview":"../outside.png"}"#,
        )?;
        image::RgbImage::new(2, 3).save(root.path().join("outside.png"))?;
        assert!(project_preview(&dir).is_err());
        image::RgbImage::new(2, 3).save(dir.join("preview.png"))?;
        assert_eq!(
            project_preview(&dir)?,
            dir.join("preview.png").canonicalize()?
        );
        assert_eq!(
            canonical_wallpaper(&dir)?,
            dir.join("project.json").canonicalize()?
        );
        fs::write(dir.join("project.json"), r#"{"type":"application"}"#)?;
        assert!(load_project(&dir).is_err());
        Ok(())
    }
}

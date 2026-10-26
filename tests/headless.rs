//! Exercise real CLI dispatch against isolated XDG directories and mock tools.
use anyhow::Result;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Output},
};
use wallpicker::{
    model::{CategoryRef, Library, Source},
    paths::Paths,
};
fn run(root: &Path, args: &[&str]) -> Result<Output> {
    Ok(Command::new(env!("CARGO_BIN_EXE_wallpicker"))
        .args(args)
        .env("HOME", root)
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("XDG_RUNTIME_DIR", root.join("runtime"))
        .env("PATH", root.join("bin"))
        .env_remove("HYPRLAND_INSTANCE_SIGNATURE")
        .env_remove("WAYLAND_DISPLAY")
        .output()?)
}
#[test]
fn first_run_rescan_creates_default_config_without_desktop_tools() -> Result<()> {
    let root = tempfile::tempdir()?;
    let result = run(root.path(), &["--quiet", "rescan"])?;
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("0 images"));
    assert!(root.path().join("config/wallpicker/config.toml").exists());
    assert!(root.path().join("data/wallpicker/library.json").exists());
    assert!(
        !root
            .path()
            .join("state/dotfiles/wallpaper/current")
            .exists()
    );
    Ok(())
}
#[test]
fn random_dispatch_falls_back_and_set_restore_share_legacy_state() -> Result<()> {
    let root = tempfile::tempdir()?;
    let base = root.path();
    fs::create_dir_all(base.join("bin"))?;
    fs::create_dir_all(base.join("source"))?;
    let image = base.join("source/image with spaces.png");
    image::RgbImage::new(2, 3).save(&image)?;
    // An existing daemon responds to query; no actual compositor command is executed.
    let tool = base.join("bin/awww");
    fs::write(&tool, "#!/bin/sh\nexit 0\n")?;
    fs::set_permissions(&tool, fs::Permissions::from_mode(0o700))?;
    let mut library = Library::default();
    library.add_source(Source {
        path: base.join("source"),
        recursive: true,
        auto_categories: true,
    })?;
    library.last_category = CategoryRef::Favorites;
    let paths = Paths::under(base);
    // Match discover's XDG app directory layout rather than the helper's short paths.
    let library_path = base.join("data/wallpicker/library.json");
    library.save(&library_path)?;
    for args in [
        vec!["--quiet", "random"],
        vec!["--quiet", "set", image.to_str().unwrap()],
        vec!["restore"],
    ] {
        let result = run(base, &args)?;
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            fs::read(base.join("state/dotfiles/wallpaper/current"))?,
            image.as_os_str().as_encoded_bytes()
        );
    }
    assert!(!paths.state.exists());
    Ok(())
}
#[test]
fn empty_random_fails_and_preserves_a_corrupt_library() -> Result<()> {
    let root = tempfile::tempdir()?;
    assert!(!run(root.path(), &["--quiet", "random"])?.status.success());
    let path = root.path().join("data/wallpicker/library.json");
    fs::write(&path, b"broken")?;
    let result = run(root.path(), &["--quiet", "rescan"])?;
    assert!(!result.status.success());
    assert_eq!(fs::read(&path)?, b"broken");
    assert!(String::from_utf8_lossy(&result.stderr).contains("backup"));
    Ok(())
}

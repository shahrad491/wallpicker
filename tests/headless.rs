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
fn command(root: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_wallpicker"));
    command
        .args(args)
        .env("HOME", root)
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("XDG_RUNTIME_DIR", root.join("runtime"))
        .env("PATH", root.join("bin"))
        .env_remove("HYPRLAND_INSTANCE_SIGNATURE")
        .env_remove("WAYLAND_DISPLAY");
    command
}
fn run(root: &Path, args: &[&str]) -> Result<Output> {
    Ok(command(root, args).output()?)
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

#[test]
fn rotation_loop_waits_before_first_change_and_once_uses_same_dispatch() -> Result<()> {
    use std::{
        io::Write,
        process::Child,
        time::{Duration, Instant},
    };
    struct Guard(Child);
    impl Drop for Guard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let root = tempfile::tempdir()?;
    let base = root.path();
    fs::create_dir_all(base.join("bin"))?;
    fs::create_dir_all(base.join("source"))?;
    let image = base.join("source/tiny.png");
    image::RgbImage::new(2, 3).save(&image)?;
    let mut fixture = tempfile::NamedTempFile::new_in(base.join("bin"))?;
    fixture.write_all(b"#!/bin/sh\nexit 0\n")?;
    fixture
        .as_file()
        .set_permissions(fs::Permissions::from_mode(0o700))?;
    fixture.into_temp_path().persist(base.join("bin/awww"))?;
    let mut library = Library::default();
    library.add_source(Source {
        path: base.join("source"),
        recursive: true,
        auto_categories: true,
    })?;
    library.save(&base.join("data/wallpicker/library.json"))?;
    let state = base.join("state/dotfiles/wallpaper/current");
    let mut child = Guard(command(base, &["--quiet", "rotate", "--interval", "1s"]).spawn()?);
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        !state.exists(),
        "Rotation must not swap immediately on service start"
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while !state.exists() && Instant::now() < deadline {
        assert!(
            child.0.try_wait()?.is_none(),
            "Rotation exited before its first change"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(fs::read(&state)?, image.as_os_str().as_encoded_bytes());
    drop(child);
    let output = run(base, &["--quiet", "rotate", "--once"])?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("tiny.png"));
    assert!(run(base, &["live", "stop"])?.status.success());
    Ok(())
}

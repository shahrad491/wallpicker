//! Wallpaper backend with injectable paths, legacy state compatibility and safe PID tracking.
use crate::{
    config::Config,
    model::{CategoryRef, Library, ScanResult, scan, store::atomic_write},
    paths::Paths,
};
use anyhow::{Context, Result, bail};
use rand::seq::IndexedRandom;
use std::{
    env,
    ffi::{OsStr, OsString},
    fs::{self, File, OpenOptions},
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::OpenOptionsExt,
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Duration,
};

pub type Transition = String;
#[derive(Debug, Clone)]
pub struct ApplyOptions {
    pub transition: Transition,
    pub duration: f32,
    pub notify: bool,
}
impl From<&Config> for ApplyOptions {
    fn from(config: &Config) -> Self {
        Self {
            transition: config.behavior.transition.clone(),
            duration: config.behavior.duration,
            notify: config.behavior.notify,
        }
    }
}
/// Executable paths can be replaced by fixtures without touching the live desktop.
#[derive(Debug, Clone)]
pub struct Tools {
    pub awww: PathBuf,
    pub daemon: PathBuf,
    pub swaybg: PathBuf,
    pub notify: PathBuf,
}
impl Default for Tools {
    fn default() -> Self {
        Self {
            awww: "awww".into(),
            daemon: "awww-daemon".into(),
            swaybg: "swaybg".into(),
            notify: "notify-send".into(),
        }
    }
}
#[derive(Clone)]
pub struct Backend {
    pub paths: Paths,
    pub config: Config,
    pub tools: Tools,
    runtime: PathBuf,
    session: String,
}
impl Backend {
    pub fn new(paths: Paths, config: Config) -> Self {
        // Match the legacy script's session-specific runtime directory.
        let runtime = env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| paths.cache.parent().unwrap_or(&paths.cache).into())
            .join("dotfiles-wallpaper");
        let session = env::var("HYPRLAND_INSTANCE_SIGNATURE")
            .or_else(|_| env::var("WAYLAND_DISPLAY"))
            .unwrap_or_else(|_| "default".into())
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || "_.-".contains(c) {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        Self {
            paths,
            config,
            tools: Tools::default(),
            runtime,
            session,
        }
    }
    pub fn with_runtime(
        paths: Paths,
        config: Config,
        runtime: PathBuf,
        session: String,
        tools: Tools,
    ) -> Self {
        Self {
            paths,
            config,
            tools,
            runtime,
            session,
        }
    }
    pub fn current(&self) -> Option<PathBuf> {
        fs::read(&self.paths.state)
            .ok()
            .filter(|b| !b.is_empty())
            .map(|b| PathBuf::from(OsString::from_vec(b)))
    }
    pub fn apply(&self, path: &Path) -> Result<()> {
        self.apply_with_options(path, &ApplyOptions::from(&self.config))
    }
    pub fn apply_with_options(&self, path: &Path, opts: &ApplyOptions) -> Result<()> {
        let result = self.apply_inner(path, opts);
        if let Err(error) = &result {
            log::error!("Applying wallpaper: {error:#}");
            if opts.notify {
                self.notify("Wallpaper picker", &format!("{error:#}"), true)
            }
        }
        result
    }
    fn apply_inner(&self, path: &Path, opts: &ApplyOptions) -> Result<()> {
        anyhow::ensure!(
            opts.duration.is_finite() && (0.0..=60.0).contains(&opts.duration),
            "Invalid transition duration"
        );
        anyhow::ensure!(
            [
                "random", "left", "right", "top", "bottom", "center", "outer", "fade", "none"
            ]
            .contains(&opts.transition.as_str()),
            "Invalid transition"
        );
        let path = path
            .canonicalize()
            .with_context(|| format!("Image missing: {}", path.display()))?;
        anyhow::ensure!(path.is_file(), "Wallpaper is not a file");
        File::open(&path).context("Wallpaper is unreadable")?;
        fs::create_dir_all(&self.runtime)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(self.runtime.join(format!("{}.apply.lock", self.session)))?;
        lock.lock()?;
        let old = self.read_pid();
        let animated = if executable(&self.tools.awww).is_some() {
            match self.apply_awww(&path, opts) {
                Ok(()) => true,
                Err(error) => {
                    log::warn!("awww failed, trying swaybg: {error:#}");
                    false
                }
            }
        } else {
            false
        };
        if animated {
            atomic_write(&self.paths.state, path.as_os_str().as_bytes())?;
            if let Some((pid, start)) = old {
                self.stop_static(pid, &start)
            }
            let _ = fs::remove_file(self.pid_path());
        } else {
            self.apply_static(&path, old)?;
        }
        drop(lock);
        if opts.notify {
            self.notify(
                "Wallpaper changed",
                &path
                    .file_name()
                    .unwrap_or(path.as_os_str())
                    .to_string_lossy(),
                false,
            )
        }
        Ok(())
    }
    fn apply_awww(&self, path: &Path, opts: &ApplyOptions) -> Result<()> {
        if !self.query() {
            let mut daemon = self.spawn_detached(
                &self.tools.daemon,
                &[
                    "--namespace".as_ref(),
                    "dotfiles".as_ref(),
                    "--no-cache".as_ref(),
                ],
                "animation",
            )?;
            let mut ready = false;
            for _ in 0..50 {
                if self.query() {
                    ready = true;
                    break;
                }
                if daemon.try_wait()?.is_some() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            if !ready {
                let _ = daemon.kill();
                let _ = daemon.wait();
                bail!("awww daemon did not become ready")
            }
            reap(daemon);
            if let Some(current) = self.current().filter(|p| p.is_file()) {
                let _ = Command::new(&self.tools.awww)
                    .args([
                        "img",
                        "--namespace",
                        "dotfiles",
                        "--transition-type",
                        "none",
                        "--",
                    ])
                    .arg(current)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
        }
        let effects = ["left", "right", "top", "bottom", "center", "outer"];
        let transition = if opts.transition == "random" {
            *effects.choose(&mut rand::rng()).context("No transitions")?
        } else {
            &opts.transition
        };
        let output = Command::new(&self.tools.awww)
            .args([
                "img",
                "--namespace",
                "dotfiles",
                "--resize",
                "crop",
                "--transition-type",
                transition,
                "--transition-duration",
                &opts.duration.to_string(),
                "--transition-fps",
                "60",
                "--transition-pos",
                "center",
                "--",
            ])
            .arg(path)
            .output()?;
        anyhow::ensure!(
            output.status.success(),
            "awww failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(())
    }
    fn query(&self) -> bool {
        Command::new(&self.tools.awww)
            .args(["query", "--namespace", "dotfiles"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }
    fn spawn_detached(&self, tool: &Path, args: &[&OsStr], kind: &str) -> Result<Child> {
        fs::create_dir_all(&self.paths.cache)?;
        let log = OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(
                self.paths
                    .cache
                    .join(format!("{}.{kind}.log", self.session)),
            )?;
        let mut cmd = Command::new(tool);
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log);
        // SAFETY: setsid is async-signal-safe; this closure does not allocate or lock.
        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() == -1 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
        cmd.spawn()
            .with_context(|| format!("Starting {}", tool.display()))
    }
    fn apply_static(&self, path: &Path, old: Option<(u32, String)>) -> Result<()> {
        let mut child = self.spawn_detached(
            &self.tools.swaybg,
            &[
                "-i".as_ref(),
                path.as_os_str(),
                "-m".as_ref(),
                "fill".as_ref(),
            ],
            "static",
        )?;
        std::thread::sleep(Duration::from_millis(400));
        anyhow::ensure!(
            child.try_wait()?.is_none(),
            "swaybg could not display the image; see {}",
            self.paths.cache.display()
        );
        let pid = child.id();
        let result = (|| -> Result<()> {
            let start = process_start(pid).context("swaybg exited before it was ready")?;
            self.save_static_selection(path, pid, &start)
        })();
        if let Err(error) = result {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        reap(child);
        if let Some((pid, start)) = old {
            self.stop_static(pid, &start)
        }
        Ok(())
    }
    // Publish the PID first, then the authoritative current path. If saving current
    // fails, restore both records before terminating the new process.
    fn save_static_selection(&self, path: &Path, pid: u32, start: &str) -> Result<()> {
        fn read_optional(path: &Path) -> Result<Option<Vec<u8>>> {
            match fs::read(path) {
                Ok(bytes) => Ok(Some(bytes)),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(error.into()),
            }
        }
        let pid_path = self.pid_path();
        let previous_pid = read_optional(&pid_path)?;
        let previous_state = read_optional(&self.paths.state)?;
        atomic_write(&pid_path, format!("{pid} {start}\n").as_bytes())?;
        if let Err(error) = atomic_write(&self.paths.state, path.as_os_str().as_bytes()) {
            for (record, previous) in [
                (&pid_path, previous_pid),
                (&self.paths.state, previous_state),
            ] {
                let rollback = match previous {
                    Some(bytes) => atomic_write(record, &bytes),
                    None => match fs::remove_file(record) {
                        Ok(()) => Ok(()),
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                        Err(error) => Err(error.into()),
                    },
                };
                if let Err(rollback_error) = rollback {
                    log::error!(
                        "Could not roll back {}: {rollback_error:#}",
                        record.display()
                    );
                }
            }
            return Err(error);
        }
        Ok(())
    }
    fn pid_path(&self) -> PathBuf {
        self.runtime.join(format!("{}.pid", self.session))
    }
    fn read_pid(&self) -> Option<(u32, String)> {
        let text = fs::read_to_string(self.pid_path()).ok()?;
        let mut fields = text.split_whitespace();
        Some((fields.next()?.parse().ok()?, fields.next()?.into()))
    }
    fn stop_static(&self, pid: u32, start: &str) {
        let expected = executable(&self.tools.swaybg);
        if expected.is_some()
            && process_start(pid).as_deref() == Some(start)
            && fs::read_link(format!("/proc/{pid}/exe")).ok() == expected
        {
            let _ = Command::new("kill")
                .args(["-TERM", "--", &pid.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
    pub fn notify(&self, title: &str, body: &str, critical: bool) {
        let mut cmd = Command::new(&self.tools.notify);
        if critical {
            cmd.args(["-u", "critical"]);
        }
        let _ = cmd
            .args(["--", title, body])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    pub fn restore(&self) -> Result<()> {
        if let Some(path) = self.current().filter(|p| p.is_file()) {
            self.apply_with_options(
                &path,
                &ApplyOptions {
                    transition: "none".into(),
                    duration: 0.0,
                    notify: false,
                },
            )?
        }
        Ok(())
    }
    pub fn random(&self, category: &CategoryRef, no_rescan: bool) -> Result<PathBuf> {
        let library = Library::load(&self.paths.library)?;
        let need_scan = matches!(category, CategoryRef::All | CategoryRef::Auto { .. });
        let mut result = if need_scan {
            scan(&library.sources)
        } else {
            ScanResult::default()
        };
        let mut paths = library.category_paths(category, &result);
        if paths.is_empty() {
            if result.pool.is_empty() {
                result = scan(&library.sources)
            }
            paths = library.category_paths(&CategoryRef::All, &result);
        }
        // All/auto have no persisted pool; --no-rescan can only reuse manual/favorite lists.
        let _ = no_rescan;
        let selection = select_random(
            &paths,
            self.current().as_deref(),
            self.config.behavior.avoid_repeat,
        );
        if let Err(error) = &selection
            && self.config.behavior.notify
        {
            self.notify("Wallpaper picker", &error.to_string(), true)
        }
        selection
    }
}
fn reap(mut child: Child) {
    std::thread::spawn(move || {
        let _ = child.wait();
    });
}
fn process_start(pid: u32) -> Option<String> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    stat.rsplit_once(") ")?
        .1
        .split_whitespace()
        .nth(19)
        .map(str::to_owned)
}
fn executable(tool: &Path) -> Option<PathBuf> {
    if tool.components().count() > 1 {
        return tool.canonicalize().ok();
    }
    env::split_paths(&env::var_os("PATH")?)
        .map(|p| p.join(tool))
        .find_map(|p| {
            use std::os::unix::fs::PermissionsExt;
            p.metadata()
                .ok()
                .filter(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)?;
            p.canonicalize().ok()
        })
}
pub fn select_random(
    paths: &[PathBuf],
    current: Option<&Path>,
    avoid_repeat: bool,
) -> Result<PathBuf> {
    let valid: Vec<_> = paths.iter().filter(|p| p.is_file()).collect();
    let candidates: Vec<_> = valid
        .iter()
        .filter(|p| !avoid_repeat || valid.len() < 2 || Some(p.as_path()) != current)
        .collect();
    candidates
        .choose(&mut rand::rng())
        .map(|p| (**p).clone())
        .context("No wallpapers available. Add a source directory in the picker.")
}
pub fn apply(path: &Path, opts: &ApplyOptions) -> Result<()> {
    let paths = Paths::discover()?;
    let config = Config::load(&paths.config)?;
    Backend::new(paths, config).apply_with_options(path, opts)
}
pub fn restore() -> Result<()> {
    let paths = Paths::discover()?;
    let config = Config::load(&paths.config)?;
    Backend::new(paths, config).restore()
}
pub fn current() -> Option<PathBuf> {
    let paths = Paths::discover().ok()?;
    Backend::new(paths, Config::default()).current()
}
pub fn pick_random(category: &CategoryRef, avoid_repeat: bool) -> Result<PathBuf> {
    let paths = Paths::discover()?;
    let mut config = Config::load(&paths.config)?;
    config.behavior.avoid_repeat = avoid_repeat;
    Backend::new(paths, config).random(category, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    fn script(path: &Path, contents: &str) {
        fs::write(path, format!("#!/bin/sh\n{contents}\n")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    fn fixture() -> (tempfile::TempDir, Backend, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths::under(root.path());
        let tools = Tools {
            awww: root.path().join("awww"),
            daemon: root.path().join("daemon"),
            swaybg: root.path().join("swaybg"),
            notify: root.path().join("notify"),
        };
        script(&tools.awww, "exit 0");
        script(&tools.daemon, "exit 0");
        script(&tools.notify, "exit 0");
        let mut config = Config::default();
        config.behavior.notify = false;
        let backend = Backend::with_runtime(
            paths,
            config,
            root.path().join("runtime"),
            "test".into(),
            tools,
        );
        let image = root.path().join("tiny.png");
        image::RgbImage::new(2, 3).save(&image).unwrap();
        (root, backend, image)
    }
    #[test]
    fn mock_apply_state_and_restore() {
        let (_root, backend, image) = fixture();
        backend.apply(&image).unwrap();
        assert_eq!(backend.current(), Some(image.canonicalize().unwrap()));
        assert_eq!(
            fs::read(&backend.paths.state).unwrap(),
            image.as_os_str().as_bytes()
        );
        backend.restore().unwrap();
    }
    #[test]
    fn apply_uses_legacy_namespace_and_sends_exactly_one_success_notification() {
        let (root, mut backend, image) = fixture();
        let args = root.path().join("args");
        let toasts = root.path().join("toasts");
        // Paths come from tempfile and contain no shell metacharacters; quote explicitly.
        let quoted_args = format!("'{}'", args.display().to_string().replace('\'', "'\\''"));
        let quoted_toasts = format!("'{}'", toasts.display().to_string().replace('\'', "'\\''"));
        script(
            &backend.tools.awww,
            &format!("if [ \"$1\" = img ]; then printf '%s\\n' \"$@\" > {quoted_args}; fi"),
        );
        script(
            &backend.tools.notify,
            &format!("printf 'toast\\n' >> {quoted_toasts}"),
        );
        backend.config.behavior.notify = true;
        backend.apply(&image).unwrap();
        let recorded = fs::read_to_string(args).unwrap();
        let lines = recorded.lines().collect::<Vec<_>>();
        assert!(
            lines
                .windows(2)
                .any(|pair| pair == ["--namespace", "dotfiles"])
        );
        assert_eq!(lines.last().copied(), image.to_str());
        assert_eq!(fs::read_to_string(toasts).unwrap(), "toast\n");
    }
    #[test]
    fn failed_apply_preserves_current() {
        let (_root, backend, image) = fixture();
        backend.apply(&image).unwrap();
        script(&backend.tools.awww, "exit 1");
        script(&backend.tools.daemon, "exit 1");
        assert!(backend.apply(&image).is_err());
        assert_eq!(backend.current(), Some(image));
    }
    #[test]
    fn random_avoids_current_and_handles_one_or_empty() {
        let (root, _backend, image) = fixture();
        let second = root.path().join("second.png");
        image::RgbImage::new(2, 2).save(&second).unwrap();
        assert_eq!(
            select_random(&[image.clone(), second.clone()], Some(&image), true).unwrap(),
            second
        );
        assert_eq!(
            select_random(std::slice::from_ref(&image), Some(&image), true).unwrap(),
            image
        );
        assert!(select_random(&[], None, true).is_err());
    }
    #[test]
    fn no_saved_state_restore_is_quiet_noop() {
        let (_root, backend, _image) = fixture();
        backend.restore().unwrap();
        assert!(!backend.paths.state.exists());
    }
    #[test]
    fn proc_start_parser_uses_kernel_start_time() {
        assert!(process_start(std::process::id()).is_some());
    }
    #[test]
    fn fallback_uses_injected_static_program() {
        let (_root, backend, image) = fixture();
        fs::remove_file(&backend.tools.awww).unwrap();
        script(&backend.tools.swaybg, "exec sleep 30");
        backend.apply(&image).unwrap();
        let (pid, start) = backend.read_pid().unwrap();
        // This is our fixture process; do not leave it running after the test.
        let _ = Command::new("kill")
            .args(["-TERM", "--", &pid.to_string()])
            .status();
        assert!(!start.is_empty());
        assert_eq!(backend.current(), Some(image));
    }
    #[test]
    fn random_empty_favorites_falls_back_to_scanned_sources() {
        use crate::model::Source;
        let (root, backend, image) = fixture();
        let mut library = Library::default();
        library
            .add_source(Source {
                path: root.path().into(),
                recursive: false,
                auto_categories: true,
            })
            .unwrap();
        library.save(&backend.paths.library).unwrap();
        assert_eq!(
            backend.random(&CategoryRef::Favorites, true).unwrap(),
            image
        );
    }
    #[test]
    fn existing_daemon_does_not_require_daemon_executable() {
        let (_root, backend, image) = fixture();
        fs::remove_file(&backend.tools.daemon).unwrap();
        backend.apply(&image).unwrap();
        assert_eq!(backend.current(), Some(image));
    }
    #[test]
    fn static_pid_failure_never_publishes_new_current() {
        let (root, backend, image) = fixture();
        backend.apply(&image).unwrap();
        let second = root.path().join("new.png");
        image::RgbImage::new(2, 2).save(&second).unwrap();
        fs::remove_file(&backend.tools.awww).unwrap();
        script(&backend.tools.swaybg, "exec sleep 30");
        fs::create_dir(backend.pid_path()).unwrap();
        assert!(backend.apply(&second).is_err());
        assert_eq!(backend.current(), Some(image));
    }
    #[test]
    fn empty_random_notifies_once_and_respects_quiet() {
        let (root, mut backend, _image) = fixture();
        Library::default().save(&backend.paths.library).unwrap();
        let notice = root.path().join("notice");
        script(
            &backend.tools.notify,
            &format!("printf '%s\\n' \"$@\" >> '{}'", notice.display()),
        );
        backend.config.behavior.notify = true;
        assert!(backend.random(&CategoryRef::All, false).is_err());
        let notification = fs::read_to_string(&notice).unwrap();
        assert_eq!(notification.matches("critical").count(), 1);
        backend.config.behavior.notify = false;
        assert!(backend.random(&CategoryRef::All, false).is_err());
        assert_eq!(fs::read_to_string(notice).unwrap(), notification);
    }
    #[test]
    fn manual_missing_files_and_hidden_auto_fall_back_to_all() {
        use crate::model::Source;
        let (root, backend, image) = fixture();
        let folder = root.path().join("folder");
        fs::create_dir(&folder).unwrap();
        let gone = folder.join("gone.png");
        image::RgbImage::new(2, 2).save(&gone).unwrap();
        let mut library = Library::default();
        library
            .add_source(Source {
                path: root.path().into(),
                recursive: true,
                auto_categories: true,
            })
            .unwrap();
        let id = library.add_category("Missing").unwrap();
        library.add_images(id, std::slice::from_ref(&gone)).unwrap();
        library.hidden_auto_categories.insert("folder".into());
        library.save(&backend.paths.library).unwrap();
        fs::remove_file(&gone).unwrap();
        assert_eq!(
            backend.random(&CategoryRef::Manual { id }, true).unwrap(),
            image
        );
        image::RgbImage::new(2, 2).save(&gone).unwrap();
        atomic_write(&backend.paths.state, gone.as_os_str().as_bytes()).unwrap();
        assert_eq!(
            backend
                .random(
                    &CategoryRef::Auto {
                        name: "folder".into()
                    },
                    false
                )
                .unwrap(),
            image
        );
    }
    #[test]
    fn stored_random_skips_unrelated_missing_source() {
        use crate::model::Source;
        let (root, backend, image) = fixture();
        let source = root.path().join("removed-source");
        fs::create_dir(&source).unwrap();
        let mut library = Library::default();
        library
            .add_source(Source {
                path: root.path().into(),
                recursive: false,
                auto_categories: false,
            })
            .unwrap();
        library.toggle_favorite(&image).unwrap();
        let id = library.add_category("Stored").unwrap();
        library
            .add_images(id, std::slice::from_ref(&image))
            .unwrap();
        // Keep another missing source in the persisted library: stored selection needs no traversal.
        library.sources.push(Source {
            path: root.path().with_file_name(format!(
                "{}-missing",
                root.path().file_name().unwrap().to_string_lossy()
            )),
            recursive: true,
            auto_categories: true,
        });
        library.save(&backend.paths.library).unwrap();
        assert_eq!(
            backend.random(&CategoryRef::Favorites, true).unwrap(),
            image
        );
        assert_eq!(
            backend.random(&CategoryRef::Manual { id }, true).unwrap(),
            image
        );
    }
}

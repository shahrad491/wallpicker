//! Command dispatch, diagnostics, and GUI instance ownership.
use anyhow::{Context, Result};
use clap::Parser;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::Path,
    process::Command as ProcessCommand,
    sync::Mutex,
};
use wallpicker::{
    cli::{Cli, Command},
    config::Config,
    model::{Library, Source, scan},
    paths::Paths,
};
struct FileLogger(Mutex<File>);
impl log::Log for FileLogger {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        if let Ok(mut file) = self.0.lock() {
            let _ = writeln!(
                file,
                "{:?} {} {}",
                std::time::SystemTime::now(),
                record.level(),
                record.args()
            );
        }
    }
    fn flush(&self) {
        if let Ok(mut file) = self.0.lock() {
            let _ = file.flush();
        }
    }
}
fn logging(paths: &Paths) -> Result<()> {
    fs::create_dir_all(&paths.cache)?;
    let logpath = paths.cache.join("wallpicker.log");
    if logpath.metadata().is_ok_and(|m| m.len() > 2 * 1024 * 1024) {
        let _ = fs::rename(&logpath, paths.cache.join("wallpicker.log.1"));
    }
    let file = OpenOptions::new().create(true).append(true).open(logpath)?;
    let logger = Box::leak(Box::new(FileLogger(Mutex::new(file))));
    let _ = log::set_logger(logger);
    log::set_max_level(log::LevelFilter::Info);
    std::panic::set_hook(Box::new(|info| {
        log::error!("panic: {info}");
        eprintln!("wallpicker: {info}");
    }));
    Ok(())
}
fn load_library(paths: &Paths) -> Result<Library> {
    let mut library = Library::load(&paths.library)?;
    if !paths.library.exists() && paths.default_source.is_dir() {
        library.add_source(Source {
            path: paths.default_source.clone(),
            recursive: true,
            auto_categories: true,
        })?;
    }
    Ok(library)
}
fn instance_lock(cache: &Path) -> Result<Option<File>> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(cache.join("picker.lock"))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(std::fs::TryLockError::WouldBlock) => {
            let _ = ProcessCommand::new("hyprctl")
                .args(["dispatch", "focuswindow", "class:^(wallpicker)$"])
                .output();
            Ok(None)
        }
        Err(std::fs::TryLockError::Error(error)) => Err(error.into()),
    }
}
fn run() -> Result<()> {
    let cli = Cli::parse();
    let paths = Paths::discover()?;
    logging(&paths)?;
    let mut config = Config::load(&paths.config)?;
    if !paths.config.exists() {
        config.save(&paths.config)?;
    }
    if cli.quiet {
        config.behavior.notify = false;
    }
    let library = load_library(&paths)?;
    if !paths.library.exists() {
        library.save(&paths.library)?;
    }
    let backend = wallpicker::wallpaper::Backend::new(paths.clone(), config.clone());
    match cli.command.unwrap_or(Command::Pick) {
        Command::Pick => {
            let Some(_lock) = instance_lock(&paths.cache)? else {
                return Ok(());
            };
            wallpicker::app::run(paths, config, library)
        }
        Command::Rescan => {
            let result = scan(&library.sources);
            println!(
                "{} images, {} folders, {} skipped ({:.0} ms)",
                result.pool.len(),
                result.auto_categories.len(),
                result.skipped.len(),
                result.duration.as_secs_f64() * 1000.0
            );
            for issue in result.skipped {
                eprintln!("{}: {}", issue.path.display(), issue.reason);
            }
            Ok(())
        }
        Command::Set { path } => backend.apply(&path),
        Command::Restore => backend.restore(),
        Command::Random {
            category,
            no_rescan,
        } => {
            let category = category
                .map(|name| library.resolve_category(&name))
                .transpose()?
                .unwrap_or(library.last_category);
            let path = backend.random(&category, no_rescan)?;
            backend.apply(&path)
        }
    }
}
fn main() {
    if let Err(error) = run().context("wallpicker failed") {
        log::error!("{error:#}");
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}

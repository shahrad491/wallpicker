//! Iced picker with asynchronous scanning, thumbnails, apply, and serialized saves.
use crate::{
    config::Config,
    message::Message,
    model::{CategoryRef, Library, ScanResult, Source, scan},
    paths::Paths,
    thumbs::Thumbnailer,
    wallpaper::Backend,
};
use iced::{
    Element, Length, Subscription, Task,
    keyboard::{self, key::Named},
    widget::{self, Space, button, column, container, pick_list, row, text, text_input},
};
use rand::prelude::IndexedRandom;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    process::Command,
    time::Duration,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    #[default]
    Name,
    Newest,
    Oldest,
}
impl std::fmt::Display for Sort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Name => "Name",
                Self::Newest => "Newest",
                Self::Oldest => "Oldest",
            }
        )
    }
}
#[derive(Debug, Clone)]
pub enum Dialog {
    Commands,
    NewCategory,
    Rename(u64),
    DeleteCategory(u64),
    Sources,
    AddSource,
    RemoveSource(PathBuf),
    AddImages,
    RemoveImages,
    Cleanup,
    CleanupCache,
    Hide(String),
    Context(PathBuf),
}
pub(crate) struct App {
    pub paths: Paths,
    pub config: Config,
    pub library: Library,
    pub scan: ScanResult,
    pub visible: Vec<PathBuf>,
    pub category: CategoryRef,
    pub selected: BTreeSet<PathBuf>,
    pub anchor: Option<usize>,
    pub active: Option<PathBuf>,
    pub hovered: Option<PathBuf>,
    pub cursor: iced::Point,
    pub context_position: iced::Point,
    pub current: Option<PathBuf>,
    pub thumbnails: BTreeMap<PathBuf, PathBuf>,
    pub thumbnail_errors: BTreeSet<PathBuf>,
    pub thumbnailer: Thumbnailer,
    pub search: String,
    pub sort: Sort,
    pub dialog: Option<Dialog>,
    pub dialog_text: String,
    pub command_index: usize,
    pub dialog_recursive: bool,
    pub dialog_auto: bool,
    pub error: Option<String>,
    pub status: String,
    pub scanning: bool,
    pub scan_again: bool,
    pub applying: bool,
    pub saving: bool,
    pub save_again: bool,
    pub close_requested: bool,
    pub modifiers: keyboard::Modifiers,
    pub width: f32,
    pub height: f32,
    pub scroll_y: f32,
}
pub fn run(paths: Paths, config: Config, library: Library) -> anyhow::Result<()> {
    let size = monitor_size(&config);
    let thumbs = Thumbnailer::new(paths.cache.join("thumbs"), config.thumbs.clone())?;
    iced::application(
        move || {
            App::new(
                paths.clone(),
                config.clone(),
                library.clone(),
                thumbs.clone(),
                size,
            )
        },
        App::update,
        App::view,
    )
    .title("wallpicker")
    .settings(iced::Settings {
        id: Some("wallpicker".into()),
        ..Default::default()
    })
    .window(iced::window::Settings {
        size,
        decorations: false,
        exit_on_close_request: false,
        position: iced::window::Position::Centered,
        min_size: Some(iced::Size::new(640.0, 400.0)),
        ..Default::default()
    })
    .theme(|a: &App| crate::theme::theme(&a.config.theme))
    .subscription(App::subscription)
    .run()?;
    Ok(())
}
pub fn monitor_size(config: &Config) -> iced::Size {
    let value = Command::new("hyprctl")
        .args(["monitors", "-j"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| serde_json::from_slice::<serde_json::Value>(&o.stdout).ok());
    size_from_monitors(value.as_ref(), config)
}
fn size_from_monitors(value: Option<&serde_json::Value>, config: &Config) -> iced::Size {
    let monitor = value.and_then(|v| v.as_array()).and_then(|a| {
        a.iter()
            .find(|m| m["focused"].as_bool() == Some(true))
            .or_else(|| a.first())
    });
    if let Some(m) = monitor {
        let scale = m["scale"].as_f64().unwrap_or(1.0).max(0.1);
        let mut w = m["width"].as_f64().unwrap_or(1200.0);
        let mut h = m["height"].as_f64().unwrap_or(800.0);
        if matches!(m["transform"].as_u64(), Some(1 | 3 | 5 | 7)) {
            std::mem::swap(&mut w, &mut h)
        }
        let w = (w / scale) as f32;
        let h = (h / scale) as f32;
        return iced::Size::new(
            (w * config.window.width_percent as f32 / 100.0).min(w),
            (h * config.window.height_percent as f32 / 100.0).min(h),
        );
    }
    iced::Size::new(1200.0, 800.0)
}
impl App {
    fn new(
        paths: Paths,
        config: Config,
        library: Library,
        thumbnailer: Thumbnailer,
        size: iced::Size,
    ) -> (Self, Task<Message>) {
        let category = library.last_category.clone();
        let current = Backend::new(paths.clone(), config.clone()).current();
        let mut app = Self {
            paths,
            config,
            library,
            scan: ScanResult::default(),
            visible: vec![],
            category,
            selected: BTreeSet::new(),
            anchor: None,
            active: None,
            hovered: None,
            cursor: iced::Point::new(300.0, 100.0),
            context_position: iced::Point::new(300.0, 100.0),
            current,
            thumbnails: BTreeMap::new(),
            thumbnail_errors: BTreeSet::new(),
            thumbnailer,
            search: String::new(),
            sort: Sort::Name,
            dialog: None,
            dialog_text: String::new(),
            command_index: 0,
            dialog_recursive: true,
            dialog_auto: true,
            error: None,
            status: "Scanning sources…".into(),
            scanning: false,
            scan_again: false,
            applying: false,
            saving: false,
            save_again: false,
            close_requested: false,
            modifiers: keyboard::Modifiers::default(),
            width: size.width,
            height: size.height,
            scroll_y: 0.0,
        };
        let task = app.start_scan();
        (app, task)
    }
    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            iced::event::listen_with(|event, status, _| {
                let global = matches!(&event,iced::Event::Keyboard(keyboard::Event::KeyPressed{key,modifiers,..}) if modifiers.control() || matches!(key,keyboard::Key::Named(Named::Escape|Named::ArrowUp|Named::ArrowDown)));
                if global
                    || matches!(
                        &event,
                        iced::Event::Mouse(iced::mouse::Event::CursorMoved { .. })
                    )
                    || status == iced::event::Status::Ignored
                {
                    Some(Message::Event(event))
                } else {
                    None
                }
            }),
            iced::time::every(Duration::from_millis(100)).map(|_| Message::Tick),
        ])
    }
    fn start_scan(&mut self) -> Task<Message> {
        if self.scanning {
            self.scan_again = true;
            return Task::none();
        }
        self.scanning = true;
        self.status = "Scanning sources…".into();
        let sources = self.library.sources.clone();
        Task::perform(async move { scan(&sources) }, Message::Scanned)
    }
    fn save(&mut self) -> Task<Message> {
        if self.saving {
            self.save_again = true;
            return Task::none();
        }
        self.saving = true;
        let library = self.library.clone();
        let path = self.paths.library.clone();
        Task::perform(
            async move { library.save(&path).map_err(|e| e.to_string()) },
            Message::Saved,
        )
    }
    pub(crate) fn title(&self) -> String {
        match &self.category {
            CategoryRef::All => "All wallpapers".into(),
            CategoryRef::Favorites => "Favorites".into(),
            CategoryRef::Manual { id } => self
                .library
                .categories
                .iter()
                .find(|c| c.id == *id)
                .map(|c| c.name.clone())
                .unwrap_or_else(|| "Category".into()),
            CategoryRef::Auto { name } => name.clone(),
        }
    }
    fn refresh(&mut self) {
        let paths: BTreeSet<_> = match &self.category {
            CategoryRef::All => self.scan.pool.iter().map(|e| e.path.clone()).collect(),
            CategoryRef::Favorites => self.library.favorites.iter().cloned().collect(),
            CategoryRef::Manual { id } => self
                .library
                .categories
                .iter()
                .find(|c| c.id == *id)
                .map(|c| c.images.iter().cloned().collect())
                .unwrap_or_default(),
            CategoryRef::Auto { name } => self
                .scan
                .auto_categories
                .get(name)
                .map(|v| v.iter().cloned().collect())
                .unwrap_or_default(),
        };
        let query = self.search.to_lowercase();
        let mut entries: Vec<_> = self
            .scan
            .pool
            .iter()
            .filter(|e| {
                paths.contains(&e.path)
                    && e.path
                        .file_name()
                        .is_some_and(|n| n.to_string_lossy().to_lowercase().contains(&query))
            })
            .collect();
        entries.sort_by(|a, b| match self.sort {
            Sort::Name => a
                .path
                .file_name()
                .cmp(&b.path.file_name())
                .then(a.path.cmp(&b.path)),
            Sort::Newest => b.mtime.cmp(&a.mtime).then(a.path.cmp(&b.path)),
            Sort::Oldest => a.mtime.cmp(&b.mtime).then(a.path.cmp(&b.path)),
        });
        self.visible = entries.into_iter().map(|e| e.path.clone()).collect();
        self.selected.retain(|p| self.visible.contains(p));
        if self
            .active
            .as_ref()
            .is_some_and(|p| !self.visible.contains(p))
        {
            self.active = None
        }
        self.anchor = self
            .active
            .as_ref()
            .and_then(|p| self.visible.iter().position(|v| v == p));
        self.schedule_previews();
    }
    pub(crate) fn columns(&self) -> usize {
        (((self.width - 300.0) / 230.0).floor() as usize).max(1)
    }
    pub(crate) fn tile_width(&self) -> f32 {
        ((self.width - 320.0) / self.columns() as f32 - 12.0).max(100.0)
    }
    pub(crate) fn row_height(&self) -> f32 {
        (self.tile_width() - 16.0) * 9.0 / 16.0 + 74.0
    }
    pub(crate) fn row_window(&self) -> std::ops::Range<usize> {
        let rows = self.visible.len().div_ceil(self.columns());
        let window_rows = (self.height / self.row_height()).ceil() as usize + 5;
        let start = ((self.scroll_y / self.row_height()).floor() as usize)
            .saturating_sub(2)
            .min(rows.saturating_sub(window_rows));
        let end = (start + window_rows).min(rows);
        start..end
    }
    fn preview_window(&self) -> std::ops::Range<usize> {
        let rows = self.row_window();
        let columns = self.columns();
        (rows.start * columns).min(self.visible.len())..(rows.end * columns).min(self.visible.len())
    }
    fn schedule_previews(&mut self) {
        let paths = &self.visible[self.preview_window()];
        let entries: Vec<_> = paths
            .iter()
            .filter_map(|path| {
                self.scan
                    .pool
                    .iter()
                    .find(|entry| &entry.path == path)
                    .cloned()
            })
            .collect();
        self.thumbnailer.prioritize(&entries);
        for entry in &entries {
            if self.thumbnails.contains_key(&entry.path)
                || self.thumbnail_errors.contains(&entry.path)
            {
                continue;
            }
            match self.thumbnailer.request(entry) {
                crate::thumbs::ThumbnailRequest::Cached(path) => {
                    self.thumbnails.insert(entry.path.clone(), path);
                }
                crate::thumbs::ThumbnailRequest::Failed(_) => {
                    self.thumbnail_errors.insert(entry.path.clone());
                }
                crate::thumbs::ThumbnailRequest::Pending => {}
            }
        }
    }
    fn preview_status(&self) -> String {
        let paths = &self.visible[self.preview_window()];
        let ready = paths
            .iter()
            .filter(|p| self.thumbnails.contains_key(*p))
            .count();
        let failed = paths
            .iter()
            .filter(|p| self.thumbnail_errors.contains(*p))
            .count();
        format!(
            "{} · previews {ready}/{}{}",
            self.status,
            paths.len(),
            if failed == 0 {
                String::new()
            } else {
                format!(" · {failed} unavailable")
            }
        )
    }
    fn apply(&mut self, path: PathBuf) -> Task<Message> {
        if self.applying {
            return Task::none();
        }
        self.applying = true;
        self.status = "Applying wallpaper…".into();
        let backend = Backend::new(self.paths.clone(), self.config.clone());
        Task::perform(
            async move { backend.apply(&path).map_err(|e| e.to_string()) },
            Message::Applied,
        )
    }
    fn choose(&mut self, path: PathBuf) {
        let index = self.visible.iter().position(|p| p == &path);
        select(
            &self.visible,
            &mut self.selected,
            self.anchor,
            index,
            self.modifiers.control(),
            self.modifiers.shift(),
        );
        if !self.modifiers.shift() {
            self.anchor = index
        }
        self.active = Some(path);
    }
    fn set_error(&mut self, result: anyhow::Result<()>) -> bool {
        match result {
            Ok(()) => true,
            Err(e) => {
                self.error = Some(e.to_string());
                false
            }
        }
    }
    pub(crate) fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ActivateCommand => {
                let commands = self.commands();
                if let Some((_, action)) = commands.get(self.command_index) {
                    let action = action.clone();
                    self.dialog = None;
                    self.dialog_text.clear();
                    return self.update(action);
                }
                Task::none()
            }
            Message::Tick => {
                for event in self.thumbnailer.drain() {
                    match event.result {
                        Ok(path) => {
                            if self
                                .scan
                                .pool
                                .iter()
                                .find(|entry| entry.path == event.path)
                                .and_then(|entry| self.thumbnailer.cached(entry))
                                .as_ref()
                                == Some(&path)
                            {
                                self.thumbnails.insert(event.path, path);
                            }
                        }
                        Err(e) => {
                            log::warn!("Thumbnail {}: {e}", event.path.display());
                            if let Some(entry) =
                                self.scan.pool.iter().find(|entry| entry.path == event.path)
                                && self.visible[self.preview_window()].contains(&event.path)
                                && matches!(
                                    self.thumbnailer.request(entry),
                                    crate::thumbs::ThumbnailRequest::Failed(_)
                                )
                            {
                                self.thumbnail_errors.insert(event.path);
                            }
                        }
                    }
                }
                Task::none()
            }
            Message::Event(event) => {
                match event {
                    iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                        self.cursor = position
                    }
                    iced::Event::Window(iced::window::Event::CloseRequested) => {
                        return self.update(Message::Close);
                    }
                    iced::Event::Window(iced::window::Event::Resized(size)) => {
                        self.width = size.width;
                        self.height = size.height;
                        self.schedule_previews();
                    }
                    iced::Event::Keyboard(keyboard::Event::ModifiersChanged(m)) => {
                        self.modifiers = m
                    }
                    iced::Event::Keyboard(keyboard::Event::KeyPressed {
                        key, modifiers, ..
                    }) => {
                        if key == keyboard::Key::Named(Named::Escape) {
                            return self.update(if self.dialog.is_some() {
                                Message::Cancel
                            } else {
                                Message::Close
                            });
                        }
                        if modifiers.control() && key.as_ref() == keyboard::Key::Character("p") {
                            return self.update(Message::OpenDialog(Dialog::Commands));
                        }
                        if let Some(dialog) = &self.dialog {
                            if matches!(dialog, Dialog::Commands) {
                                let count = self.commands().len();
                                match key {
                                    keyboard::Key::Named(Named::ArrowDown) => {
                                        self.command_index =
                                            (self.command_index + 1).min(count.saturating_sub(1));
                                        return Task::none();
                                    }
                                    keyboard::Key::Named(Named::ArrowUp) => {
                                        self.command_index = self.command_index.saturating_sub(1);
                                        return Task::none();
                                    }
                                    keyboard::Key::Named(Named::Enter) => {
                                        return self.update(Message::ActivateCommand);
                                    }
                                    _ => {}
                                }
                            } else if matches!(dialog, Dialog::AddSource) && modifiers.control() {
                                match key.as_ref() {
                                    keyboard::Key::Character("r") => {
                                        self.dialog_recursive = !self.dialog_recursive;
                                        return Task::none();
                                    }
                                    keyboard::Key::Character("f") => {
                                        self.dialog_auto = !self.dialog_auto;
                                        return Task::none();
                                    }
                                    keyboard::Key::Character("b") => {
                                        return self.update(Message::Browse);
                                    }
                                    _ => {}
                                }
                            } else if matches!(key, keyboard::Key::Named(Named::Enter))
                                && matches!(
                                    dialog,
                                    Dialog::DeleteCategory(_)
                                        | Dialog::RemoveSource(_)
                                        | Dialog::RemoveImages
                                        | Dialog::Cleanup
                                        | Dialog::CleanupCache
                                        | Dialog::Hide(_)
                                )
                            {
                                return self.update(Message::Confirm);
                            }
                        }
                        if self.dialog.is_none() {
                            return widget::operation::is_focused("search").map(move |focused| {
                                if focused {
                                    Message::Noop
                                } else {
                                    Message::Key(key.clone(), modifiers)
                                }
                            });
                        }
                    }
                    _ => {}
                }
                Task::none()
            }
            Message::Key(key, m) => {
                match key.as_ref() {
                    keyboard::Key::Named(Named::Enter) => {
                        return self.update(Message::ApplySelected);
                    }
                    keyboard::Key::Named(Named::Space) => {
                        return self.update(Message::FavoriteSelected);
                    }
                    keyboard::Key::Named(Named::F2) => {
                        if let CategoryRef::Manual { id } = self.category {
                            return self.update(Message::OpenDialog(Dialog::Rename(id)));
                        }
                    }
                    keyboard::Key::Character("/") => return widget::operation::focus("search"),
                    keyboard::Key::Character("a") if m.control() => {
                        return self.update(Message::SelectAll);
                    }
                    keyboard::Key::Character("d") if m.control() => {
                        return self.update(Message::ClearSelection);
                    }
                    keyboard::Key::Character("r" | "R") => return self.update(Message::Random),
                    _ => {
                        let step = match key.as_ref() {
                            keyboard::Key::Named(Named::ArrowLeft)
                            | keyboard::Key::Character("h") => -1,
                            keyboard::Key::Named(Named::ArrowRight)
                            | keyboard::Key::Character("l") => 1,
                            keyboard::Key::Named(Named::ArrowUp)
                            | keyboard::Key::Character("k") => -(self.columns() as isize),
                            keyboard::Key::Named(Named::ArrowDown)
                            | keyboard::Key::Character("j") => self.columns() as isize,
                            _ => 0,
                        };
                        if step != 0 && !self.visible.is_empty() {
                            let old = self
                                .active
                                .as_ref()
                                .and_then(|p| self.visible.iter().position(|v| v == p))
                                .unwrap_or(0);
                            let next = (old as isize + step)
                                .clamp(0, self.visible.len() as isize - 1)
                                as usize;
                            self.modifiers = m;
                            self.choose(self.visible[next].clone());
                            let row = next / self.columns();
                            let y = row as f32 * self.row_height();
                            return widget::operation::scroll_to(
                                "gallery",
                                widget::scrollable::AbsoluteOffset {
                                    x: 0.0,
                                    y: (y - (self.height - 300.0).max(100.0) / 2.0).max(0.0),
                                },
                            );
                        }
                    }
                }
                Task::none()
            }
            Message::SearchSubmitted => {
                if let Some(path) = self.visible.first().cloned() {
                    self.modifiers = keyboard::Modifiers::default();
                    self.choose(path);
                }
                // Gallery is not focusable; focusing its ID releases text-input focus.
                widget::operation::focus("gallery")
            }
            Message::Search(value) => {
                self.search = value;
                self.scroll_y = 0.0;
                self.refresh();
                widget::operation::snap_to("gallery", widget::scrollable::RelativeOffset::START)
            }
            Message::Sort(sort) => {
                self.sort = sort;
                self.refresh();
                Task::none()
            }
            Message::Category(category) => {
                self.category = category.clone();
                self.library.set_last_category(category);
                self.selected.clear();
                self.active = None;
                self.scroll_y = 0.0;
                self.refresh();
                Task::batch([
                    self.save(),
                    widget::operation::snap_to(
                        "gallery",
                        widget::scrollable::RelativeOffset::START,
                    ),
                ])
            }
            Message::Rescan => self.start_scan(),
            Message::Scanned(scan) => {
                self.status = format!(
                    "{} wallpapers · {} skipped · {:.0} ms",
                    scan.pool.len(),
                    scan.skipped.len(),
                    scan.duration.as_secs_f64() * 1000.0
                );
                self.scanning = false;
                self.thumbnails.clear();
                self.thumbnail_errors.clear();
                self.scan = scan;
                self.refresh();
                if self.scan_again {
                    self.scan_again = false;
                    self.start_scan()
                } else {
                    Task::none()
                }
            }
            Message::Click(path) => {
                self.choose(path);
                Task::none()
            }
            Message::Apply(path) => self.apply(path),
            Message::ApplySelected => {
                if let Some(path) = self
                    .active
                    .clone()
                    .or_else(|| self.selected.first().cloned())
                {
                    self.apply(path)
                } else {
                    Task::none()
                }
            }
            Message::Applied(result) => {
                self.applying = false;
                match result {
                    Ok(()) => self.update(Message::Close),
                    Err(e) => {
                        self.error = Some(e);
                        self.status = "Apply failed".into();
                        Task::none()
                    }
                }
            }
            Message::Favorite(path) => {
                let result = self.library.toggle_favorite(&path).map(|_| ());
                if self.set_error(result) {
                    self.refresh();
                    self.save()
                } else {
                    Task::none()
                }
            }
            Message::FavoriteSelected => {
                let selected = self.selected.iter().cloned().collect::<Vec<_>>();
                for path in selected {
                    let result = self.library.toggle_favorite(&path).map(|_| ());
                    self.set_error(result);
                }
                self.refresh();
                self.save()
            }
            Message::SelectAll => {
                self.selected = self.visible.iter().cloned().collect();
                Task::none()
            }
            Message::ClearSelection => {
                self.selected.clear();
                Task::none()
            }
            Message::Random => {
                let pool: Vec<_> = if self.visible.is_empty() {
                    self.scan.pool.iter().map(|e| e.path.clone()).collect()
                } else {
                    self.visible.clone()
                };
                let choices: Vec<_> = pool
                    .iter()
                    .filter(|p| {
                        !self.config.behavior.avoid_repeat
                            || pool.len() < 2
                            || Some(*p) != self.current.as_ref()
                    })
                    .cloned()
                    .collect();
                if let Some(path) = choices.choose(&mut rand::rng()) {
                    self.apply(path.clone())
                } else {
                    self.error =
                        Some("This category has no wallpapers. Select All or add a source.".into());
                    Task::none()
                }
            }
            Message::Hover(path) => {
                self.hovered = path;
                Task::none()
            }
            Message::Context(path) => {
                if !self.selected.contains(&path) {
                    self.selected.clear();
                    self.selected.insert(path.clone());
                }
                self.active = Some(path.clone());
                self.context_position = self.cursor;
                self.dialog = Some(Dialog::Context(path));
                Task::none()
            }
            Message::Copy(path) => {
                self.dialog = None;
                iced::clipboard::write(path.to_string_lossy().into_owned())
            }
            Message::Reveal(path) => {
                self.dialog = None;
                Task::perform(
                    async move {
                        let parent = path.parent().unwrap_or(&path);
                        Command::new("xdg-open")
                            .arg(parent)
                            .spawn()
                            .map(|_| ())
                            .map_err(|e| e.to_string())
                    },
                    |r| match r {
                        Ok(()) => Message::Noop,
                        Err(e) => Message::Applied(Err(e)),
                    },
                )
            }
            Message::OpenDialog(dialog) => {
                self.dialog_text = match &dialog {
                    Dialog::Rename(id) => self
                        .library
                        .categories
                        .iter()
                        .find(|c| c.id == *id)
                        .map(|c| c.name.clone())
                        .unwrap_or_default(),
                    _ => String::new(),
                };
                self.command_index = 0;
                self.dialog = Some(dialog);
                self.error = None;
                widget::operation::focus("dialog-input")
            }
            Message::DialogText(value) => {
                self.dialog_text = value;
                self.command_index = 0;
                Task::none()
            }
            Message::DialogRecursive(value) => {
                self.dialog_recursive = value;
                Task::none()
            }
            Message::DialogAuto(value) => {
                self.dialog_auto = value;
                Task::none()
            }
            Message::Cancel => {
                self.dialog = None;
                self.error = None;
                Task::none()
            }
            Message::Confirm => self.confirm(),
            Message::Browse => Task::perform(
                async {
                    match Command::new("zenity")
                        .args([
                            "--file-selection",
                            "--directory",
                            "--title=Choose wallpaper source",
                        ])
                        .output()
                    {
                        Ok(o) if o.status.success() => {
                            Ok(Some(String::from_utf8_lossy(&o.stdout).trim_end().into()))
                        }
                        Ok(o) if o.status.code() == Some(1) => Ok(None),
                        Ok(o) => Err(format!(
                            "Directory picker exited {}. Enter the path manually.",
                            o.status
                        )),
                        Err(e) => Err(format!(
                            "Directory picker unavailable ({e}). Enter the path manually."
                        )),
                    }
                },
                Message::Browsed,
            ),
            Message::Browsed(result) => {
                match result {
                    Ok(Some(path)) => self.dialog_text = path,
                    Ok(None) => {}
                    Err(e) => self.error = Some(e),
                }
                Task::none()
            }
            Message::AddImage(path) => {
                if !self.selected.contains(&path) {
                    self.choose(path);
                }
                self.update(Message::OpenDialog(Dialog::AddImages))
            }
            Message::AddTo(id) => {
                let paths = self.selected.iter().cloned().collect::<Vec<_>>();
                let result = self.library.add_images(id, &paths);
                if self.set_error(result) {
                    self.dialog = None;
                    self.save()
                } else {
                    Task::none()
                }
            }
            Message::RemoveSelected => self.update(Message::OpenDialog(Dialog::RemoveImages)),
            Message::HideFolder(name) => self.update(Message::OpenDialog(Dialog::Hide(name))),
            Message::UnhideFolder(name) => {
                self.library.hidden_auto_categories.remove(&name);
                self.save()
            }
            Message::ToggleSourceRecursive(index, value) => {
                if let Some(source) = self.library.sources.get_mut(index) {
                    source.recursive = value;
                }
                Task::batch([self.save(), self.start_scan()])
            }
            Message::ToggleSourceAuto(index, value) => {
                if let Some(source) = self.library.sources.get_mut(index) {
                    source.auto_categories = value;
                }
                Task::batch([self.save(), self.start_scan()])
            }
            Message::Saved(result) => {
                self.saving = false;
                if let Err(e) = result {
                    self.error = Some(format!("Could not save library: {e}"));
                }
                if self.save_again {
                    self.save_again = false;
                    self.save()
                } else if self.close_requested && self.error.is_none() {
                    iced::exit()
                } else {
                    Task::none()
                }
            }
            Message::DismissError => {
                self.error = None;
                Task::none()
            }
            Message::Scroll(y) => {
                self.scroll_y = y;
                self.schedule_previews();
                Task::none()
            }
            Message::CleanCache => self.update(Message::OpenDialog(Dialog::CleanupCache)),
            Message::DoCleanCache => {
                let thumbs = self.thumbnailer.clone();
                let entries = self.scan.pool.clone();
                Task::perform(
                    async move { thumbs.cleanup(&entries).map_err(|e| e.to_string()) },
                    Message::CacheCleaned,
                )
            }
            Message::CacheCleaned(result) => {
                match result {
                    Ok(n) => self.status = format!("Removed {n} orphaned thumbnails"),
                    Err(e) => self.error = Some(e),
                }
                Task::none()
            }
            Message::Close => {
                if self.saving {
                    self.close_requested = true;
                    self.status = "Saving changes…".into();
                    Task::none()
                } else {
                    iced::exit()
                }
            }
            Message::Noop => Task::none(),
        }
    }
    pub(crate) fn commands(&self) -> Vec<(String, Message)> {
        let mut commands = vec![
            (
                "Show All wallpapers".into(),
                Message::Category(CategoryRef::All),
            ),
            (
                "Show Favorites".into(),
                Message::Category(CategoryRef::Favorites),
            ),
            (
                "Create category".into(),
                Message::OpenDialog(Dialog::NewCategory),
            ),
            ("Add source".into(), Message::OpenDialog(Dialog::AddSource)),
            (
                "Manage sources".into(),
                Message::OpenDialog(Dialog::Sources),
            ),
            ("Rescan sources".into(), Message::Rescan),
            (
                "Clean missing entries".into(),
                Message::OpenDialog(Dialog::Cleanup),
            ),
            ("Clean orphaned thumbnail cache".into(), Message::CleanCache),
            ("Sort by name".into(), Message::Sort(Sort::Name)),
            ("Sort newest first".into(), Message::Sort(Sort::Newest)),
            ("Sort oldest first".into(), Message::Sort(Sort::Oldest)),
            ("Random wallpaper".into(), Message::Random),
            ("Select all images".into(), Message::SelectAll),
            ("Clear selection".into(), Message::ClearSelection),
        ];
        for c in &self.library.categories {
            commands.push((
                format!("Show category: {}", c.name),
                Message::Category(CategoryRef::Manual { id: c.id }),
            ));
            commands.push((
                format!("Rename category: {}", c.name),
                Message::OpenDialog(Dialog::Rename(c.id)),
            ));
            commands.push((
                format!("Delete category: {}", c.name),
                Message::OpenDialog(Dialog::DeleteCategory(c.id)),
            ));
            if !self.selected.is_empty() {
                commands.push((
                    format!("Add selection to category: {}", c.name),
                    Message::AddTo(c.id),
                ));
            }
        }
        for name in self.scan.auto_categories.keys() {
            if self.library.hidden_auto_categories.contains(name) {
                commands.push((
                    format!("Unhide folder: {name}"),
                    Message::UnhideFolder(name.clone()),
                ))
            } else {
                commands.push((
                    format!("Show folder: {name}"),
                    Message::Category(CategoryRef::Auto { name: name.clone() }),
                ));
                commands.push((
                    format!("Hide folder: {name}"),
                    Message::HideFolder(name.clone()),
                ));
            }
        }
        for name in &self.library.hidden_auto_categories {
            if !self.scan.auto_categories.contains_key(name) {
                commands.push((
                    format!("Unhide folder: {name}"),
                    Message::UnhideFolder(name.clone()),
                ));
            }
        }
        for (i, source) in self.library.sources.iter().enumerate() {
            let name = source.path.display();
            commands.push((
                format!("Remove source: {name}"),
                Message::OpenDialog(Dialog::RemoveSource(source.path.clone())),
            ));
            commands.push((
                format!("Toggle recursive scanning: {name}"),
                Message::ToggleSourceRecursive(i, !source.recursive),
            ));
            commands.push((
                format!("Toggle folder categories: {name}"),
                Message::ToggleSourceAuto(i, !source.auto_categories),
            ));
        }
        if !self.selected.is_empty() {
            commands.push((
                "Toggle selected favorites".into(),
                Message::FavoriteSelected,
            ));
            if matches!(
                self.category,
                CategoryRef::Manual { .. } | CategoryRef::Favorites
            ) {
                commands.push((
                    "Remove selected memberships".into(),
                    Message::RemoveSelected,
                ));
            }
        }
        if let Some(path) = &self.active {
            commands.push((
                "Apply selected wallpaper".into(),
                Message::Apply(path.clone()),
            ));
            commands.push((
                "Reveal selected wallpaper".into(),
                Message::Reveal(path.clone()),
            ));
            commands.push(("Copy selected path".into(), Message::Copy(path.clone())));
        }
        let query = self.dialog_text.to_lowercase();
        commands
            .into_iter()
            .filter(|(name, _)| name.to_lowercase().contains(&query))
            .collect()
    }
    fn confirm(&mut self) -> Task<Message> {
        let Some(dialog) = self.dialog.clone() else {
            return Task::none();
        };
        if matches!(dialog, Dialog::CleanupCache) {
            self.dialog = None;
            return self.update(Message::DoCleanCache);
        }
        let rescan = matches!(dialog, Dialog::AddSource | Dialog::RemoveSource(_));
        let result = match dialog {
            Dialog::NewCategory => self.library.add_category(&self.dialog_text).map(|_| ()),
            Dialog::Rename(id) => self.library.rename_category(id, &self.dialog_text),
            Dialog::DeleteCategory(id) => {
                let result = self.library.delete_category(id);
                if result.is_ok() {
                    self.category = self.library.last_category.clone()
                }
                result
            }
            Dialog::AddSource => {
                let input = self.dialog_text.trim();
                let path = if input == "~" || input.starts_with("~/") {
                    std::env::var_os("HOME")
                        .map(PathBuf::from)
                        .map(|h| h.join(input.trim_start_matches('~').trim_start_matches('/')))
                        .unwrap_or_else(|| PathBuf::from(input))
                } else {
                    PathBuf::from(input)
                };
                self.library.add_source(Source {
                    path,
                    recursive: self.dialog_recursive,
                    auto_categories: self.dialog_auto,
                })
            }
            Dialog::RemoveSource(path) => self.library.remove_source(&path).map(|r| {
                self.status = format!("Removed {} memberships; files untouched", r.removed)
            }),
            Dialog::RemoveImages => {
                let paths = self.selected.iter().cloned().collect::<Vec<_>>();
                match self.category {
                    CategoryRef::Manual { id } => self.library.remove_images(id, &paths),
                    CategoryRef::Favorites => {
                        self.library.favorites.retain(|p| !paths.contains(p));
                        Ok(())
                    }
                    _ => Ok(()),
                }
            }
            Dialog::Cleanup => {
                let report = self.library.prune_missing();
                self.status = format!("Cleaned {} missing memberships", report.removed);
                Ok(())
            }
            Dialog::Hide(name) => {
                self.library.hidden_auto_categories.insert(name);
                self.category = CategoryRef::All;
                self.library.set_last_category(CategoryRef::All);
                Ok(())
            }
            _ => Ok(()),
        };
        if self.set_error(result) {
            self.dialog = None;
            self.refresh();
            if rescan {
                Task::batch([self.save(), self.start_scan()])
            } else {
                self.save()
            }
        } else {
            Task::none()
        }
    }
    pub(crate) fn view(&self) -> Element<'_, Message> {
        let toolbar = row![
            text_input("Search wallpapers  /", &self.search)
                .id("search")
                .on_input(Message::Search)
                .on_submit(Message::SearchSubmitted)
                .padding(11)
                .width(Length::Fill),
            pick_list(
                [Sort::Name, Sort::Newest, Sort::Oldest],
                Some(self.sort),
                Message::Sort
            ),
            button("Random  R").on_press_maybe((!self.applying).then_some(Message::Random)),
            button(if self.scanning {
                "Scanning…"
            } else {
                "Rescan"
            })
            .on_press_maybe((!self.scanning).then_some(Message::Rescan))
        ]
        .spacing(10)
        .align_y(iced::Alignment::Center);
        let heading = row![
            column![
                text(self.title()).size(28),
                text(format!(
                    "{} images · find your next backdrop",
                    self.visible.len()
                ))
                .size(13)
                .color(crate::theme::color(&self.config.theme.muted))
            ]
            .spacing(5),
            Space::new().width(Length::Fill),
            button("×").on_press(Message::Close)
        ]
        .align_y(iced::Alignment::Center);
        // Preserve fixed child positions: the gallery's Iced state must survive
        // selection and error changes, including its scroll offset.
        let error_content: Element<'_, Message> = if let Some(error) = &self.error {
            container(
                row![
                    text(error).size(13).width(Length::Fill),
                    button("Dismiss").on_press(Message::DismissError)
                ]
                .spacing(10),
            )
            .padding(12)
            .style(container::danger)
            .into()
        } else {
            Space::new().height(0).into()
        };
        let has_selection = !self.selected.is_empty();
        let batch_toolbar = row![
            text(if has_selection {
                format!("{} selected", self.selected.len())
            } else {
                "Select wallpapers".into()
            })
            .size(13),
            button("☆ Favorite").on_press_maybe(has_selection.then_some(Message::FavoriteSelected)),
            button("Add to category")
                .on_press_maybe(has_selection.then_some(Message::OpenDialog(Dialog::AddImages))),
            button("Remove").on_press_maybe(
                (has_selection
                    && matches!(
                        self.category,
                        CategoryRef::Manual { .. } | CategoryRef::Favorites
                    ))
                .then_some(Message::RemoveSelected)
            ),
            button("Clear").on_press_maybe(has_selection.then_some(Message::ClearSelection)),
            button("Apply ↵").on_press_maybe(
                (has_selection && !self.applying).then_some(Message::ApplySelected)
            )
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);
        let mut content =
            column![heading, toolbar, container(error_content), batch_toolbar].spacing(14);
        content = content.push(crate::ui::gallery::view(self)).push(
            row![
                text(self.preview_status()).size(12),
                Space::new().width(Length::Fill),
                text("Ctrl+P  commands     Space  favorite     ↵  apply     Esc  close").size(12)
            ]
            .spacing(8),
        );
        let base = container(
            row![
                crate::ui::sidebar::view(self),
                container(content)
                    .padding(24)
                    .width(Length::Fill)
                    .height(Length::Fill)
            ]
            .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill);
        if matches!(self.dialog, Some(Dialog::Context(_))) {
            let anchor = self.context_position;
            widget::stack![
                base,
                widget::mouse_area(Space::new().width(Length::Fill).height(Length::Fill))
                    .on_press(Message::Cancel)
                    .on_right_press(Message::Cancel),
                widget::float(widget::opaque(crate::ui::dialogs::view(self))).translate(
                    move |bounds, viewport| context_translation(anchor, bounds, viewport)
                )
            ]
            .into()
        } else if self.dialog.is_some() {
            widget::stack![
                base,
                widget::opaque(
                    container(crate::ui::dialogs::view(self))
                        .center_x(Length::Fill)
                        .center_y(Length::Fill)
                        .style(|_| container::Style {
                            background: Some(iced::Color::from_rgba(0.0, 0.0, 0.0, 0.68).into()),
                            ..Default::default()
                        })
                )
            ]
            .into()
        } else {
            widget::stack![base].into()
        }
    }
}
fn context_translation(
    anchor: iced::Point,
    bounds: iced::Rectangle,
    viewport: iced::Rectangle,
) -> iced::Vector {
    let x = anchor.x.clamp(
        viewport.x,
        (viewport.x + viewport.width - bounds.width).max(viewport.x),
    );
    let y = anchor.y.clamp(
        viewport.y,
        (viewport.y + viewport.height - bounds.height).max(viewport.y),
    );
    iced::Vector::new(x - bounds.x, y - bounds.y)
}

fn select(
    visible: &[PathBuf],
    selected: &mut BTreeSet<PathBuf>,
    anchor: Option<usize>,
    index: Option<usize>,
    ctrl: bool,
    shift: bool,
) {
    let Some(index) = index else { return };
    if shift {
        if !ctrl {
            selected.clear()
        }
        let anchor = anchor.unwrap_or(index).min(visible.len().saturating_sub(1));
        for p in &visible[anchor.min(index)..=anchor.max(index)] {
            selected.insert(p.clone());
        }
    } else if ctrl {
        let path = &visible[index];
        if !selected.remove(path) {
            selected.insert(path.clone());
        }
    } else {
        selected.clear();
        selected.insert(visible[index].clone());
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_range_and_toggle() {
        let paths = vec!["a".into(), "b".into(), "c".into()];
        let mut selected = BTreeSet::new();
        select(&paths, &mut selected, None, Some(0), false, false);
        assert_eq!(selected.len(), 1);
        select(&paths, &mut selected, Some(0), Some(2), false, true);
        assert_eq!(selected.len(), 3);
        select(&paths, &mut selected, None, Some(1), true, false);
        assert!(!selected.contains(&PathBuf::from("b")));
    }
    #[test]
    fn scaled_focused_monitor() {
        let monitors = serde_json::json!([{"width":3840,"height":2160,"scale":2.0,"focused":false},{"width":1920,"height":1080,"scale":1.5,"focused":true}]);
        let size = size_from_monitors(Some(&monitors), &Config::default());
        assert_eq!(size, iced::Size::new(1088.0, 612.0));
    }
    #[test]
    fn monitor_fallback_and_rotated_screen() {
        assert_eq!(
            size_from_monitors(None, &Config::default()),
            iced::Size::new(1200.0, 800.0)
        );
        let monitors = serde_json::json!([{"width":1920,"height":1080,"scale":1.0,"transform":1,"focused":true}]);
        assert_eq!(
            size_from_monitors(Some(&monitors), &Config::default()),
            iced::Size::new(918.0, 1632.0)
        );
    }
    fn fixture() -> (tempfile::TempDir, App) {
        let temp = tempfile::tempdir().expect("temporary directory");
        let paths = Paths::under(temp.path());
        let config = Config::default();
        let thumbs = Thumbnailer::new(paths.cache.join("thumbs"), config.thumbs.clone())
            .expect("thumbnail service");
        let (app, _) = App::new(
            paths,
            config,
            Library::default(),
            thumbs,
            iced::Size::new(1200.0, 800.0),
        );
        (temp, app)
    }
    #[test]
    fn category_switch_persists_and_clears_selection() {
        let (_temp, mut app) = fixture();
        app.selected.insert(PathBuf::from("old"));
        let _ = app.update(Message::Category(CategoryRef::Favorites));
        assert_eq!(app.library.last_category, CategoryRef::Favorites);
        assert!(app.selected.is_empty());
        assert!(app.saving);
        let _ = app.update(Message::Category(CategoryRef::All));
        assert!(app.save_again);
    }
    #[test]
    fn close_waits_for_latest_save_and_apply_failure_stays_open() {
        let (_temp, mut app) = fixture();
        app.saving = true;
        let _ = app.update(Message::Close);
        assert!(app.close_requested);
        app.close_requested = false;
        app.applying = true;
        let _ = app.update(Message::Applied(Err("test backend failure".into())));
        assert!(!app.applying);
        assert!(!app.close_requested);
        assert_eq!(app.error.as_deref(), Some("test backend failure"));
    }
    #[test]
    fn command_palette_exposes_management_without_favorites_deletion() {
        let (_temp, mut app) = fixture();
        let id = app.library.add_category("Night").expect("manual category");
        let actions = app.commands();
        assert!(actions.iter().any(
            |(_, action)| matches!(action,Message::OpenDialog(Dialog::DeleteCategory(i)) if *i==id)
        ));
        assert!(
            !actions
                .iter()
                .any(|(name, _)| name == "Delete category: Favorites")
        );
        app.dialog_text = "show night".into();
        assert!(app.commands().is_empty());
        app.dialog_text = "night".into();
        assert_eq!(app.commands().len(), 3);
    }
    #[test]
    fn source_changes_during_scan_queue_latest_snapshot() {
        let (_temp, mut app) = fixture();
        assert!(app.scanning);
        let _ = app.update(Message::Rescan);
        assert!(app.scan_again);
        let _ = app.update(Message::Scanned(ScanResult::default()));
        assert!(app.scanning);
        assert!(!app.scan_again);
        let _ = app.update(Message::Scanned(ScanResult::default()));
        assert!(!app.scanning);
    }
    #[test]
    fn context_menu_stays_inside_viewport() {
        let offset = context_translation(
            iced::Point::new(790.0, 590.0),
            iced::Rectangle::new(iced::Point::ORIGIN, iced::Size::new(320.0, 300.0)),
            iced::Rectangle::new(iced::Point::ORIGIN, iced::Size::new(800.0, 600.0)),
        );
        assert_eq!(offset, iced::Vector::new(480.0, 300.0));
    }
    #[test]
    fn selection_errors_and_dialogs_preserve_scrollable_tree_state() {
        use iced::advanced::widget::{Tree, tree};
        #[derive(Debug)]
        struct ScrollMarker {
            offset_y: f32,
        }
        fn mark(tree: &mut Tree, tag: tree::Tag) -> usize {
            let mut count = 0;
            if tree.tag == tag {
                tree.state = tree::State::new(ScrollMarker { offset_y: 1200.0 });
                count += 1;
            }
            count
                + tree
                    .children
                    .iter_mut()
                    .map(|child| mark(child, tag))
                    .sum::<usize>()
        }
        fn preserved(tree: &Tree) -> usize {
            let own = match &tree.state {
                tree::State::Some(state) => state
                    .downcast_ref::<ScrollMarker>()
                    .map(|marker| {
                        assert_eq!(marker.offset_y, 1200.0);
                        1
                    })
                    .unwrap_or(0),
                tree::State::None => 0,
            };
            own + tree.children.iter().map(preserved).sum::<usize>()
        }
        let (_temp, mut app) = fixture();
        app.scanning = false;
        app.visible = (0..200)
            .map(|i| PathBuf::from(format!("/fixture/{i}.png")))
            .collect();
        app.scroll_y = 1200.0;
        let probe: Element<'_, Message> = widget::scrollable(Space::new()).into();
        let scroll_tag = probe.as_widget().tag();
        let mut tree = Tree::new(app.view().as_widget());
        // Scrollable's private state is opaque. A sentinel verifies that diff
        // retains the exact state slot which owns the user's scroll offset.
        let marked = mark(&mut tree, scroll_tag);
        assert_eq!(marked, 2, "sidebar and gallery scroll states");
        let _ = app.update(Message::Click(app.visible[40].clone()));
        tree.diff(app.view().as_widget());
        assert_eq!(
            preserved(&tree),
            marked,
            "first selection must retain scroll state"
        );
        app.error = Some("test error".into());
        tree.diff(app.view().as_widget());
        assert_eq!(
            preserved(&tree),
            marked,
            "error banner must retain scroll state"
        );
        let _ = app.update(Message::Context(app.visible[40].clone()));
        tree.diff(app.view().as_widget());
        assert_eq!(
            preserved(&tree),
            marked,
            "opening menu must retain scroll state"
        );
        let _ = app.update(Message::Cancel);
        tree.diff(app.view().as_widget());
        assert_eq!(
            preserved(&tree),
            marked,
            "closing menu must retain scroll state"
        );
        assert_eq!(app.scroll_y, 1200.0);
    }
    #[test]
    fn preview_window_only_requests_viewport_and_overscan() {
        let (_temp, mut app) = fixture();
        app.visible = (0..1300)
            .map(|i| PathBuf::from(format!("/fixture/{i}.png")))
            .collect();
        let initial = app.preview_window();
        assert_eq!(initial.start, 0);
        assert!(
            initial.end < 60,
            "initial viewport must not queue the collection"
        );
        app.scroll_y = app.row_height() * 100.0;
        let scrolled = app.preview_window();
        assert!(scrolled.start > initial.end);
        assert!(scrolled.len() < 60);
        app.visible.truncate(1);
        assert_eq!(
            app.preview_window(),
            0..1,
            "shrinking a category must clamp stale scroll offsets"
        );
    }
}

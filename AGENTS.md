# AGENTS.md — wallpicker

Instructions for AI coding agents working on this repository. Read this and `ROADMAP.md` before doing anything.

## 1. What this project is

`wallpicker` is a Rust wallpaper picker for Hyprland/Wayland, replacing a rofi + bash flow. It scans one or more wallpaper directories, maintains manual categories + Favorites + derived folder categories, and applies wallpapers through `awww` (fallback `swaybg`).

**Current state:** planning complete, implementation not started. Start at M0 in `ROADMAP.md`.

Full product decisions, data model, CLI contract, and milestones live in `ROADMAP.md`. Do not re-litigate confirmed decisions without asking the user.

## 2. Hard rules

1. **Never modify `~/.config/hypr`, `~/.local/bin`, system packages, or any file outside this repository** without the user's explicit permission. Installing the binary and wiring keybinds happens only in M8 and only after approval; always create timestamped backups.
2. **Never delete or move user wallpaper files.** Category operations are metadata-only.
3. **Leave the old script working.** Do not edit or delete `~/.config/hypr/scripts/wallpaper.sh`; it is the fallback.
4. **No destructive store writes.** Back up a corrupt `library.json`; never silently overwrite it.
5. **Ask before adding heavy dependencies** (GTK, libadwaita, webview, databases). Prefer the crate list in the roadmap.
6. **Do not commit build artifacts, caches, logs, or generated images.** `target/`, `.cache/`, `*.log`.
7. **Wiring is documented, not applied.** `README.md` must contain copy-paste Hyprland keybind, autostart, and window-rule snippets. Do not edit `~/.config/hypr` unless the user explicitly asks; default is user-applied wiring.
8. **Do not run the GUI unattended for long periods**; it is a real window in the user's session.

## 3. Commands

Once M0 exists:

```bash
cargo check
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo run -- --help
cargo run -- rescan
cargo run -- random --category favorites
cargo build --release
```

Install (M8, only with permission):

```bash
install -Dm755 target/release/wallpicker ~/.local/bin/wallpicker
```

## 4. Repository layout and ownership

```
src/main.rs        CLI entry, lock, GUI launch
src/cli.rs         clap contract
src/config.rs      config.toml
src/model/         library, scanning, persistence  ← Track A
src/thumbs.rs      thumbnail cache/workers        ← Track B
src/wallpaper.rs   awww/swaybg/state/notify       ← Track B
src/app.rs         iced application               ← Track C
src/message.rs     UI/async message enum          ← Track C
src/theme.rs       palette/theme                  ← Track C
src/ui/            sidebar, gallery, dialogs      ← Track C
tests/             integration tests              ← Track D
README.md          install/CLI/keybind/config docs ← Track D
```

**Shared files:** `Cargo.toml`, `src/main.rs`, `src/message.rs`, `src/app.rs`. Only one agent should edit a shared file at a time. If you need a change in another track's file, write the interface requirement in `ROADMAP.md` or a short `docs/` note and tell the user; don't sweep-edit.

## 5. Interface contracts

Define these early in M0 and keep them stable. Other tracks code against them.

```rust
// model/library.rs
pub struct Library { /* sources, manual categories, favorites, hidden auto cats, last_category */ }
impl Library {
    pub fn load(path: &Path) -> Result<Self>;
    pub fn save(&self, path: &Path) -> Result<()>;

    pub fn add_category(&mut self, name: &str) -> Result<CategoryId>;
    pub fn rename_category(&mut self, id: CategoryId, name: &str) -> Result<()>;
    pub fn delete_category(&mut self, id: CategoryId) -> Result<()>;
    pub fn add_images(&mut self, id: CategoryId, paths: &[PathBuf]) -> Result<()>;
    pub fn remove_images(&mut self, id: CategoryId, paths: &[PathBuf]) -> Result<()>;
    pub fn toggle_favorite(&mut self, path: &Path) -> Result<bool>;
    pub fn set_last_category(&mut self, category: CategoryRef);

    pub fn add_source(&mut self, source: Source) -> Result<()>;
    pub fn remove_source(&mut self, path: &Path) -> Result<RemovedMemberships>;
    pub fn prune_missing(&mut self) -> PruneReport;
}

// model/scan.rs
pub struct ScanResult {
    pub pool: Vec<ImageEntry>,                       // canonical, deduped
    pub auto_categories: BTreeMap<String, Vec<PathBuf>>,
    pub skipped: Vec<ScanIssue>,
}
pub fn scan(sources: &[Source]) -> ScanResult;

// thumbs.rs
pub struct Thumbnailer { /* cache dir, workers */ }
impl Thumbnailer {
    pub fn cached(&self, entry: &ImageEntry) -> Option<PathBuf>;
    pub fn request(&self, entry: &ImageEntry) -> ThumbnailRequest; // background
}

// wallpaper.rs
pub struct ApplyOptions { pub transition: Transition, pub duration: f32, pub notify: bool }
pub fn apply(path: &Path, opts: &ApplyOptions) -> Result<()>;
pub fn restore() -> Result<()>;
pub fn current() -> Option<PathBuf>;
pub fn pick_random(category: &CategoryRef, avoid_repeat: bool) -> Result<PathBuf>;
```

CLI contract is in `ROADMAP.md` §4 and `README.md` — treat both as public API. Any CLI,
config, keybinding, or data-path change must update `README.md` in the same change.

## 6. Data and path rules

- Canonicalize all image paths before storing. Never store relative paths, `~`, or symlink aliases.
- The pool is the union of source scans, deduped by canonical path.
- Manual categories and Favorites store `Vec<PathBuf>`; auto categories are never persisted.
- Auto category = immediate subfolder name under a source, merged across sources, read-only, hideable via `hidden_auto_categories`.
- `last_category` updates on every GUI category switch.
- Store writes: write temp in the same directory, then rename; mode `0600`.
- Keep the legacy state file `~/.local/state/dotfiles/wallpaper/current` authoritative for restore compatibility.
- `awww` namespace is `dotfiles`; do not create a competing namespace.
- Thumbnail cache key: `blake3(path | mtime | size | width | height)`; target 400×225 JPEG q82, EXIF orientation applied.
- Use XDG dirs with injectable paths so tests can use temp dirs. Do not read real user dirs in unit tests.

## 7. UI requirements (Iced 0.14)

- Pin `iced = "=0.14.0"` with features `image`, `lazy`, `advanced`. `image` is not enabled by default.
- Borderless window, iced app id `wallpicker`, size ~85% of focused monitor from `hyprctl monitors -j` (account for `scale`); fallback 1200×800 and clamp to screen.
- Dark theme driven by `config.toml`; default palette matches the current rofi theme (`#152026`, `#203139`, `#69d4cd`).
- Sidebar order: All, Favorites, Manual categories, Folders (auto), Sources.
- Favorites has no rename/delete affordances.
- Single click selects; double click and Enter apply. This behavior is fixed, not configurable.
- Esc closes dialog first, then the app.
- Every destructive action has a confirmation. Deletion of a category/source explains what is and isn't removed.
- Apply success closes the window and sends one toast. Apply failure leaves the window open with a visible error.
- Keyboard shortcuts: arrows/`hjkl`, `Enter`, `Space`, `R`, `/`, `Ctrl+A`, `Ctrl+D`, `F2`, `Esc`.
- Never block the UI thread on scanning, thumbnailing, or `awww`. Use iced `Task`/subscriptions.

## 8. Testing and quality gates

- Unit tests live next to modules; integration tests in `tests/`.
- Model tests must cover: Favorites protection, duplicate/empty names, many-to-many membership, auto-category merge/hide/depth, source-removal prune, missing-file handling, store round-trip/recovery.
- Thumbnail tests generate tiny images in temp dirs; assert cache hit/miss/invalidation and unreadable-file behavior.
- CLI tests cover argument parsing and random fallback logic.
- Before handing off: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test` all pass.
- Do not use real wallpapers in tests; generate 2–3 px PNGs.
- No `unwrap`/`expect` on user-controlled I/O in library code; surface errors through `Result` and the UI/log.
- `unsafe` is allowed only for `setsid`/process detach if necessary; document it.

## 9. Multi-agent coordination

- **Claim a milestone** by adding your agent/session name next to it in `ROADMAP.md` (e.g., `M1 — @agent-a`).
- **Update the checkbox** as tasks complete, and append to the progress log with date + module.
- **Keep changes scoped** to your track. Do not reformat unrelated files.
- **Interface changes** require a note in `ROADMAP.md` §5/§6 and a heads-up to the user before touching other tracks.
- **Do not run simultaneous `cargo` commands** in the same `target/` directory if it causes lock contention; wait or use `CARGO_TARGET_DIR=target/<track>` for parallel exploration.
- **Integration order:** A (core) → B (media) / C (UI) → D (QA). C may start against fixtures once A's types compile.
- On disagreement with a confirmed decision, stop and ask the user rather than changing behavior unilaterally.

## 10. Definition of done

A task is done only when:

- [ ] Behavior matches the roadmap/decisions.
- [ ] Tests cover the new behavior (or a written reason they can't).
- [ ] `fmt`, `clippy -D warnings`, and `test` pass.
- [ ] `README.md` is updated for any CLI, config, keybinding, or data-path change.
- [ ] No user files outside the repo were modified.
- [ ] `ROADMAP.md` checkboxes/progress log are updated.
- [ ] The change is usable from the CLI or UI without manual database edits.

## 11. Useful environment facts

- Host: CachyOS, Hyprland 0.56.2, Wayland.
- `awww`, `awww-daemon`, `magick`, `notify-send`, `zenity`, `kdialog`, `swaybg` are installed.
- Existing collection: `~/Pictures/wallpaper` (~1,300 images, top-level folders `catppuccin-wallpapers`, `walls-catppuccin-mocha`, `wallpapers`, `gruvbox-wallpapers`).
- Existing launcher: `$mainMod, W` → old bash script; autostart calls `bash .../wallpaper.sh --restore`.
- No git repository yet (pending user decision).

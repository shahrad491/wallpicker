# wallpicker — Roadmap

A Rust wallpaper picker for Hyprland, replacing the current rofi + `wallpaper.sh` flow.

**Status:** M0–M7 implementation complete; M8 automated QA and release build complete. Live desktop acceptance and user-applied installation/wiring remain pending.
**Target:** Linux / Wayland / Hyprland on CachyOS. Rust stable.
**Binary:** `wallpicker`
**Primary config:** `~/.config/wallpicker/config.toml`
**Library data:** `~/.local/share/wallpicker/library.json`
**Thumbnail cache:** `~/.cache/wallpicker/thumbs/`
**Log:** `~/.cache/wallpicker/wallpicker.log`

---

## 1. Product summary

A fast, keyboard-friendly image picker with a real category system instead of a flat rofi list.

- Browse all wallpapers from one or more source directories.
- Manual categories (many-to-many) with create / rename / delete.
- Built-in **Favorites** category that cannot be deleted or renamed.
- Automatic folder categories derived from each source's immediate subfolders.
- Star an image, tag it into multiple categories, remove it from a category.
- Apply a wallpaper with `awww` transitions; `swaybg` fallback.
- Random wallpaper from the last category used in the GUI, via a global Hyprland keybind.
- Downloads/scans are not managed; the app only reads directories.

---

## 2. Confirmed decisions

| Area | Decision |
|---|---|
| UI toolkit | **Iced 0.14** (`image`, `lazy`, `advanced` features; `image` is not on by default) |
| Sources vs categories | **Hybrid:** sources feed an All pool; manual categories are tags; each source can auto-generate folder categories |
| Category membership | **Many-to-many** (paths can be in several manual categories) |
| Auto categories | Immediate subfolders only; read-only; same names merge across sources; can be hidden |
| Source removal | Confirmation, then prune that source's paths from All, auto categories, manual categories, and Favorites. Files untouched |
| Favorites | Pinned first; cannot be renamed or deleted; star toggle |
| Random shortcut | Draws from the **last category selected in the GUI**; CLI can override |
| After selection | Double-click/Enter/toolbar/Random: apply, close, toast. More/right-click Apply: apply, keep browsing, toast (user update 2026-10-28) |
| Click behavior | Single click selects; double click applies; `Enter` applies |
| Hyprland wiring | Documented in `README.md` with copy-paste keybind/autostart snippets; dotfiles are not edited automatically |
| Window | Borderless, centered, ~85% of focused monitor, dark catppuccin-ish theme |
| Backend | `awww` namespace `dotfiles`; `swaybg` fallback |
| State compatibility | Keep writing `~/.local/state/dotfiles/wallpaper/current` for login restore and old tooling |
| Old script | Leave `~/.config/hypr/scripts/wallpaper.sh` untouched as fallback |
| Storage | JSON + atomic writes, no database |
| Directory picker | `zenity --file-selection --directory` when available, editable path field fallback |

### Open questions (blocking only at the end)

- [x] Existing git repository detected and retained; no initialization or commits performed.
- [ ] Whether the README snippets are wired in by the user or the agent (default: user wires them; agent only installs the binary when asked)

---

## 3. Architecture

```
src/
  main.rs          CLI dispatch, single-instance lock, GUI launch
  cli.rs           clap definitions
  app.rs           iced application state, update, view
  message.rs       UI + async messages
  config.rs        config.toml load/save/defaults
  model/
    library.rs     pool, manual categories, favorites, last category, operations
    scan.rs        source walking, auto-category derivation, dedupe
    store.rs       atomic JSON load/save, versioning, corruption recovery
  thumbs.rs        hashed disk cache + bounded worker pool
  wallpaper.rs     awww/swaybg apply, daemon start, state file, notify
  theme.rs         iced theme + palette
  ui/
    sidebar.rs     All / Favorites / Manual / Folders / Sources
    gallery.rs     image grid, selection, hover actions, context menu
    dialogs.rs     category + source dialogs, confirmations, errors
```

### Crates

- `iced` 0.14 — features `image`, `lazy`, `advanced`
- `image` 0.25, `fast_image_resize` — thumbnail generation
- `walkdir`, `rayon` — scanning and resizing
- `serde`, `serde_json`, `toml` — persistence
- `blake3` — cache keys
- `rand` — random selection
- `clap` — CLI
- `anyhow`, `thiserror` — errors
- `libc` — detached `awww-daemon` spawn (`setsid`)
- logging crate writing to `~/.cache/wallpicker/wallpicker.log`

Avoid GTK/rfd dependencies. Use `zenity` if present.

### Data model

`library.json`:

```json
{
  "version": 1,
  "sources": [
    { "path": "/home/user/Pictures/wallpaper", "recursive": true, "auto_categories": true }
  ],
  "categories": [
    { "id": 12, "name": "Dark", "images": ["/abs/path/a.jpg"] }
  ],
  "favorites": ["/abs/path/b.png"],
  "hidden_auto_categories": ["gruvbox-wallpapers"],
  "last_category": { "kind": "auto", "name": "catppuccin-wallpapers" }
}
```

`config.toml`:

```toml
[behavior]
close_on_select = true
notify = true
avoid_repeat = true
transition = "random"      # random | left | right | top | bottom | center | outer | fade | none
duration = 1.2

[thumbs]
width = 400
height = 225
quality = 82
workers = 4

[window]
width_percent = 85
height_percent = 85

[theme]
accent = "#69d4cd"
background = "#152026"
surface = "#203139"
foreground = "#e4edef"
muted = "#9bafb6"
```

### Invariants

1. All stored image paths are **canonical, absolute** paths; the pool is deduped.
2. Manual categories and Favorites store paths only — category operations never move/delete files.
3. Favorites cannot be deleted or renamed. Manual category names are unique case-insensitively, trimmed, non-empty.
4. Auto categories are always derived at scan time and never persisted as categories.
5. `last_category` is updated whenever the selected category changes in the GUI.
6. Library writes are atomic (temp file in same directory + rename), mode `0600`.
7. The UI never decodes full-size images — only cached thumbnails.
8. Wallpaper state is drop-in compatible with the legacy bash script.
9. No destructive filesystem operation without an explicit confirmation dialog.

---

## 4. CLI contract

```
wallpicker                       Open the picker
wallpicker pick                  Open the picker (explicit)
wallpicker random                Random from last_category; avoid repeating current
wallpicker random --category favorites|all|"Dark"
wallpicker random --no-rescan    Use stored lists only where possible
wallpicker set <path>            Apply one image
wallpicker restore               Quietly re-apply last wallpaper (login)
wallpicker rescan                Scan sources and exit with a summary
wallpicker --version
wallpicker --help
```

Behavior:

- GUI is single-instance. If already running, focus via `hyprctl dispatch focuswindow` and exit.
- `random` from Favorites/manual category uses stored paths (verify existence); `all`/auto category triggers a scan.
- Empty category falls back to All; empty pool exits non-zero with a notification.
- `apply` writes the legacy state file and sends a toast unless `--quiet` / `notify = false`.

---

## 5. Milestones

### M0 — Bootstrap and shared contracts — @codex-core

- [x] Initialize the `wallpicker` Cargo package
- [x] Add dependencies and feature flags
- [x] Create module skeletons with doc comments and explicit public APIs
- [x] Path helpers + XDG-aware directories (injectable for tests)
- [x] Logging setup and panic hook
- [x] CI-style local checks documented: `fmt`, `clippy`, `test`
- [x] Existing git repository retained; `.gitignore` added (`/target`, `/.cache`, `*.log`); no initialization or commits

**Implementation:** complete; real-desktop acceptance is tracked under M8.

**Done when:** `cargo check` passes, `wallpicker --version` works, module boundaries are agreed.

---

### M1 — Config, store, and error model — @codex-core

Depends on M0.

- [x] `config.rs`: defaults, load, save, invalid-value fallback, validation
- [x] `store.rs`: load, save, atomic write, `version` field, corrupt-file backup
- [x] Operations: `add_category`, `rename_category`, `delete_category`, `add_to_category`, `remove_from_category`, `toggle_favorite`, `set_last_category`
- [x] Favorites protection and duplicate-name rules
- [x] Prune operations by source / missing files
- [x] Unit tests for every operation and for recovery

**Implementation:** complete; real-desktop acceptance is tracked under M8.

**Done when:** library operations are exhaustively tested; corrupt JSON never silently overwrites user data.

---

### M2 — Scanner and derived categories — @codex-core

Depends on M1.

- [x] Walk sources (`walkdir`, optional recursion, no symlink loops)
- [x] Filter image extensions: png, jpg, jpeg, webp, gif, bmp, tif, tiff
- [x] Canonicalize, dedupe, collect `{ path, mtime, size, parent, source }`
- [x] Build auto categories from immediate subfolders, merging same names
- [x] Skip empty folders; apply hidden list
- [x] Detect duplicate/nested sources when adding
- [x] Scan result summary (count, skipped, duration)
- [x] Tests with generated temp trees

**Implementation:** complete; real-desktop acceptance is tracked under M8.

**Done when:** scanning `~/Pictures/wallpaper` (~1,300 files, 4 top folders) produces the expected pool and folder categories.

---

### M3 — Thumbnail service — @codex-media

Depends on M0/M2 types; can run in parallel with M4.

- [x] Cache key: `blake3(canonical path | mtime | size | target dims)`
- [x] Decode, EXIF-orient, cover-crop, resize (400×225), JPEG encode
- [x] Bounded worker pool (`workers`, default 4), dedupe requests
- [x] Progressive delivery to UI; placeholder while pending
- [x] Failure cache that invalidates on mtime/size change
- [x] Cleanup of orphaned cache files (manual action)
- [x] Tests: hit, miss, invalidation, unreadable file

**Implementation:** complete; real-desktop acceptance is tracked under M8.

**Done when:** second launch after full generation renders the whole collection without regenerating thumbnails.

---

### M4 — Wallpaper backend and headless CLI — @codex-media

Depends on M0/M1; parallel with M2/M3.

- [x] Reuse `awww` namespace `dotfiles`; detect daemon via `awww query --namespace dotfiles`
- [x] Start `awww-daemon` detached (`setsid`, logs to cache) when missing
- [x] Apply with configured transition/duration/resize, matching current script
- [x] `swaybg` fallback with PID/start-time tracking
- [x] Atomic legacy state write
- [x] `notify-send` success/critical failure notifications
- [x] `restore`, `set`, `random --category`, `rescan`
- [x] CLI tests where practical (argument parsing, selection logic)

**Implementation:** complete; real-desktop acceptance is tracked under M8.

**Done when:** `wallpicker set`, `restore`, and `random` work without opening a window; the old script and new binary can alternate safely.

---

### M5 — Iced shell and visual design — @codex-ui

Depends on M1/M2; UI can start against fixture data.

- [x] Iced 0.14 application skeleton, window settings, app id `wallpicker`
- [x] Borderless window sized from `hyprctl monitors -j` (focused monitor, scale-aware), fallback 1200×800
- [x] Custom theme from config palette; rofi-like dark look
- [x] Sidebar: All, Favorites, Categories, Folders, Sources + counts
- [x] Toolbar: search, sort, Random, Rescan
- [x] Grid: fluid columns, 16:9 tile, hover star, current-wallpaper badge
- [x] Empty/loading/error states
- [x] Single-instance lock + focus existing window

**Implementation:** complete; real-desktop acceptance is tracked under M8.

**Done when:** app opens with real data, is keyboard-dismissable, and looks intentional at both 1080p scales (1.0 and 1.5).

---

### M6 — Gallery interactions — @codex-ui

Depends on M5.

- [x] Click select, Ctrl-click toggle, Shift-click range, double-click / Enter apply (single click only selects)
- [x] Hover actions: favorite, add to category, more
- [x] Right-click context menu (iced `float`): apply, favorite, add to category submenu, remove from current category, reveal, copy path
- [x] Batch toolbar for selections: favorite, add, remove, clear
- [x] Keyboard: arrows/hjkl, Enter, Space, R, `/`, Ctrl+A, Ctrl+D, Esc
- [x] Search filter + Name/Newest/Oldest sorting
- [x] In-app random from current category, avoids repeat when possible
- [x] Ordinary apply → close + toast; context Apply → keep browsing + toast; failure → visible error
- [x] Last category persisted on sidebar change

**Implementation:** complete; real-desktop acceptance is tracked under M8.

**Done when:** the picker can be driven entirely by keyboard or entirely by mouse for every core action.

---

### M7 — Category and source management — @codex-ui

Depends on M1/M5/M6.

- [x] New category dialog + validation
- [x] Rename (F2 / context menu)
- [x] Delete with count confirmation; files untouched
- [x] Multi-select add/remove to manual category
- [x] Favorites cannot be renamed/deleted (UI affordance hidden/disabled)
- [x] Auto categories: hide/unhide, explain regeneration
- [x] Source manager: add (path + Browse… via zenity), recursive toggle, auto-category toggle, remove
- [x] Remove source confirmation explaining membership pruning
- [x] "Clean up missing entries" action
- [x] Rescan feedback and progress

**Implementation:** complete; real-desktop acceptance is tracked under M8.

**Done when:** every CRUD path in the decision table is reachable and has a confirmation or undo-safe behavior.

---

### M8 — QA, performance, packaging — @codex-core

- [ ] Run against the full 1,300-image collection; measure open time and scroll smoothness
- [x] Viewport virtualization implemented; offscreen rows use spacers (live smoothness still pending)
- [ ] Test scale factor 1.5 (`eDP-1`) and multi-monitor focus
- [ ] Test awww transitions and swaybg fallback (temporarily hide awww)
- [ ] Test restore at login, single instance, corrupt library, missing images
- [x] `cargo fmt --check`, `cargo clippy --offline --all-targets -- -D warnings`, `cargo test --offline` (54 tests)
- [x] Release build: `target/release/wallpicker` (about 28.7 MB); help/version/rescan smoke passed
- [ ] Install to `~/.local/bin/wallpicker` (only after explicit permission)
- [x] Optional desktop entry provided in `assets/wallpicker.desktop` (not installed)
- [x] `README.md`: install/build, every CLI command, in-app keyboard shortcuts, config reference, data locations, and copy-paste Hyprland keybind/autostart/window-rule snippets
- [ ] Verify the README snippets work in a real Hyprland config before declaring M8 done
- [ ] Hyprland wiring is **user-applied from the README** by default; only edit dotfiles with explicit permission, with backups
  - replace `$mainMod, W` rofi/script bind with `wallpicker`
  - add `$mainMod SHIFT, W` → `wallpicker random`
  - optionally add favorites random bind
  - switch autostart restore line to `wallpicker restore`
  - add float/center/rounding window rules for class `wallpicker`

**Done when:** you can use wallpicker daily and the old script is a fallback, not the default.

---

### M9 — Color, startup, preview apply, and distribution — @codex-core / @codex-ui / @codex-media / @codex-qa

- [x] Diagnose washed-out previews against source/cached pixels; retain original cache colors
- [x] Use native sRGB rendering; disable Iced web-colors float-surface path
- [x] Default to software rendering to bypass cold GPU initialization; preserve explicit hardware overrides, remove portal theme query and verbose adapter enumeration
- [x] Record startup checkpoints and first redraw for real-session timing
- [x] More/right-click Apply keeps browsing; ordinary apply closes; successful previews refresh Current badge
- [x] Add Makefile, native release archive/checksum, staging and installation scripts
- [x] GitHub README and user/developer documentation under `docs/`
- [x] 61 Rust tests, format, all-target Clippy, packaging fixture and staged-install backup checks
- [x] Final native release archive validation: checksum, safe extraction, executable mode, docs, version/help/rescan
- [x] User confirmed corrected preview colors
- [ ] Repeat attended software-first cold-start/keep-open test in user's compositor session

**User decision update (2026-10-28):** More/right-click Apply deliberately keeps the
window open for testing wallpapers. This supersedes the original close-always rule for
that action; normal double-click/Enter/toolbar/Random keep their closing behavior.

**Renderer interface note:** no new CLI/config keys; GUI-only graphics defaults honor
explicit environment overrides. Existing cache JPEGs remain valid; original wallpaper
files are never modified. Packaging only creates artifacts under `target/`.

---

### M10 — Live media, rotation, and Linux installation — @codex-core / @codex-media / @codex-ui / @codex-qa

- [x] Accept user's software renderer check; keep first-cold-start measurement pending
- [x] Media contracts: image/video/project.json classification; canonical project manifests keep schema v1 paths
- [x] Scanner background title/preview metadata; hide project internals as one gallery item
- [x] Config contracts: [live] output/FPS/mute/assets, [rotation] interval/category/include_live/notify
- [ ] mpvpaper videos and external linux-wallpaperengine projects with owned-process cleanup/restore
- [ ] Background video/project previews and cache invalidation
- [ ] Sleeping rotation CLI, configurable interval, category fallback, static-only default
- [ ] GUI playback/rotation settings and explicit service controls
- [x] User installer, absolute desktop launcher, icon and inactive systemd user unit
- [x] Tar/standalone binary and .deb packaging; optional AppImage tooling; documented Flatpak constraints
- [ ] Combined tests/fmt/Clippy, release, tar/.deb extraction and service/package syntax checks
- [ ] Attended live backend/service check (optional playback tools not installed in this environment)

**User scope update:** support both looping local videos and Wallpaper Engine projects.
Video/project playback delegates to optional `mpvpaper`/`linux-wallpaperengine`; the app
never installs them or claims complete upstream project compatibility. Existing image
behavior and software-renderer startup remain. Automatic rotation waits the configured
interval before the first change; enabling a service is a separate user action.

**Shared API note:** `ImageEntry` now adds optional scanned `title` and `preview`
(`PreviewMetadata {path, mtime, size}`), preventing project JSON reads in UI thumbnail
lookups. This data is derived, not persisted. CLI adds `rotate` and `live stop`; README
and docs describe all new settings/paths/installation controls in the same change.

---

### Implementation and validation notes (2026-10-26)

- Core, media, UI, and QA followed the track ownership below. M0–M7 checkboxes record
  implemented behavior; their real-session acceptance criteria remain part of M8.
- `Backend` and `Thumbnailer` retain the documented wrappers/contracts and add injectable
  constructors so tests never invoke real desktop tools or write to user XDG directories.
- Ctrl+P opens a searchable keyboard command palette for all management/sidebar actions.
  Search Enter returns to gallery selection. Context menus use Iced float overlays.
- Stored manual tags are preserved when recursion is disabled and become visible in the
  gallery again when their source is scanned recursively. New membership checks still
  require an image in an enabled source scope.
- `--no-rescan` reuses stored manual/Favorites lists. All/folders and empty-list fallback
  still scan because the derived pool is intentionally not persisted.
- Real collection scan through the CLI: **1,294 images, 4 folders, 0 skipped, 55 ms**
  (debug build, isolated XDG storage). This does not measure thumbnail or window open time.
- Bounded GUI smoke attempt could not create a Wayland connection in the sandbox
  (`NoCompositor`); `hyprctl` also could not connect. No window remained running and no
  real wallpaper application, autostart, or Hyprland configuration change was performed.
- User confirmed the real window opens and wallpaper application succeeds. Their first
  attended test found selection resetting scroll and slow/missing previews; fixes keep
  widget state stable and prioritize visible-row previews. M8 still requires a repeat
  attended check of layout/scrolling/scale, real backend transitions
  and login restore, second-instance focus, and README wiring in the user's session.

## 6. Parallel work plan

Use separate agents/windows per track. Shared files must be coordinated.

| Track | Scope | Depends on | Owns |
|---|---|---|---|
| A — Core | M0, M1, M2 | — | `Cargo.toml` (shared), `main.rs`, `cli.rs`, `config.rs`, `model/**` |
| B — Media | M3, M4 | M0 types | `thumbs.rs`, `wallpaper.rs` |
| C — UI | M5, M6, M7 | M1/M2 APIs | `app.rs`, `message.rs`, `theme.rs`, `ui/**` |
| D — Quality | tests, docs, M8 | all | `tests/**`, `README.md`, CI notes |

Rules:

- Track A defines the model/API contracts first; B and C code against them with fixture data.
- Only one agent edits `Cargo.toml` at a time. Announce dependency additions.
- C should not block on thumbnails/backend: use placeholder paths and a mock apply function.
- D joins after A/B/C have compilable units; avoid broad refactors during integration.

---

## 7. Verification checklist

- [x] Favorites cannot be deleted or renamed, by UI or model API
- [x] Deleting a category never deletes an image
- [x] Removing a source prompts and prunes all memberships
- [x] Auto categories merge, hide, and regenerate correctly
- [x] Manual category with a missing file hides it without losing the entry
- [x] Random uses the last GUI category; fallbacks are sane
- [ ] Apply writes legacy state and shows a toast
- [ ] Restore at login works headlessly
- [x] Thumbnail cache survives restart and invalidates on file change
- [ ] 1,300 images open and scroll acceptably
- [ ] 1.5 scale factor window fits the screen
- [ ] No second instance opens; existing window focuses
- [x] Corrupt library does not destroy data
- [ ] Old `wallpaper.sh` still works during transition

---

## 8. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Iced 0.14 is new; API churn | Pin `=0.14.0`; verify docs before each module; keep UI logic in plain state |
| 1,300-tile grid performance | `lazy` feature, chunked rendering, measure with real data |
| Wayland window class/position | Set app id, verify with `hyprctl clients`; use Hyprland window rules |
| `awww` daemon lifecycle | Reuse namespace, spawn detached, query before apply, graceful swaybg fallback |
| Auto categories surprising users | Read-only, grouped separately, hideable, documented |
| Multi-agent merge conflicts | File ownership table, interface-first contracts, small PR-sized tasks |
| Accidental dotfile damage | Never edit `~/.config/hypr` without explicit approval; always back up |

---

## 9. Progress log

Append one line per completed milestone/task group.

| Date | Milestone | Note |
|---|---|---|
| 2026-10-26 | Planning | Roadmap + AGENTS.md created |
| 2026-10-26 | Docs | README.md added: CLI, in-app controls, config, data paths, Hyprland wiring snippets; click behavior fixed (single selects, double applies) |
| 2026-10-26 | M0–M2 / core | Cargo package, stable contracts, XDG paths/logging, validated config, private atomic library/recovery, canonical scanning and metadata operations implemented; real collection scan 1,294 images / 4 folders / 55 ms. |
| 2026-10-26 | M3–M4 / media | Bounded thumbnail workers with EXIF/cache/failure tests; injectable awww/swaybg backend, legacy state/PID handling, quiet restore, random fallback, notifications. |
| 2026-10-26 | M5–M7 / UI | Iced shell, virtualized gallery, keyboard command palette, floating context menu, CRUD/confirmations, serialized saves and queued scans, visible errors and close-after-save. |
| 2026-10-26 | M8 / automated QA | 50 tests pass (generated images, injected tools, isolated XDG); fmt and all-target Clippy with warnings denied pass. CLI help/version/rescan smoke passed; bounded GUI smoke blocked by sandbox NoCompositor. Desktop acceptance requested from user. |
| 2026-10-26 | M8 / release | Optimized binary built (28,675,496 bytes); isolated version/rescan smoke passed: 1,294 images, 4 folders, 0 skipped, 54 ms. Installation, compositor acceptance, and wiring intentionally pending. |
| 2026-10-26 | M8 / user desktop test | User confirmed picker opens and applies wallpaper; reported click resetting scroll and most previews missing. Diagnosed Iced child-index state replacement and slow unoptimized full-collection thumbnail queue. Fixes: stable widget tree, viewport priority/cancellation, faster cover-crop area downsampling, optimized dev dependencies, readiness feedback; 54 tests, fmt, and all-target Clippy with warnings denied pass. |
| 2026-10-26 | M8 / gallery fixes validated | 54 tests pass, including widget-tree preservation through selection/errors/dialogs, viewport bounds after category changes, crop correctness, and stale thumbnail cancellation. Fmt and all-target Clippy pass. Optimized-dependency sample: 1080p PNG thumbnail pipeline approximately 12.5–20.4 ms; existing cache reused. Repeat attended test requested via updated binary. |
| 2026-10-28 | M9 / rendering and startup | Compared source and cached RGB values (approximately 1–2/255 differences); cache is not tinted. Disabled web-colors for native sRGB surfaces and removed theme-portal detection. GUI defaults prefer Vulkan/low-power GPU without overriding explicit environment choices; default log level skips INFO-only adapter enumeration. Startup checkpoints/first redraw retained for live timing. |
| 2026-10-28 | M9 / interaction | User-requested More/right-click Apply now keeps browsing and refreshes Current; ordinary apply still closes. Async completions carry path/intent, failures preserve current wallpaper, concurrent requests guarded. Three new flow regressions. |
| 2026-10-28 | M9 / distribution and docs | Added locked Make targets, native tar.gz+SHA-256 packaging, explicit installer with timestamped binary/launcher backups and DESTDIR support, root docs guides/troubleshooting and GitHub README. Packaging fixture and repeated staged installs verified inside target/. |
| 2026-10-28 | M9 / automated QA | 61 Rust tests, fmt, all-target Clippy with warnings denied, shell syntax, package fixture/checksum, staged backup collision checks, and documentation links pass. Final release archive built and validated; actual first-cold-frame and native color appearance require user Wayland recheck. |
| 2026-10-28 | M9 / package validated | make package completed; native Linux x86_64 archive and SHA-256 under target/dist/. Extracted payload passed version/help/isolated-rescan smoke; archive paths, executable permissions and docs verified. No user installation, Hyprland edit, or original wallpaper change performed. Live Wayland retest remains pending. |
| 2026-10-28 | M9 / startup follow-up | User confirmed corrected colors, but Vulkan/iGPU first-open still about5s. Font initialization profiled23–28ms, first shaping under1ms. Software renderer clipped stress scene (40previews/70labels,1088×612 at1.5 scale): firstframe48–50ms, warm median15.5–16.4ms; no pathological scroll-mask lag observed. Switched default to tiny-skia, keeping GPU opt-in and explicit overrides. Live software-first launch still needs recheck. |
| 2026-10-28 | M9 / software-first release | 61 tests, fmt and all-target Clippy pass after renderer policy update. Both software and GPU headless screenshots preserve gray128, gray64 and accent RGB exactly. Rebuilt final native archive and verified checksum, extraction and executable version. Software-first live startup/scrolling retest requested; old GPU5s report preserved above. |
| 2026-10-28 | M10 / claimed | User accepts current startup pending cold check and requests easy Linux installation, both local video/Wallpaper Engine playback, configurable30m rotation, and distribution choices. Claimed core/media/UI/QA tracks; no automatic dependency installation or service activation. |

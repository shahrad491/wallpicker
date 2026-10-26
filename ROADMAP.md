# wallpicker — Roadmap

A Rust wallpaper picker for Hyprland, replacing the current rofi + `wallpaper.sh` flow.

**Status:** Planning complete. Implementation not started.
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
| After selection | Apply, close, show `notify-send` toast |
| Click behavior | Single click selects; double click applies; `Enter` applies |
| Hyprland wiring | Documented in `README.md` with copy-paste keybind/autostart snippets; dotfiles are not edited automatically |
| Window | Borderless, centered, ~85% of focused monitor, dark catppuccin-ish theme |
| Backend | `awww` namespace `dotfiles`; `swaybg` fallback |
| State compatibility | Keep writing `~/.local/state/dotfiles/wallpaper/current` for login restore and old tooling |
| Old script | Leave `~/.config/hypr/scripts/wallpaper.sh` untouched as fallback |
| Storage | JSON + atomic writes, no database |
| Directory picker | `zenity --file-selection --directory` when available, editable path field fallback |

### Open questions (blocking only at the end)

- [ ] Initialize a git repository? (recommended before multi-agent work)
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

### M0 — Bootstrap and shared contracts

- [ ] `cargo init --name wallpicker`
- [ ] Add dependencies and feature flags
- [ ] Create module skeletons with doc comments and explicit public APIs
- [ ] Path helpers + XDG-aware directories (injectable for tests)
- [ ] Logging setup and panic hook
- [ ] CI-style local checks documented: `fmt`, `clippy`, `test`
- [ ] Optional: `git init`, `.gitignore` (`/target`, `*.log`)

**Done when:** `cargo check` passes, `wallpicker --version` works, module boundaries are agreed.

---

### M1 — Config, store, and error model

Depends on M0.

- [ ] `config.rs`: defaults, load, save, invalid-value fallback, validation
- [ ] `store.rs`: load, save, atomic write, `version` field, corrupt-file backup
- [ ] Operations: `add_category`, `rename_category`, `delete_category`, `add_to_category`, `remove_from_category`, `toggle_favorite`, `set_last_category`
- [ ] Favorites protection and duplicate-name rules
- [ ] Prune operations by source / missing files
- [ ] Unit tests for every operation and for recovery

**Done when:** library operations are exhaustively tested; corrupt JSON never silently overwrites user data.

---

### M2 — Scanner and derived categories

Depends on M1.

- [ ] Walk sources (`walkdir`, optional recursion, no symlink loops)
- [ ] Filter image extensions: png, jpg, jpeg, webp, gif, bmp, tif, tiff
- [ ] Canonicalize, dedupe, collect `{ path, mtime, size, parent, source }`
- [ ] Build auto categories from immediate subfolders, merging same names
- [ ] Skip empty folders; apply hidden list
- [ ] Detect duplicate/nested sources when adding
- [ ] Scan result summary (count, skipped, duration)
- [ ] Tests with generated temp trees

**Done when:** scanning `~/Pictures/wallpaper` (~1,300 files, 4 top folders) produces the expected pool and folder categories.

---

### M3 — Thumbnail service

Depends on M0/M2 types; can run in parallel with M4.

- [ ] Cache key: `blake3(canonical path | mtime | size | target dims)`
- [ ] Decode, EXIF-orient, cover-crop, resize (400×225), JPEG encode
- [ ] Bounded worker pool (`workers`, default 4), dedupe requests
- [ ] Progressive delivery to UI; placeholder while pending
- [ ] Failure cache that invalidates on mtime/size change
- [ ] Cleanup of orphaned cache files (manual action)
- [ ] Tests: hit, miss, invalidation, unreadable file

**Done when:** second launch after full generation renders the whole collection without regenerating thumbnails.

---

### M4 — Wallpaper backend and headless CLI

Depends on M0/M1; parallel with M2/M3.

- [ ] Reuse `awww` namespace `dotfiles`; detect daemon via `awww query --namespace dotfiles`
- [ ] Start `awww-daemon` detached (`setsid`, logs to cache) when missing
- [ ] Apply with configured transition/duration/resize, matching current script
- [ ] `swaybg` fallback with PID/start-time tracking
- [ ] Atomic legacy state write
- [ ] `notify-send` success/critical failure notifications
- [ ] `restore`, `set`, `random --category`, `rescan`
- [ ] CLI tests where practical (argument parsing, selection logic)

**Done when:** `wallpicker set`, `restore`, and `random` work without opening a window; the old script and new binary can alternate safely.

---

### M5 — Iced shell and visual design

Depends on M1/M2; UI can start against fixture data.

- [ ] Iced 0.14 application skeleton, window settings, app id `wallpicker`
- [ ] Borderless window sized from `hyprctl monitors -j` (focused monitor, scale-aware), fallback 1200×800
- [ ] Custom theme from config palette; rofi-like dark look
- [ ] Sidebar: All, Favorites, Categories, Folders, Sources + counts
- [ ] Toolbar: search, sort, Random, Rescan
- [ ] Grid: fluid columns, 16:9 tile, hover star, current-wallpaper badge
- [ ] Empty/loading/error states
- [ ] Single-instance lock + focus existing window

**Done when:** app opens with real data, is keyboard-dismissable, and looks intentional at both 1080p scales (1.0 and 1.5).

---

### M6 — Gallery interactions

Depends on M5.

- [ ] Click select, Ctrl-click toggle, Shift-click range, double-click / Enter apply (single click only selects)
- [ ] Hover actions: favorite, add to category, more
- [ ] Right-click context menu (iced `float`): apply, favorite, add to category submenu, remove from current category, reveal, copy path
- [ ] Batch toolbar for selections: favorite, add, remove, clear
- [ ] Keyboard: arrows/hjkl, Enter, Space, R, `/`, Ctrl+A, Ctrl+D, Esc
- [ ] Search filter + Name/Newest/Oldest sorting
- [ ] In-app random from current category, avoids repeat when possible
- [ ] Apply → close + toast; failure → stay open with error banner
- [ ] Last category persisted on sidebar change

**Done when:** the picker can be driven entirely by keyboard or entirely by mouse for every core action.

---

### M7 — Category and source management

Depends on M1/M5/M6.

- [ ] New category dialog + validation
- [ ] Rename (F2 / context menu)
- [ ] Delete with count confirmation; files untouched
- [ ] Multi-select add/remove to manual category
- [ ] Favorites cannot be renamed/deleted (UI affordance hidden/disabled)
- [ ] Auto categories: hide/unhide, explain regeneration
- [ ] Source manager: add (path + Browse… via zenity), recursive toggle, auto-category toggle, remove
- [ ] Remove source confirmation explaining membership pruning
- [ ] "Clean up missing entries" action
- [ ] Rescan feedback and progress

**Done when:** every CRUD path in the decision table is reachable and has a confirmation or undo-safe behavior.

---

### M8 — QA, performance, packaging

- [ ] Run against the full 1,300-image collection; measure open time and scroll smoothness
- [ ] If grid is slow: enable `lazy` tiles and/or chunked rendering
- [ ] Test scale factor 1.5 (`eDP-1`) and multi-monitor focus
- [ ] Test awww transitions and swaybg fallback (temporarily hide awww)
- [ ] Test restore at login, single instance, corrupt library, missing images
- [ ] `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`
- [ ] Release build, install to `~/.local/bin/wallpicker`
- [ ] Optional desktop entry with `StartupWMClass=wallpicker`
- [ ] `README.md`: install/build, every CLI command, in-app keyboard shortcuts, config reference, data locations, and copy-paste Hyprland keybind/autostart/window-rule snippets
- [ ] Verify the README snippets work in a real Hyprland config before declaring M8 done
- [ ] Hyprland wiring is **user-applied from the README** by default; only edit dotfiles with explicit permission, with backups
  - replace `$mainMod, W` rofi/script bind with `wallpicker`
  - add `$mainMod SHIFT, W` → `wallpicker random`
  - optionally add favorites random bind
  - switch autostart restore line to `wallpicker restore`
  - add float/center/rounding window rules for class `wallpicker`

**Done when:** you can use wallpicker daily and the old script is a fallback, not the default.

---

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

- [ ] Favorites cannot be deleted or renamed, by UI or model API
- [ ] Deleting a category never deletes an image
- [ ] Removing a source prompts and prunes all memberships
- [ ] Auto categories merge, hide, and regenerate correctly
- [ ] Manual category with a missing file hides it without losing the entry
- [ ] Random uses the last GUI category; fallbacks are sane
- [ ] Apply writes legacy state and shows a toast
- [ ] Restore at login works headlessly
- [ ] Thumbnail cache survives restart and invalidates on file change
- [ ] 1,300 images open and scroll acceptably
- [ ] 1.5 scale factor window fits the screen
- [ ] No second instance opens; existing window focuses
- [ ] Corrupt library does not destroy data
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

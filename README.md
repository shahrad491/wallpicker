# wallpicker

A fast, keyboard-friendly wallpaper picker for **Hyprland/Wayland**, written in Rust.
It replaces the rofi + bash gallery with real categories, favorites, folder sources, and
`awww` transitions.

> **Status:** under construction. The CLI/contract below is the target design; unchecked
> roadmap items are not implemented yet. See [`ROADMAP.md`](ROADMAP.md).

---

## Features

- Browse every wallpaper from one or more source directories.
- Manual categories with many-to-many membership (create, rename, delete).
- **Favorites** pinned first; cannot be renamed or deleted.
- Automatic folder categories from each source's immediate subfolders (read-only, hideable).
- Star images, tag them into multiple categories, remove them from a category.
- Single click selects, double click applies; full keyboard navigation.
- Random wallpaper from the last category used in the GUI, via a global keybind.
- `awww` transitions with a `swaybg` fallback.
- Disk thumbnail cache, progressive loading, no full-size decoding in the UI.

---

## Install

### From source

```bash
git clone <repo-url> wallpicker   # or use the existing folder
cd wallpicker
cargo build --release
install -Dm755 target/release/wallpicker ~/.local/bin/wallpicker
```

Requirements at runtime:

| Tool | Purpose | Required |
|---|---|---|
| `awww` + `awww-daemon` | animated wallpaper transitions | recommended |
| `swaybg` | fallback static wallpaper | optional |
| `notify-send` | "Wallpaper changed" toast | optional |
| `zenity` | native "Browse…" for source directories | optional |
| `hyprctl` | monitor sizing + focusing an existing window | Hyprland |

---

## Commands

```
wallpicker                       Open the picker
wallpicker pick                  Open the picker (explicit)
wallpicker random                Random wallpaper from the last category used in the GUI
wallpicker random --category favorites
wallpicker random --category all
wallpicker random --category "Dark"
wallpicker random --no-rescan    Skip rescanning sources before choosing
wallpicker set <path>            Apply one image
wallpicker restore               Re-apply the last wallpaper (quiet; for login)
wallpicker rescan                Scan sources and print a summary
wallpicker --version
wallpicker --help
```

Examples:

```bash
# keyboard shortcut: open the picker
wallpicker

# keyboard shortcut: instant random from the last category selected in the GUI
wallpicker random

# always draw from Favorites
wallpicker random --category favorites

# set one specifically
wallpicker set ~/Pictures/wallpaper/gruvbox-wallpapers/mountain.png

# at login
wallpicker restore
```

### Behavior notes

- The GUI is single-instance. Launching it again focuses the existing window.
- `random` uses the last category selected in the picker. If that category is missing or
  empty it falls back to **All**. It avoids repeating the current wallpaper when possible.
- `random` from Favorites or a manual category reads stored paths; `all` or a folder category
  triggers a scan.
- Applying a wallpaper writes the legacy state file used by the old bash script, so
  `restore` and old tooling keep working.
- A successful apply closes the picker and sends one notification. Failure keeps the window
  open with an error.

---

## In-app controls

| Input | Action |
|---|---|
| Single click | Select image |
| Double click | Apply image |
| `Enter` | Apply selection |
| `Ctrl` + click | Toggle selection |
| `Shift` + click | Select range |
| Arrows / `h` `j` `k` `l` | Move selection |
| `Space` | Toggle favorite |
| `R` | Random from the selected category |
| `/` | Focus search |
| `Ctrl+A` / `Ctrl+D` | Select all / clear selection |
| `F2` | Rename selected category |
| `Esc` | Close dialog, then close the app |

Right-click a tile for: Apply · Toggle favorite · Add to category · Remove from current
category · Reveal in file manager · Copy path.

---

## Categories and sources

- **All wallpapers** is the union of every source directory.
- **Favorites** is built in. Images can be starred/unstarred; the category itself cannot be
  renamed or deleted.
- **Categories** are manual tags. An image can belong to many. Deleting a category only
  removes the tags — image files are never touched.
- **Folders** are derived automatically from the immediate subfolders of each source. Same
  names from different sources merge. They are read-only and can be hidden; they reappear
  after a rescan unless hidden.
- **Sources** are directories to scan. Add as many as you like; enable recursion and
  auto-folder categories per source.
- Removing a source asks for confirmation and removes that source's paths from All, folder
  categories, manual categories, and Favorites. Files on disk are untouched.
- Manual categories can only contain images from sources. To include loose images, add
  their directory as a source.

---

## Hyprland wiring

These snippets are **not applied automatically**. Copy the parts you want into your own
config. The examples use Hyprland 0.56 block-style window rules.

### Keybinds

`~/.config/hypr/core/keybindings.conf` (or wherever your binds live):

```ini
# wallpicker picker
bind = $mainMod, W, exec, wallpicker

# instant random from the last category used in the picker
bind = $mainMod SHIFT, W, exec, wallpicker random

# optional: instant random from Favorites
bind = $mainMod ALT, W, exec, wallpicker random --category favorites
```

Replace the existing line that launches `wallpaper.sh` if you want wallpicker to be the
default:

```ini
# old
# bind = $mainMod, W, exec, bash ~/.config/hypr/scripts/wallpaper.sh
```

### Autostart / restore

`~/.config/hypr/core/autostart.conf`:

```ini
exec-once = wallpicker restore
```

The old script can stay as a fallback; it remains compatible with the same state file.

### Window rules

Add a new file, e.g. `~/.config/hypr/ui/wallpicker.conf`, and source it from your UI config,
or paste this into an existing sourced file:

```ini
windowrule {
    name = wallpicker-window
    match:class = ^wallpicker$
    float = yes
    center = yes
    border_size = 0
}
```

If you are on Hyprland 0.52 or older, use the legacy one-line form:

```ini
windowrule = float, class:^(wallpicker)$
windowrule = center, class:^(wallpicker)$
```

Reload Hyprland after editing:

```bash
hyprctl reload
```

Verify the window class if a rule does not match:

```bash
hyprctl clients | grep -A2 -i wallpicker
```

---

## Configuration

`~/.config/wallpicker/config.toml` is created with defaults on first run.

```toml
[behavior]
close_on_select = true
notify = true
avoid_repeat = true
transition = "random"      # random | left | right | top | bottom | center | outer | fade | none
duration = 1.2             # seconds

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

`behavior.single_click_apply` is intentionally **not** configurable: single click selects,
double click applies.

---

## Data locations

| Path | Contents |
|---|---|
| `~/.config/wallpicker/config.toml` | User settings |
| `~/.local/share/wallpicker/library.json` | Sources, manual categories, favorites, last category |
| `~/.cache/wallpicker/thumbs/` | Thumbnail cache |
| `~/.cache/wallpicker/wallpicker.log` | Diagnostics |
| `~/.local/state/dotfiles/wallpaper/current` | Last applied wallpaper (legacy-compatible) |

`library.json` is written atomically. If it is ever corrupt, wallpicker backs it up instead
of overwriting it.

---

## How wallpapers are applied

1. If `awww` is available, use namespace `dotfiles` with the configured transition.
   Start `awww-daemon` detached if it is not running.
2. Otherwise spawn `swaybg -i <image> -m fill` and track its PID so the previous instance is
   replaced cleanly.
3. Atomically write the current wallpaper path to the legacy state file.
4. Send a desktop notification (unless disabled).

---

## Building and testing

```bash
cargo check
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo run -- --help
cargo run -- rescan
cargo build --release
```

Tests never use real wallpaper files; they generate small images in temporary directories.

---

## Troubleshooting

**Picker does not open / second launch does nothing**
The app is single-instance; the existing window should be focused. Check the log at
`~/.cache/wallpicker/wallpicker.log`.

**Wallpaper does not change**
Verify the daemon and namespace:

```bash
awww query --namespace dotfiles
awww-daemon --namespace dotfiles --no-cache &
```

Check that the image still exists and is readable. The app falls back to `swaybg` if
`awww` is unavailable.

**No thumbnails**
Delete the cache directory and reopen:

```bash
rm -rf ~/.cache/wallpicker/thumbs
```

**Window rules do not match**
Run `hyprctl clients` while the picker is open and confirm the class is `wallpicker`.

---

## Project documents

- [`ROADMAP.md`](ROADMAP.md) — milestones, decisions, architecture, verification checklist.
- [`AGENTS.md`](AGENTS.md) — rules and contracts for AI agents/contributors.

## License

TBD.

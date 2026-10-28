# Configuration and data

First run creates `~/.config/wallpicker/config.toml` with defaults. Invalid TOML or
invalid values use defaults for that run while preserving the file and logging the
reason. Edit the file, then restart the picker.

```toml
[behavior]
close_on_select = true
notify = true
avoid_repeat = true
transition = "random"
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

Transitions: `random`, `left`, `right`, `top`, `bottom`, `center`, `outer`, `fade`,
`none`. Duration is 0–60 seconds; dimensions are 1–4096; JPEG quality is 1–100;
workers are 1–32; window percentages are 1–100. Theme colors use `#RRGGBB`.
`close_on_select` remains in config for compatibility; apply/close behavior follows
its action intent: double-click/Enter closes, context-menu Apply keeps the picker open.
Single-click apply is not configurable.

## Optional live and rotation settings

```toml
[live]
output = "ALL"
fps = 30
muted = true
# assets_dir = "/path/to/owned/engine/assets"

[rotation]
interval = "30m"
include_live = false
notify = false
# category = "favorites" # omitted: use last selected GUI category
```

See [live wallpaper backends](live-wallpapers.md) and [background rotation](rotation.md).
Rotation is inactive until explicitly started; normal picker launch does not enable it.

## Files

| Default path | Contents |
|---|---|
| `~/.config/wallpicker/config.toml` | Settings |
| `~/.local/share/wallpicker/library.json` | Sources, manual categories, favorites, hidden folders, last category |
| `~/.cache/wallpicker/thumbs/` | Generated JPEG previews |
| `~/.cache/wallpicker/wallpicker.log` | Diagnostics; rotated when large |
| `~/.local/state/dotfiles/wallpaper/current` | Legacy-compatible current wallpaper |

Absolute `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_CACHE_HOME`, and `XDG_STATE_HOME`
change their respective roots. The backend reuses `awww` namespace `dotfiles`.
Application metadata writes use atomic same-directory replacement and mode `0600`.
Auto categories are derived from scans and are not saved as manual categories.

## Recovery

Corrupt library JSON is backed up beside the original as `.corrupt-*.bak`, and loading
stops with an error. The original is preserved. Restore a known-good backup or rename
the original explicitly before starting a fresh library. Future schema versions are
refused without rewriting them. Missing files stay in metadata until explicit cleanup.

Use the command palette's orphaned-thumbnail cleanup to remove unused previews.
Cache removal is separate from wallpaper files and does not change tags. If a preview
fails, inspect the log and verify the source is a readable image. Cache keys include
canonical path, modification time, file size, and target dimensions; source changes
invalidate previous previews.

The current state file is shared with the old wallpaper script. Applying a wallpaper
updates it only after backend success; keeping the old script offers a fallback during
migration.

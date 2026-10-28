# Using wallpicker

Start `wallpicker` or `wallpicker pick`. On first run it adds `~/Pictures/wallpaper`
as a recursive source if that directory exists. Otherwise choose **Sources** and add
a directory. Source files are read only: all tags and favorites live in metadata.

## Gallery

- A single click selects; double-click or `Enter` applies and closes after success.
- **Apply** in a tile's right-click/More menu applies while keeping the picker open,
  so you can try several wallpapers. A failure stays visible in the picker.
- `Ctrl`-click toggles selections; `Shift`-click selects a range.
- Arrows or `h j k l` move selection; `Space` toggles favorite.
- `/` focuses search. `Enter` selects the first matching image and returns to the gallery.
- `Ctrl+A` selects all visible results; `Ctrl+D` clears selection.
- `R` chooses randomly from the current category, avoiding the current wallpaper when possible.
- `F2` renames a manual category. `Esc` closes a dialog/menu first, then the picker.

The toolbar offers search, Name/Newest/Oldest sorting, Random, and Rescan. Cached
previews arrive progressively. Large files need decoding before their first preview;
subsequent launches reuse cache entries. The footer reports preview readiness.

## Categories and sources

**All** is the deduplicated union of scans. **Favorites** is a protected built-in
category. Manual categories are many-to-many tags: one image can appear in several.
Deleting a category removes metadata, never image files.

**Folders** are read-only categories named after each source's immediate subfolders;
deeper images join their first subfolder. Matching names merge across sources. Hiding
a folder changes visibility without changing the All pool. Unhide through `Ctrl+P`.

Sources have recursive and automatic-folder options. Duplicate or nested sources are
rejected. Scanning recognizes PNG, JPEG, WebP, GIF, BMP, and TIFF extensions and does
not follow symlinks. Removing a source requires confirmation and prunes that source's
tags and favorites. Turning recursion off preserves deeper stored tags for later.
Missing images are hidden; **Clean up missing entries** explicitly prunes metadata.

Use **Add to category** for selections, **Remove from category** for a manual category,
and the batch toolbar for multi-selection. Destructive metadata actions explain their
scope and require confirmation.

## Command palette

`Ctrl+P` searches actions, categories, and source settings. Use Up/Down and `Enter` to
execute. This provides a keyboard route to category/source management, sorting,
reveal/copy, hide/unhide, missing-entry cleanup, and orphaned-preview cleanup.

`Enter` submits category/source dialogs or confirms a destructive confirmation;
`Esc` cancels. In Add source, `Ctrl+R` toggles recursion, `Ctrl+F` toggles folder
categories, and `Ctrl+B` opens the optional Zenity browser.

## Headless commands

```sh
wallpicker random
wallpicker random --category favorites
wallpicker random --category all
wallpicker random --category "Dark"
wallpicker random --no-rescan
wallpicker set /absolute/path/to/wallpaper.png
wallpicker --quiet set /absolute/path/to/wallpaper.png
wallpicker restore
wallpicker rescan
wallpicker rotate --interval 30m
wallpicker rotate --once
wallpicker live stop
wallpicker --help
wallpicker --version
```

`random` uses the last category selected in the GUI unless overridden. Empty or stale
categories fall back to All. Favorites/manual categories reuse stored paths after
checking existence; All/folders and empty-category fallback scan sources even with
`--no-rescan`, because the scan pool is not persisted. An empty pool exits with an error.

`restore` quietly reads the legacy current-wallpaper state. `set` can apply a supported
image directly without adding it to sources. `rescan` reports image/folder/skipped
counts without applying a wallpaper. `--quiet` suppresses desktop notifications globally.
A second picker process asks Hyprland to focus the existing window and exits.

See [live wallpapers](live-wallpapers.md) for supported video/engine inputs and optional
backends, and [rotation](rotation.md) for scheduling and the inactive user service.

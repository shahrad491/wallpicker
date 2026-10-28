# Optional Hyprland integration

Install `wallpicker` somewhere on your session's `PATH` before using these snippets.
Apply them yourself after trying the app, keeping a timestamped backup of any config
you edit. The project build/package commands do not change your dotfiles. Keep the old
`wallpaper.sh` script for fallback; it shares the current-wallpaper state file.

## Keybinds

```ini
bind = $mainMod, W, exec, wallpicker
bind = $mainMod SHIFT, W, exec, wallpicker random
bind = $mainMod ALT, W, exec, wallpicker random --category favorites
```

Replace an existing `$mainMod, W` wallpaper binding if needed to avoid duplicates.
The last line is optional. Random uses the last selected GUI category by default.

## Login restore

```ini
exec-once = wallpicker restore
```

Use one restore mechanism at login rather than running both the old script and the
new command concurrently. Restore is quiet and does not open the picker.

## Window rules

For Hyprland 0.56 block-style rules:

```ini
windowrule {
    name = wallpicker-window
    match:class = ^wallpicker$
    float = yes
    center = yes
    border_size = 0
}
```

The picker requests a borderless window around 85% of the focused monitor's logical
size, accounting for scale and rotation. A 1200×800 fallback is used when monitor
information is unavailable. Window rules handle compositor positioning.

On older Hyprland versions that use legacy one-line rules:

```ini
windowrule = float, class:^(wallpicker)$
windowrule = center, class:^(wallpicker)$
```

After editing a sourced config, run `hyprctl reload`. Inspect `hyprctl configerrors`
and `hyprctl clients` if rules do not apply; the app id/class should be `wallpicker`.
These snippets still require validation in your actual compositor config.

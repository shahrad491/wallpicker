# Startup, rendering, and previews

Use an optimized build (`make build`) before comparing startup. The default software
renderer avoids GPU driver and shader initialization before the first window. First
use still initializes the window/font stack, scans sources, and decodes visible uncached
images. Only visible rows plus overscan are requested; later visits reuse cached JPEGs.
Actual cold-start time still depends on the host and needs an attended measurement.

## Renderer selection

With no graphics overrides, GUI launches default to `ICED_BACKEND=tiny-skia` for prompt
first-open behavior. Explicit `ICED_BACKEND` choices are preserved. Providing a
`WGPU_BACKEND` or `WGPU_POWER_PREF` override also selects hardware rendering if no
`ICED_BACKEND` is given. Hardware rendering defaults missing values to
`WGPU_BACKEND=vulkan` and `WGPU_POWER_PREF=low`; explicit values always win.
A safe re-exec applies defaults before graphics threads are created. Headless commands
do not need this GUI setup. No global shell environment is changed.

```sh
# Default software rendering:
./target/release/wallpicker

# Explicit GPU rendering (Vulkan/low-power defaults):
ICED_BACKEND=wgpu ./target/release/wallpicker

# Choose a particular hardware API/adapter preference:
ICED_BACKEND=wgpu WGPU_BACKEND=vulkan WGPU_POWER_PREF=high ./target/release/wallpicker
ICED_BACKEND=wgpu WGPU_BACKEND=gl ./target/release/wallpicker

# Force software rendering even if WGPU variables are exported in your session:
ICED_BACKEND=tiny-skia ./target/release/wallpicker
```

GPU rendering can be useful on large/high-DPI displays, but its first launch may need
shader compilation and driver initialization. Software rendering avoids that startup
cost; both backends use the same gallery, thumbnail cache, and input behavior.

For an installed binary, substitute `wallpicker` for `./target/release/wallpicker`.
An explicit backend may fail if its system driver is unavailable. Compare screenshots
with the same wallpaper and monitor settings; avoid switching several settings at once.

## Colors

The native Iced build disables its `web-colors` feature so native sRGB/linear surfaces
and image sampling use the intended color path. The app supplies its own theme, so
Linux desktop-theme detection is also disabled. Existing thumbnail JPEGs do not need
to be deleted for this renderer change.

Wide-gamut/HDR output and embedded ICC color management remain limited. Different
viewers may perform additional profile conversion or tone mapping; identical appearance
across them is not guaranteed. If a preview still differs, compare Vulkan and tiny-skia
on the same output and inspect the source file's profile/bit depth before rebuilding caches.

## Logs and timings

The default log records warnings/errors and startup checkpoints, including the first
redraw. Read `~/.cache/wallpicker/wallpicker.log` (or the corresponding XDG cache path).
First redraw means the first frame, not that every visible preview has finished decoding.

Enable additional app and driver information for diagnosis:

```sh
WALLPICKER_LOG=info ./target/release/wallpicker
```

Verbose driver logs can slow adapter probing and increase log volume; leave the default
level in place when measuring normal startup. Timings from different builds, cold/warm
caches, renderers, or monitor setups are not directly comparable.

## Other issues

- **No wallpapers:** open Sources, check the directory and recursion option, then Rescan.
- **Missing previews:** confirm files are readable images and inspect the log. Orphaned
  preview cleanup is available through `Ctrl+P`; source files are never removed.
- **Second launch does nothing:** inspect `hyprctl clients`; the existing picker should
  receive focus. Check that its class is `wallpicker`.
- **Apply fails:** check `awww query --namespace dotfiles` and that `swaybg` is available
  for fallback. Failure remains visible; the old wallpaper script can still be used.
- **Rules do not apply:** inspect `hyprctl configerrors` and the [Hyprland guide](hyprland.md).
- **Corrupt metadata:** follow [library recovery](configuration.md#recovery); the app preserves
  the original and creates a timestamped backup rather than silently resetting it.

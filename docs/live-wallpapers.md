# Video and Wallpaper Engine wallpapers

Live wallpapers are opt-in external backends. wallpicker does not install dependencies
or download video, workshop, or engine assets. Static images continue to use `awww`
with a `swaybg` fallback.

| Source | Backend | Preview |
|---|---|---|
| MP4, WebM, MKV, MOV, M4V, AVI | `mpvpaper` | Optional FFmpeg first-frame extraction |
| Wallpaper Engine project (`project.json` or its containing folder) | `linux-wallpaperengine` | Project preview when available; placeholder otherwise |

Add a source through the picker or apply directly:

```sh
wallpicker set /path/to/video.mp4
wallpicker set /path/to/wallpaper-engine-project
wallpicker set /path/to/wallpaper-engine-project/project.json
wallpicker live stop
```

Stopping live playback only stops processes tracked as owned by wallpicker; it does not
kill unrelated players. It keeps the stored current path, so `wallpicker restore` can
start that wallpaper again. A stopped live backend may leave the compositor's underlying
static wallpaper visible. Applying another wallpaper replaces app-owned playback.
Before switching back through the old bash wallpaper script, run `wallpicker live stop`:
that script cannot stop app-owned live players, which can remain above its static
background. It does not gain support for restoring these live paths. Applying a static
wallpaper through wallpicker stops its own live playback automatically.

## Configuration

```toml
[live]
output = "ALL"
fps = 30
muted = true
# assets_dir = "/path/to/owned/Wallpaper Engine/assets"
```

`ALL` selects outputs; Wallpaper Engine output discovery uses `hyprctl monitors -j`.
An explicit monitor name such as `DP-1` bypasses that discovery. Change the FPS and mute
settings to suit your system. Engine asset paths must point to assets you own and
provide; no licensed content is bundled. Preserve source project files and videos.

Install `mpvpaper` separately for videos, FFmpeg for first-frame previews, and a
compatible `linux-wallpaperengine` for engine projects. Missing backends produce an
actionable error. FFmpeg preview extraction is time-bounded and runs in background
workers; it does not start video playback or an engine instance just to draw a tile.

Wallpaper Engine compatibility varies by project type and the installed Linux engine
implementation. Web, scene, shader, and effect support should not be assumed complete.
Use projects/assets you have permission to use and test each on your own compositor.
These optional playback backends are absent on this development host, so live playback
is not claimed as tested here.

Background rotation excludes live entries by default. Enable `rotation.include_live`
or `rotate --include-live` explicitly if you want to cycle them; expect higher CPU/GPU
usage and backend restarts. See [rotation](rotation.md).

# Picker implementation notes

M5–M7: Iced 0.14 application with configurable palette, focused-monitor logical sizing,
virtualized 16:9 thumbnail rows, asynchronous scans/apply, progressive cached previews,
serialized metadata snapshots, and close-after-save behavior. Main owns the process lock.

The UI supports category/source CRUD, folder hide/unhide, manual batch membership,
favorites, missing-entry cleanup, orphaned cache cleanup, search, sorting, reveal/copy,
and confirmations that explain metadata-only removal. Apply failures remain visible.

Every action is also available through **Ctrl+P**, a searchable command palette.
Type a category, source, or action name; use **Up/Down** and **Enter** to execute.
This is the keyboard route for sidebar selection and management, since Iced buttons
are not keyboard focusable. **Enter** confirms a destructive confirmation; **Esc** cancels.
Category-name and source-path dialogs submit with **Enter**. In Add source,
**Ctrl+R** toggles recursion, **Ctrl+F** toggles folder categories, and **Ctrl+B** browses.
The existing gallery shortcuts (arrows/hjkl, Space, R, /, Ctrl+A, Ctrl+D, F2) remain.

Unit tests cover range/toggle selection, logical and rotated monitor sizing, category
persistence and selection reset, queued saves on close, apply failure state, and palette
management commands. Real-window appearance, scale-factor behavior, scroll smoothness,
and compositor focus need a short attended GUI test before marking visual QA complete.

Tile right-click and More open a floating context menu at the last pointer position.
The menu is clamped to the window viewport, scrolls on short windows, and dismisses
when clicking outside it. Destructive confirmations remain centered modals.

Live-test fixes: selection and error controls now occupy stable widget-tree slots;
the root stays a Stack when opening or closing dialogs. This preserves the gallery's
Scrollable state instead of recreating it on first selection. A headless tree-diff
regression marks the private scroll-state slots and verifies continuity through first
selection, an error banner, and opening/closing a context menu. Attended scrolling
verification remains useful for compositor/layout behavior.

Preview requests now cover only the rendered viewport plus overscan. Scrolling,
resizing, searching, sorting, and category changes update worker priorities so obsolete
queued jobs are skipped. The footer reports ready previews for this window, with
unreadable previews counted separately. The full collection is no longer queued ahead
of the category being browsed.

Context-menu Apply now applies the wallpaper while keeping the picker open, so it can
be tested against the desktop. The menu closes when the request starts; success updates
the Current badge and status to the path that was actually applied. Failures remain
visible and preserve the previous Current badge. Double-click, Enter, toolbar Apply,
and Random keep their existing apply-and-close behavior. Concurrent apply requests are
ignored, and each asynchronous completion carries its own path and close intent.
The same keep-open action is available in Ctrl+P as “Test selected wallpaper (keep open)”.

Playback & rotation settings are available from the sidebar and Ctrl+P. The dialog
edits an interval (`30s`, `15m`, `2h`, `1d`), optional category (blank follows the last
GUI category), inclusion of live media, rotation notifications, live output, frame
rate, audio mute, and optional absolute Wallpaper Engine assets path. Draft validation
precedes config updates; saves use a separate serialized queue from library metadata,
and closing waits for both queues. Enter or Ctrl+S saves; Tab moves between fields;
Ctrl+L toggles live rotation, Ctrl+N notifications, Ctrl+M mute. Closing leaves unsaved
draft changes unapplied.

Start / restart saved settings, Stop, and Refresh status operate only the explicitly
installed `wallpicker-rotation.service` in the user's systemd instance. Starting never
installs or enables the unit. Save changes before restarting; the service waits for
the first interval before changing a wallpaper. Tests construct service argument and
output fixtures without invoking systemctl or changing any service.

Video and Wallpaper Engine entries have media badges. Project captions, search, and
name sorting prefer scanner-provided titles, with a project-folder fallback. Browsing
and thumbnail generation do not start live playback; applying an entry does.
Before an explicit Start/restart, the picker imports only available session-variable
names (`WAYLAND_DISPLAY`, `DISPLAY`, `HYPRLAND_INSTANCE_SIGNATURE`, `PATH`) into the
systemd user manager. Values are not printed. Starting without `WAYLAND_DISPLAY`
returns a visible error. Stop and Refresh do not import or require a display.

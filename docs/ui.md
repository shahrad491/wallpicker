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

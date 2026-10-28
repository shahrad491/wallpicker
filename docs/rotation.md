# Optional background rotation

wallpicker can run a headless rotation loop with `wallpicker rotate`. The default
interval is 30 minutes. Rotation is opt-in: installing a package or opening the picker
does not start a daemon. The user service is installed but not enabled.

In `~/.config/wallpicker/config.toml`:

```toml
[rotation]
interval = "30m"
include_live = false
notify = false
# category = "favorites" # optional; otherwise use the last selected GUI category
```

Keep live wallpapers excluded when you want a static rotation pool. `notify = false`
avoids a desktop toast at each background change. Empty-category fallback and
avoid-repeat behavior follow the headless random selector. Use a source/category with
at least two images if you want visible changes each interval.

## Start in the current session

```sh
wallpicker rotate
```

The loop waits for the first interval before changing the wallpaper. To apply a single
rotation immediately, or override loop settings:

```sh
wallpicker rotate --once
wallpicker rotate --interval 10m --category favorites
wallpicker rotate --include-live
```

`--once` performs one change and exits. `--include-live` permits video/engine entries;
the default rotation pool contains still images only.

Stop the loop with `Ctrl+C`. This stops future rotation; the detached wallpaper backend keeps
the currently displayed wallpaper. Do not run multiple independent rotators for the
same session. Configuration is re-read at each cycle. Restart the service after changing settings if
you need the new interval/settings to take effect immediately.

## Enable the user service explicitly

After installing to a stable prefix, use the picker's **Start** action to start rotation
in the current graphical session. The UI imports `PATH`, `WAYLAND_DISPLAY`, `DISPLAY`,
and `HYPRLAND_INSTANCE_SIGNATURE` into the systemd user manager before restarting the
unit. This does not implicitly enable login startup.

To enable login startup explicitly, run these yourself from a graphical session:

```sh
systemctl --user import-environment PATH WAYLAND_DISPLAY DISPLAY HYPRLAND_INSTANCE_SIGNATURE
systemctl --user daemon-reload
systemctl --user enable --now wallpicker-rotation.service
```

The installer renders the executable path into the unit, so it does not depend on
`~/.local/bin` being in systemd's PATH. User installation puts the unit in
`~/.local/share/systemd/user/`; system packages use `/usr/share/systemd/user/`.
Custom nonstandard prefixes may require making the unit discoverable in your user's
systemd unit search path. If you override `XDG_DATA_HOME`/`XDG_CONFIG_HOME`, ensure the
app process and systemd user manager agree on those values.

```sh
systemctl --user status wallpicker-rotation.service
journalctl --user -u wallpicker-rotation.service
systemctl --user disable --now wallpicker-rotation.service
```

The service is attached to `graphical-session.target`. Your compositor/session must
manage that target and provide the Wayland environment. `ConditionEnvironment=WAYLAND_DISPLAY`
prevents a start when the user manager has no Wayland display; import the environment
from your active session before starting. `KillMode=process` deliberately
stops the rotator without killing its detached wallpaper children; backend replacement
and PID ownership remain managed by wallpicker. Logging out/compositor shutdown may
still terminate the wallpaper backend. No service command above is run by packaging.

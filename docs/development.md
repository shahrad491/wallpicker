# Development and verification

Builds are pinned by `Cargo.lock`; Iced is pinned to `=0.14.0` with `image`, `lazy`,
`advanced`, and `tokio`. Explicit native renderer features replace Iced defaults:
`web-colors` and Linux desktop-theme detection are disabled. See
[renderer troubleshooting](troubleshooting.md) for software-first startup and GPU overrides. Dependencies are optimized in development builds so image decoding and
rendering remain usable with `cargo run`. Release mode is recommended for daily use.

```sh
make check
make build
make package-check
```

`make check` runs formatting, Clippy on all targets with warnings denied, and tests.
`make package-check` tests archive layout/permissions/checksum using a tiny shell fixture
executable, then stages repeated installs inside `target/` and verifies executable/launcher
backups. When dpkg-deb/readelf are available, it also builds a `.deb` with a trusted
ELF fixture, checks dependencies/layout, and verifies there is no activation script.
It does not claim the Rust application compiled or run a GUI. To verify the
real release, run `make package`, extract it, and check `bin/wallpicker --version`.

## Isolated scan smoke test

From the repository:

```sh
mkdir -p .cache/smoke/config .cache/smoke/data .cache/smoke/cache .cache/smoke/state
XDG_CONFIG_HOME="$PWD/.cache/smoke/config" \
XDG_DATA_HOME="$PWD/.cache/smoke/data" \
XDG_CACHE_HOME="$PWD/.cache/smoke/cache" \
XDG_STATE_HOME="$PWD/.cache/smoke/state" \
  ./target/release/wallpicker rescan
```

This writes app data inside the repository and may read `~/Pictures/wallpaper` as the
default source. It does not apply a wallpaper. `set`, `random`, and `restore` still
change the live desktop even when data paths are isolated. Tests generate small PNGs
in temporary directories rather than using real wallpaper files.

## Modules

| Module | Responsibility |
|---|---|
| `main`, `cli`, `paths` | Dispatch, single-instance ownership, diagnostics, injectable XDG paths |
| `startup` | Safe GUI graphics defaults, startup checkpoints, native color-policy regression |
| `config` | TOML defaults, validation, explicit saves |
| `model/library`, `model/store` | Metadata operations, schema and corruption protection, atomic persistence |
| `model/scan` | Source walking, canonical deduplication, folder categories |
| `thumbs` | Bounded background workers, crop/resize/cache, progressive delivery |
| `wallpaper` | `awww`/`swaybg`, state, notifications, random fallback |
| `app`, `message`, `theme`, `ui` | Iced state/tasks, keyboard and pointer interaction, dialogs, gallery |

Scanning, decoding, and external wallpaper commands must not block Iced's event
thread. The gallery renders visible thumbnail rows with overscan. Metadata saves are
serialized; category switches persist the last category. Deleting categories or sources
changes only metadata and never deletes a wallpaper. See [UI notes](ui.md) for detailed
state-preservation regressions and implementation history.

Automated gates do not replace attended tests of live apply/restore, compositor focus,
monitor scaling, preview colors, scroll behavior, or the 1,300-image collection. Record
those results in the roadmap rather than marking unrun desktop checks complete.

`target/`, `.cache/`, logs, staged files, and package archives are generated artifacts;
keep them out of commits. The license remains undecided; packaging does not grant or
invent a license.

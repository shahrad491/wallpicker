# Build, package, and install

wallpicker targets Linux on Wayland, particularly Hyprland. Build with stable Rust
supporting edition 2024 and the checked-in dependency lockfile. The default Iced
renderers use the system graphics stack; keep your distribution's working Wayland,
XKB, and graphics driver libraries installed. This project does not install packages.

Runtime tools:

| Tool | Use |
|---|---|
| `awww` and `awww-daemon` | Recommended wallpaper backend; namespace `dotfiles` |
| `swaybg` | Static fallback if `awww` is unavailable or fails |
| `mpvpaper`, `ffmpeg`, `linux-wallpaperengine` | Optional live/video/Wallpaper Engine support; install separately as needed |
| `notify-send` | Optional desktop notifications |
| `zenity` | Optional directory browser; editable source path works without it |
| `hyprctl` | Hyprland monitor sizing and focus of an existing picker |
| `xdg-open` | Reveal an image in the file manager |

## Build and try

Run from the repository:

```sh
make build
./target/release/wallpicker --help
./target/release/wallpicker
```

`make build` runs `cargo build --release --locked`. The first build downloads crates
and compiles Iced. `make run` builds and opens the optimized picker. Cargo alone works
if Make is unavailable: `cargo run --release --locked`.

## Create a release archive

```sh
make package
```

Outputs are `target/dist/wallpicker-<version>-linux-<architecture>.tar.gz` and its
`.sha256` checksum. The archive includes `bin/wallpicker`, a standalone `install.sh`, desktop launcher, SVG
icon, inactive rotation-service template, README/docs, and an `INSTALL.txt` quick guide. It does not include
wallpapers, caches, or user config. Packaging builds with `Cargo.lock`; it is a native
build for the current Linux architecture, not a cross-compilation command.

To package an already-built release:

```sh
./scripts/package.sh --skip-build
./scripts/package.sh --skip-build --output-dir "$PWD/target/my releases"
```

`--binary PATH` with `--skip-build` selects a different executable explicitly. This
bypasses building, so the caller must ensure that binary matches the source version
and host architecture. `CARGO_TARGET_DIR` is respected for native Cargo output.
Cross-target builds (`CARGO_BUILD_TARGET` or Cargo target configuration) require passing
the correct executable explicitly; the archive architecture comes from `uname -m`.

The tarball is movable but is not a static or universal Linux bundle. It uses runtime
shared libraries and a compatible host graphics stack; wallpaper tools are separate.
Inspect `ldd bin/wallpicker` on the destination if a shared library is missing.

## Use an archive

From the directory containing the archive:

```sh
sha256sum -c wallpicker-*.tar.gz.sha256
# Select the archive you want if several versions are present:
tar -xzf wallpicker-0.1.0-linux-x86_64.tar.gz
cd wallpicker-0.1.0-linux-x86_64
./bin/wallpicker
```

Use the architecture/version actually produced on your machine. The checksum detects
accidental corruption; it is not an authentication signature.

## Install explicitly

To install the binary, launcher, and docs from source for your user:

```sh
make install
```

The default prefix is `~/.local`. Ensure `~/.local/bin` is in your session's `PATH`.
The installed desktop entry uses an absolute executable path, `Icon=wallpicker`, and
`StartupWMClass=wallpicker`, so the app launcher works even if your shell PATH needs
updating. The installer prints bash/zsh and fish PATH commands without editing rc files. Installation
creates timestamped `.bak-YYYYMMDD-HHMMSS` backups beside any existing wallpicker
binary and launcher before replacement; repeated installs in the same second use a
numeric suffix. A failed backup stops installation before either is replaced. It does
not change Hyprland config.

For a distribution-style staging directory:

```sh
make install PREFIX=/usr DESTDIR="$PWD/target/stage"
```

This writes under `target/stage/usr/` without changing `/usr`. `DESTDIR` is a staging
root, and `PREFIX` is the final install prefix. `make install` rebuilds with `--locked`.

From an extracted archive, one command installs for your user:

```sh
./install.sh
```

Keep your existing wallpaper script as a fallback. [Hyprland wiring](hyprland.md) is
optional and applied by you after trying the picker.

See [package choices](packaging.md) for `.deb`, optional AppImage, and current Flatpak
limitations. The rotation service is installed but stays inactive until you
[enable it explicitly](rotation.md).

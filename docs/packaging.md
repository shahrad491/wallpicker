# Choosing a package format

| Format | Build command | Status and limits |
|---|---|---|
| Native binary | `make build` | `target/release/wallpicker`; needs compatible Linux shared libraries |
| `.tar.gz` | `make package` | Implemented; extracted `./install.sh` installs for the current user |
| `.deb` | `make package-deb` | Implemented host-native package; use the opt-in Debian container recipe below for an older distribution baseline |
| AppImage | `make package-appimage` | Optional builder; requires `linuxdeploy` and `appimagetool` already on PATH |
| Flatpak | No supported target yet | Host wallpaper integration needs a deliberate sandbox/host-helper design |

No packaging target installs system packages or enables background services. Output
archives and SHA-256 files go under `target/dist/`. A package includes no wallpapers
or Wallpaper Engine assets and downloads no backend tools.

## Native tar archive

Extract the archive, then run `./bin/wallpicker` without installation or `./install.sh`
to install the binary, absolute-path desktop launcher, SVG icon, documentation, and an
inactive user rotation service under `~/.local`. Existing executable, launcher, icon,
and unit receive timestamped backups. The installer prints shell-specific PATH setup;
it never edits your shell startup files. Prefixes containing spaces are supported.

```sh
tar -xzf wallpicker-0.1.0-linux-x86_64.tar.gz
cd wallpicker-0.1.0-linux-x86_64
./install.sh
```

Replace the version/architecture with your actual archive name. `--prefix` and
`--destdir` work in both source and extracted installers. Backends and system graphics
libraries remain host requirements; this is not a universal static distribution.

## Debian package

```sh
make package-deb
# Inspect without installing:
dpkg-deb --info target/dist/wallpicker_0.1.0_amd64.deb
dpkg-deb --contents target/dist/wallpicker_0.1.0_amd64.deb
```

The package places the app in `/usr/bin`, its launcher/icon under `/usr/share`, and the
inactive rotation unit in `/usr/share/systemd/user`. There are no maintainer scripts:
installation never enables rotation, changes Hyprland, or applies a wallpaper.
`DEB_MAINTAINER='Your name <your-address>' make package-deb` sets maintainer metadata;
the fallback is `wallpicker contributors` without an invented contact address.

For distributable Debian/Ubuntu artifacts, build in a container/CI runner for the
oldest release you support. On a native Debian-family builder, `dpkg-shlibdeps` generates
linked-library requirements; runtime-loaded Iced X11/Wayland/XKB dependencies are added
explicitly. See [Debian's shared-library policy](https://www.debian.org/doc/debian-policy/ch-sharedlibs.html).

On this CachyOS host, Debian package databases cannot determine library dependencies.
The builder checks the maximum referenced GLIBC symbol and maps only known SONAMEs;
unknown libraries stop packaging. Such an artifact is structurally valid, **not verified
compatible with Debian/Ubuntu**. A new glibc ABI floor can exclude existing releases;
the current host release references **GLIBC 2.44**, and renaming an archive to `.deb`
does not solve that. Inspect its `Depends` and rebuild on
the target distribution. Backend availability and live-wallpaper behavior still need
interactive testing on each supported distribution.

## Opt-in Debian container build

The checked-in [`packaging/debian.Dockerfile`](../packaging/debian.Dockerfile) provides a
concrete alternative to exporting a CachyOS-linked binary. It builds the Rust release
inside Debian Bookworm, generates dependencies using that container's Debian package
database, and exports only `.deb`, `.tar.gz`, and SHA-256 files. No host binary or cache
is copied into the build; `.dockerignore` excludes local state and wallpaper files while
retaining `Cargo.lock`.

With Docker BuildKit/Buildx available, run this yourself from the repository:

```sh
docker buildx build \
  --file packaging/debian.Dockerfile \
  --output type=local,dest=target/debian-dist \
  .
```

This opt-in command downloads a base image/crates and installs build packages **inside
the build container**; exported artifacts stay under `target/debian-dist/`. It does not
install the app on the host, push an image, start wallpaper backends, or enable a service.
The scratch final stage and [Docker local exporter](https://docs.docker.com/build/exporters/local-tar/)
copy only the resulting package files to the chosen local directory.

The default `RUST_IMAGE=rust:1-bookworm` is a supported tag of the
[Docker Official Rust image](https://hub.docker.com/_/rust), maintained in
[rust-lang/docker-rust](https://github.com/rust-lang/docker-rust). Override the base or
maintainer explicitly if needed:

```sh
docker buildx build \
  --file packaging/debian.Dockerfile \
  --build-arg RUST_IMAGE=rust:1-bookworm \
  --build-arg 'DEB_MAINTAINER=Your name <your-address>' \
  --output type=local,dest=target/debian-dist \
  .
```

For reproducible release infrastructure, pin the chosen image by digest as well as
keeping `Cargo.lock`; the floating Rust tag and Debian repository packages can change.
Choose an actual Debian-based Rust builder rather than substituting an Ubuntu runtime
image without a Rust toolchain. Builds are native to the selected container platform;
architecture emulation/cross-building requires separate setup and testing.

**Recipe status:** reviewed but not built here. No container was run, image pulled, or
package installed to validate this path. A Bookworm-built artifact should avoid this
host's GLIBC 2.44 floor, but inspect its actual `Depends` and test installation, GUI,
and backends on each target Debian/Ubuntu release before calling it supported. Package
format compatibility does not guarantee compositor/backend availability.

## AppImage

The target checks for `linuxdeploy` and `appimagetool`, stages an AppDir with binary,
launcher, icon, and docs, bundles discoverable dependencies, then invokes appimagetool.
Neither tool is downloaded or installed automatically. They are absent on this host,
so an AppImage is not claimed as built or tested here. The workflow follows the
[AppImage packaging guide](https://docs.appimage.org/packaging-guide/manual.html).

Build on an appropriately old base distribution. Graphics drivers and dynamically
loaded libraries require testing; AppImage does not automatically bundle every Wayland
integration or the external wallpaper backends. It does not remove glibc compatibility
limits. The optional systemd rotation service targets an installed native executable;
install a stable binary path before configuring that service.

## Flatpak

wallpicker currently invokes host tools (`awww`, `swaybg`, and optional live backends),
connects to the compositor/daemon sockets, writes legacy wallpaper state, and tracks
host backend process ownership. Ordinary Flatpak permissions alone do not make these
host commands and PIDs equivalent to sandbox commands and PIDs.

A supported Flatpak needs a reviewed host-helper/portal strategy, session socket and
source-directory access, state-path ownership, backend lifecycle handling, and a test
matrix for the intended compositors. A broad `flatpak-spawn --host` escape would also
need permissions and a security/support decision. There is no working Flatpak manifest
or zero-config sandbox claim in this release. Prefer the native package for now.

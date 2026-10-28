#!/bin/sh
# Native Linux release archive; no installation or desktop changes.
set -eu
project_root=$(CDPATH= cd "$(dirname "$0")/.." && pwd)
output_dir=$project_root/target/dist
skip_build=false
binary=
format=tar
usage() {
    printf '%s\n' 'Usage: scripts/package.sh [--format tar|deb|appimage] [--skip-build] [--binary PATH] [--output-dir DIR]' \
        'Build a locked native release and package it with its launcher and documentation.' \
        '--skip-build uses an existing binary; --binary requires --skip-build.'
}
while [ "$#" -gt 0 ]; do
    case "$1" in
        --skip-build) skip_build=true; shift ;;
        --binary|--output-dir|--format)
            option=$1
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            case "$option" in --format) format=$2 ;; --binary) binary=$2 ;; --output-dir) output_dir=$2 ;; esac
            shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) printf 'Unknown argument: %s\n' "$1" >&2; usage >&2; exit 2 ;;
    esac
done
case "$format" in tar|deb|appimage) ;; *) printf 'Unknown format: %s\n' "$format" >&2; exit 2 ;; esac
case "$format" in
 deb) command -v dpkg-deb >/dev/null && command -v readelf >/dev/null || { printf '%s\n' 'Debian packaging requires dpkg-deb and readelf.' >&2; exit 1; } ;;
 appimage) command -v linuxdeploy >/dev/null && command -v appimagetool >/dev/null || { printf '%s\n' 'AppImage packaging needs linuxdeploy and appimagetool on PATH; no tools are downloaded automatically.' >&2; exit 1; } ;;
esac
[ "$(uname -s)" = Linux ] || { printf '%s\n' 'Packaging currently supports native Linux builds.' >&2; exit 1; }
if [ -n "$binary" ] && [ "$skip_build" != true ]; then
    printf '%s\n' '--binary requires --skip-build.' >&2; exit 2
fi
if [ "$skip_build" != true ]; then
    [ -z "${CARGO_BUILD_TARGET:-}" ] || { printf '%s\n' 'CARGO_BUILD_TARGET is set; packaging requires a native build.' >&2; exit 2; }
    (cd "$project_root" && "${CARGO:-cargo}" build --release --locked)
fi
if [ -z "$binary" ]; then
    target_dir=${CARGO_TARGET_DIR:-$project_root/target}
    case "$target_dir" in /*) ;; *) target_dir=$project_root/$target_dir ;; esac
    binary=$target_dir/release/wallpicker
fi
[ -f "$binary" ] && [ -x "$binary" ] || { printf 'Executable not found: %s\n' "$binary" >&2; exit 1; }
version=$(awk '/^\[package\]$/ { package=1; next } /^\[/ { package=0 } package && /^version[[:space:]]*=/ { gsub(/"/, "", $3); print $3; exit }' "$project_root/Cargo.toml")
case "$version" in ''|*[!0-9A-Za-z.+-]*) printf '%s\n' 'Cannot read a safe package version from Cargo.toml.' >&2; exit 1 ;; esac
arch=$(uname -m)
case "$arch" in ''|*[!0-9A-Za-z_-]*) printf '%s\n' 'Unsupported architecture name.' >&2; exit 1 ;; esac
package_name=wallpicker-$version-linux-$arch
mkdir -p "$output_dir"
output_dir=$(CDPATH= cd "$output_dir" && pwd)
stage=$(mktemp -d "$output_dir/.wallpicker-package.XXXXXX")
trap 'rm -rf "$stage"' EXIT HUP INT TERM
package_root=$stage/$package_name
mkdir -p "$package_root/bin" "$package_root/share/applications" "$package_root/share/doc/wallpicker/docs"
install -m755 "$binary" "$package_root/bin/wallpicker"
install -m644 "$project_root/assets/wallpicker.desktop" "$package_root/share/applications/wallpicker.desktop"
install -m644 "$project_root/README.md" "$project_root/ROADMAP.md" "$project_root/AGENTS.md" "$package_root/share/doc/wallpicker/"
install -m644 "$project_root"/docs/*.md "$package_root/share/doc/wallpicker/docs/"
install -Dm644 "$project_root/assets/wallpicker.svg" "$package_root/share/icons/hicolor/scalable/apps/wallpicker.svg"
install -Dm644 "$project_root/assets/wallpicker-rotation.service.in" "$package_root/share/wallpicker/wallpicker-rotation.service.in"
install -m755 "$project_root/scripts/install.sh" "$package_root/install.sh"
cat > "$package_root/INSTALL.txt" <<'NOTE'
Run without installation:
    ./bin/wallpicker

Install for the current user, from this extracted directory:
    ./install.sh

The installer backs up previous files, renders an absolute-path desktop launcher,
installs the icon and optional user rotation service, and prints shell PATH setup.
The rotation service is NOT enabled automatically.
This is a native Linux binary; runtime shared libraries and wallpaper backends are
not bundled. See share/doc/wallpicker/docs/installation.md for requirements.
Hyprland wiring is optional and user-applied. No wallpaper files are modified.
NOTE
case "$format" in
 tar)
    archive=$package_name.tar.gz
    tar -C "$stage" -czf "$stage/$archive" "$package_name"
    ;;
 deb)
    "$project_root/scripts/package-deb.sh" "$binary" "$package_root" "$stage" "$version"
    set -- "$stage"/*.deb
    archive=$(basename "$1")
    ;;
 appimage)
    appdir=$stage/Wallpicker.AppDir
    mkdir -p "$appdir/usr"
    cp -R "$package_root/bin" "$package_root/share" "$appdir/usr/"
    # AppImage launchers must stay relocatable rather than using install prefixes.
    linuxdeploy --appdir "$appdir" --executable "$appdir/usr/bin/wallpicker" --desktop-file "$project_root/assets/wallpicker.desktop" --icon-file "$project_root/assets/wallpicker.svg"
    archive=wallpicker-$version-$arch.AppImage
    ARCH="$arch" appimagetool "$appdir" "$stage/$archive"
    ;;
esac
mv "$stage/$archive" "$output_dir/$archive"
(cd "$output_dir" && sha256sum "$archive" > "$stage/$archive.sha256")
mv "$stage/$archive.sha256" "$output_dir/$archive.sha256"
printf 'Archive: %s\nChecksum: %s\n' "$output_dir/$archive" "$output_dir/$archive.sha256"

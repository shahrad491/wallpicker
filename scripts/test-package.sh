#!/bin/sh
# Exercise paths containing spaces, payload layout, permissions, and checksum.
set -eu
project_root=$(CDPATH= cd "$(dirname "$0")/.." && pwd)
mkdir -p "$project_root/target"
work=$(mktemp -d "$project_root/target/package-smoke.XXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM
mkdir -p "$work/fixture with spaces"
fixture=$work/fixture\ with\ spaces/wallpicker
printf '#!/bin/sh\nprintf "packaging fixture\\n"\n' > "$fixture"
chmod 755 "$fixture"
"$project_root/scripts/package.sh" --skip-build --binary "$fixture" --output-dir "$work/output with spaces"
set -- "$work/output with spaces"/*.tar.gz
archive=$1
(cd "$work/output with spaces" && sha256sum -c ./*.sha256)
mkdir "$work/extracted"
tar -xzf "$archive" -C "$work/extracted"
set -- "$work/extracted"/wallpicker-*
package=$1
[ -x "$package/bin/wallpicker" ]
[ "$("$package/bin/wallpicker")" = 'packaging fixture' ]
[ -f "$package/share/applications/wallpicker.desktop" ]
[ -f "$package/share/doc/wallpicker/README.md" ]
[ -f "$package/share/doc/wallpicker/docs/installation.md" ]
[ -f "$package/INSTALL.txt" ]
[ -x "$package/install.sh" ]
[ -f "$package/share/icons/hicolor/scalable/apps/wallpicker.svg" ]
[ -f "$package/share/wallpicker/wallpicker-rotation.service.in" ]
# An extracted archive is independently installable, with no source tree dependency.
"$package/install.sh" --prefix '/opt/wall picker' --destdir "$work/archive-stage"
grep -F 'Exec="/opt/wall picker/bin/wallpicker"' "$work/archive-stage/opt/wall picker/share/applications/wallpicker.desktop"
grep -F 'ExecStart="/opt/wall picker/bin/wallpicker" rotate' "$work/archive-stage/opt/wall picker/share/systemd/user/wallpicker-rotation.service"
grep -F 'ConditionEnvironment=WAYLAND_DISPLAY' "$work/archive-stage/opt/wall picker/share/systemd/user/wallpicker-rotation.service"
if "$project_root/scripts/package.sh" --skip-build --binary "$work/missing" --output-dir "$work/invalid" >/dev/null 2>&1; then
    printf '%s\n' 'Missing binary unexpectedly accepted.' >&2; exit 1
fi
stage=$work/staged\ install
"$project_root/scripts/install.sh" --binary "$fixture" --prefix /usr --destdir "$stage"
printf 'old executable\n' > "$stage/usr/bin/wallpicker"
printf 'old launcher\n' > "$stage/usr/share/applications/wallpicker.desktop"
"$project_root/scripts/install.sh" --binary "$fixture" --prefix /usr --destdir "$stage"
set -- "$stage/usr/bin"/wallpicker.bak-*
[ "$#" -eq 1 ] && [ "$(cat "$1")" = 'old executable' ]
set -- "$stage/usr/share/applications"/wallpicker.desktop.bak-*
[ "$#" -eq 1 ] && [ "$(cat "$1")" = 'old launcher' ]
[ "$("$stage/usr/bin/wallpicker")" = 'packaging fixture' ]
# Two replacements in the same second must keep separate backups.
"$project_root/scripts/install.sh" --binary "$fixture" --prefix /usr --destdir "$stage"
set -- "$stage/usr/bin"/wallpicker.bak-*
[ "$#" -eq 2 ]
set -- "$stage/usr/share/applications"/wallpicker.desktop.bak-*
[ "$#" -eq 2 ]
if command -v dpkg-deb >/dev/null && command -v readelf >/dev/null; then
    # Use a trusted native ELF fixture so ABI/dependency detection is exercised.
    "$project_root/scripts/package.sh" --format deb --skip-build --binary /bin/true --output-dir "$work/debian"
    set -- "$work/debian"/*.deb
    deb=$1
    dpkg-deb --field "$deb" Depends | grep -F 'libc6 (>='
    dpkg-deb --extract "$deb" "$work/debian-extracted"
    [ -x "$work/debian-extracted/usr/bin/wallpicker" ]
    [ -f "$work/debian-extracted/usr/share/systemd/user/wallpicker-rotation.service" ]
    dpkg-deb --control "$deb" "$work/debian-control"
    [ ! -e "$work/debian-control/postinst" ]
fi
printf '%s\n' 'Packaging and staged-install smoke tests passed (fixture binary; no app build or user install).' 

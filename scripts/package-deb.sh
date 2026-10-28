#!/bin/sh
# Internal native .deb assembly. Build on the intended Debian/Ubuntu release for distribution.
set -eu
export LC_ALL=C
binary=$1
payload=$2
stage=$3
version=$4
case "$(uname -m)" in x86_64) architecture=amd64 ;; aarch64) architecture=arm64 ;; *) printf '%s\n' 'Debian packaging supports native x86_64/aarch64 only.' >&2; exit 1 ;; esac
root=$stage/deb-root
mkdir -p "$root/usr" "$root/DEBIAN"
cp -R "$payload/bin" "$payload/share" "$root/usr/"
rm "$root/usr/share/wallpicker/wallpicker-rotation.service.in"
rmdir "$root/usr/share/wallpicker"
mkdir -p "$root/usr/share/systemd/user"
sed 's|@EXECUTABLE@|/usr/bin/wallpicker|g' "$payload/share/wallpicker/wallpicker-rotation.service.in" > "$root/usr/share/systemd/user/wallpicker-rotation.service"
sed 's|^Exec=.*|Exec=/usr/bin/wallpicker|' "$payload/share/applications/wallpicker.desktop" > "$root/usr/share/applications/wallpicker.desktop"
# Minimum GNU libc symbol version actually referenced by this executable.
required_glibc=$(readelf --version-info "$binary" | sed -n 's/.*Name: GLIBC_\([0-9][0-9.]*\).*/\1/p' | sort -V | tail -n 1)
[ -n "$required_glibc" ] || { printf '%s\n' 'Could not determine a GNU libc ABI floor; refusing an incomplete Depends field.' >&2; exit 1; }
abi_dependencies="libc6 (>= $required_glibc)"
maintainer=${DEB_MAINTAINER:-wallpicker contributors}
case "$maintainer" in ''|*'
'*|*'
'*) printf '%s\n' 'DEB_MAINTAINER must be a nonempty single line.' >&2; exit 2 ;; esac
if [ -f /etc/debian_version ] && [ -s /var/lib/dpkg/status ]; then
    command -v dpkg-shlibdeps >/dev/null || { printf '%s\n' 'Debian builders need dpkg-shlibdeps (dpkg-dev).' >&2; exit 1; }
    mkdir -p "$stage/debian-analysis/debian"
    printf 'Source: wallpicker\nSection: utils\nPriority: optional\nMaintainer: %s\n\nPackage: wallpicker\nArchitecture: any\nDescription: Wallpaper picker\n' "$maintainer" > "$stage/debian-analysis/debian/control"
    detected=$(cd "$stage/debian-analysis" && dpkg-shlibdeps -O -e "$binary")
    linked_dependencies=$(printf '%s\n' "$detected" | sed -n 's/^shlibs:Depends=//p')
    [ -n "$linked_dependencies" ] || { printf '%s\n' 'dpkg-shlibdeps returned no dependencies.' >&2; exit 1; }
    abi_dependencies="$abi_dependencies, $linked_dependencies"
    compatibility='Built on a Debian-family host; test on the intended release.'
else
    # Non-Debian builders lack Debian symbols/shlibs databases. Map only known
    # native SONAMEs, and refuse new libraries rather than silently omit them.
    needed=$(readelf -d "$binary" | sed -n 's/.*Shared library: \[\([^]]*\)\].*/\1/p')
    for library in $needed; do
        case "$library" in
            libc.so.6|libm.so.6|libdl.so.2|libpthread.so.0|librt.so.1|ld-linux-x86-64.so.2|ld-linux-aarch64.so.1) ;;
            libgcc_s.so.1) abi_dependencies="$abi_dependencies, libgcc-s1" ;;
            *) printf 'No reliable Debian mapping for %s; rebuild on the target Debian distribution.\n' "$library" >&2; exit 1 ;;
        esac
    done
    compatibility='Host-native non-Debian build; NOT verified Debian/Ubuntu compatible.'
    printf 'Warning: %s Requires GNU libc >= %s.\n' "$compatibility" "$required_glibc" >&2
fi
# Iced loads these at runtime, so ELF NEEDED analysis alone is insufficient.
runtime_dependencies='libxkbcommon0, libxkbcommon-x11-0, libwayland-client0, libx11-6, libx11-xcb1, libxcb1, libxcursor1, libxi6, libxrandr2'
size=$(du -sk "$root/usr" | awk '{print $1}')
cat > "$root/DEBIAN/control" <<CONTROL
Package: wallpicker
Version: $version
Section: utils
Priority: optional
Architecture: $architecture
Maintainer: $maintainer
Installed-Size: $size
Depends: $abi_dependencies, $runtime_dependencies
Recommends: swaybg, libvulkan1, libegl1, libgl1
Suggests: libnotify-bin, zenity, ffmpeg, mpvpaper
Description: Wallpaper picker for Wayland and Hyprland
 Browse images, favorites and categories; apply with external wallpaper backends.
 $compatibility
 Requires a graphical session and a separately installed supported backend.
 The optional user rotation unit is installed but never enabled automatically.
CONTROL
dpkg-deb --root-owner-group --build "$root" "$stage/wallpicker_${version}_${architecture}.deb"

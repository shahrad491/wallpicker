#!/bin/sh
# Explicit user-run installation, including backups before replacement.
set -eu
script_dir=$(CDPATH= cd "$(dirname "$0")" && pwd)
if [ -d "$script_dir/bin" ]; then project_root=$script_dir; else project_root=$(CDPATH= cd "$script_dir/.." && pwd); fi
prefix=${PREFIX:-${HOME:?HOME is unset}/.local}
destdir=${DESTDIR:-}
if [ -f "$project_root/Cargo.toml" ]; then
    target_dir=${CARGO_TARGET_DIR:-$project_root/target}
    case "$target_dir" in /*) ;; *) target_dir=$project_root/$target_dir ;; esac
    binary=$target_dir/release/wallpicker
    desktop=$project_root/assets/wallpicker.desktop
    icon=$project_root/assets/wallpicker.svg
    service=$project_root/assets/wallpicker-rotation.service.in
    doc_root=$project_root
else
    binary=$project_root/bin/wallpicker
    desktop=$project_root/share/applications/wallpicker.desktop
    icon=$project_root/share/icons/hicolor/scalable/apps/wallpicker.svg
    service=$project_root/share/wallpicker/wallpicker-rotation.service.in
    doc_root=$project_root/share/doc/wallpicker
fi
while [ "$#" -gt 0 ]; do
    case "$1" in
        --prefix|--destdir|--binary)
            option=$1
            [ "$#" -ge 2 ] || { printf 'Missing value for %s\n' "$option" >&2; exit 2; }
            case "$option" in --prefix) prefix=$2 ;; --destdir) destdir=$2 ;; --binary) binary=$2 ;; esac
            shift 2 ;;
        *) printf 'Usage: %s [--prefix ABSOLUTE_PATH] [--destdir ABSOLUTE_PATH] [--binary PATH]\n' "$0" >&2; exit 2 ;;
    esac
done
case "$prefix" in /*) ;; *) printf '%s\n' 'PREFIX must be absolute.' >&2; exit 2 ;; esac
# Quoted desktop/service executable paths support spaces. Reject characters that
# would require ambiguous layered desktop/systemd escaping rather than misrender.
case "$prefix" in *'"'*|*'\'*|*'`'*|*'$'*|*'
'*) printf '%s\n' 'PREFIX contains an unsupported quote, escape, or newline.' >&2; exit 2 ;; esac
case "$destdir" in ''|/*) ;; *) printf '%s\n' 'DESTDIR must be empty or absolute.' >&2; exit 2 ;; esac
[ -f "$binary" ] && [ -x "$binary" ] || { printf 'Executable not found: %s\n' "$binary" >&2; exit 1; }
install_root=$destdir$prefix
stamp=$(date +%Y%m%d-%H%M%S)
backup_if_present() {
    destination=$1
    if [ -e "$destination" ] || [ -L "$destination" ]; then
        backup=$destination.bak-$stamp
        suffix=0
        while [ -e "$backup" ] || [ -L "$backup" ]; do
            suffix=$((suffix + 1))
            backup=$destination.bak-$stamp-$suffix
        done
        cp -pP "$destination" "$backup"
        printf 'Backup: %s\n' "$backup"
    fi
}
# Finish both backups before either executable/launcher is replaced.
backup_if_present "$install_root/bin/wallpicker"
backup_if_present "$install_root/share/applications/wallpicker.desktop"
backup_if_present "$install_root/share/icons/hicolor/scalable/apps/wallpicker.svg"
backup_if_present "$install_root/share/systemd/user/wallpicker-rotation.service"
install -Dm755 "$binary" "$install_root/bin/wallpicker"
install -d "$install_root/share/applications" "$install_root/share/systemd/user"
executable=$(printf '%s' "$prefix/bin/wallpicker" | sed 's/%/%%/g; s/[&|]/\\&/g')
sed "s|^Exec=.*|Exec=\"$executable\"|" "$desktop" > "$install_root/share/applications/wallpicker.desktop"
chmod 644 "$install_root/share/applications/wallpicker.desktop"
sed "s|@EXECUTABLE@|$executable|g" "$service" > "$install_root/share/systemd/user/wallpicker-rotation.service"
chmod 644 "$install_root/share/systemd/user/wallpicker-rotation.service"
install -Dm644 "$icon" "$install_root/share/icons/hicolor/scalable/apps/wallpicker.svg"
install -d "$install_root/share/doc/wallpicker/docs"
install -m644 "$doc_root/README.md" "$doc_root/ROADMAP.md" "$doc_root/AGENTS.md" "$install_root/share/doc/wallpicker/"
install -m644 "$doc_root"/docs/*.md "$install_root/share/doc/wallpicker/docs/"
printf 'Installed wallpicker under %s\n' "$install_root"

printf '%s\n' 'Rotation service installed, not enabled. See docs/rotation.md before enabling it.'
case ":$PATH:" in *":$prefix/bin:"*) ;; *)
    printf '\nAdd the binary directory to your shell (choose one):\n'
    printf '  bash/zsh: export PATH="%s/bin:$PATH"\n' "$prefix"
    printf '  fish:     fish_add_path "%s/bin"\n' "$prefix"
    printf 'The desktop launcher already uses an absolute executable path.\n'
    ;;
esac

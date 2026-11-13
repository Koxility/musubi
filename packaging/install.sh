#!/bin/sh
# Installs Musubi from an unpacked release archive, or from a source checkout after
# `cargo build --release --locked -p musubi`, and removes it again with --uninstall.
#
#   ./install.sh                         # into ~/.local, no root needed
#   sudo ./install.sh --prefix /usr/local
#   ./install.sh --uninstall             # same --prefix as for the install
#
# PREFIX in the environment works as well, as in earlier versions.
# On Arch and its derivatives, prefer the PKGBUILD in packaging/aur/: pacman then tracks
# and updates the files.
set -eu

usage() {
    cat <<EOF
Usage: install.sh [--prefix DIR] [--uninstall]

  --prefix DIR   Install below DIR (default: \$PREFIX, or ~/.local)
  --uninstall    Remove an installation from the same prefix
  -h, --help     Show this help
EOF
}

prefix=${PREFIX:-"$HOME/.local"}
action=install
while [ $# -gt 0 ]; do
    case "$1" in
        --prefix) prefix=${2:?--prefix needs a directory}; shift 2 ;;
        --prefix=*) prefix=${1#--prefix=}; shift ;;
        --uninstall) action=uninstall; shift ;;
        -h | --help) usage; exit 0 ;;
        *) echo "Unknown option: $1" >&2; usage >&2; exit 2 ;;
    esac
done

here=$(cd "$(dirname "$0")" && pwd)
bin="$prefix/bin"
applications="$prefix/share/applications"
icons="$prefix/share/icons/hicolor"
licenses="$prefix/share/licenses/musubi"

# Desktops cache icons and launcher entries and only look again when told to. Without this,
# an upgrade keeps showing the previous icon and an uninstalled app stays in the launcher.
refresh_desktop() {
    touch "$icons" 2>/dev/null || true
    # Only an existing cache is refreshed: creating one would hide icons that other apps add
    # later without updating it.
    if [ -f "$icons/icon-theme.cache" ] && command -v gtk-update-icon-cache >/dev/null 2>&1; then
        gtk-update-icon-cache -q -t -f "$icons" >/dev/null 2>&1 || true
    fi
    if [ -d "$applications" ] && command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "$applications" >/dev/null 2>&1 || true
    fi
    if command -v kbuildsycoca6 >/dev/null 2>&1; then
        kbuildsycoca6 >/dev/null 2>&1 || true
    fi
}

check_writable() {
    dir=$prefix
    while [ ! -d "$dir" ]; do dir=$(dirname "$dir"); done
    if [ ! -w "$dir" ]; then
        echo "$prefix is not writable. Run again with sudo, or pick another --prefix." >&2
        exit 1
    fi
}

if [ "$action" = uninstall ]; then
    check_writable
    rm -f "$bin/musubi" "$applications/musubi.desktop" "$icons/scalable/apps/musubi.svg"
    rm -rf "$licenses"
    refresh_desktop
    echo "Removed Musubi from $prefix."
    echo "Your settings and data are kept in ~/.config/musubi, ~/.local/share/musubi,"
    # shellcheck disable=SC2088 # a path shown to the user, not one to expand
    echo "~/.cache/musubi and ~/.local/state/musubi; delete them to remove everything."
    exit 0
fi

# A release archive has everything next to this script; a source checkout has the
# binary in target/ and the documents at the repository root.
if [ -f "$here/musubi" ]; then
    binary="$here/musubi"
    docs="$here"
    fonts="$here"
elif [ -f "$here/../target/release/musubi" ]; then
    binary="$here/../target/release/musubi"
    docs="$here/.."
    fonts="$here/../crates/musubi-gui/fonts"
else
    echo "No musubi binary found next to install.sh or in target/release/." >&2
    echo "In a source checkout, run 'cargo build --release --locked -p musubi' first." >&2
    exit 1
fi

check_writable
install -Dm755 "$binary" "$bin/musubi"
install -Dm644 "$here/musubi.svg" "$icons/scalable/apps/musubi.svg"
install -Dm644 "$docs/LICENSE" "$licenses/LICENSE"
# The binary embeds these fonts; their licence asks to ship its text along.
for font in Sora HankenGrotesk; do
    install -Dm644 "$fonts/$font-OFL.txt" "$licenses/$font-OFL.txt"
done

# The launcher entry points at the installed binary directly, so it works even when the
# desktop session's PATH does not include $bin.
mkdir -p "$applications"
sed "s|^Exec=musubi |Exec=$bin/musubi |" "$here/musubi.desktop" > "$applications/musubi.desktop"
chmod 644 "$applications/musubi.desktop"
refresh_desktop

echo "Installed musubi $("$bin/musubi" --version 2>/dev/null | sed 's/^musubi //') to $bin/musubi"
echo "Musubi is now in your application launcher."

# Playback needs GStreamer at runtime; everything else works without it, so this only warns.
# Debian and Ubuntu ship gst-inspect-1.0 separately, so without it only the library is checked.
gstreamer_found() {
    if command -v gst-inspect-1.0 >/dev/null 2>&1; then
        gst-inspect-1.0 playbin >/dev/null 2>&1
    else
        /sbin/ldconfig -p 2>/dev/null | grep -q 'libgstreamer-1\.0\.so\.0'
    fi
}
if ! gstreamer_found; then
    echo
    echo "GStreamer was not found. Musubi starts without it, but cannot play anything."
    echo "Install GStreamer and its plugin sets, see the README."
fi

case ":$PATH:" in
    *":$bin:"*) exit 0 ;;
esac

echo
echo "To run 'musubi' from a terminal, add $bin to your PATH:"
case "$(basename "${SHELL:-}")" in
    fish) echo "    fish_add_path $bin" ;;
    bash) echo "    echo 'export PATH=\"$bin:\$PATH\"' >> ~/.bashrc" ;;
    zsh) echo "    echo 'export PATH=\"$bin:\$PATH\"' >> ~/.zshrc" ;;
    *) echo "    add $bin to PATH in your shell's startup file" ;;
esac
echo "Then open a new terminal."

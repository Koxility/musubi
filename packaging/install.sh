#!/bin/sh
# Installs Musubi from an unpacked release archive into the user's home directory.
# Set PREFIX to install elsewhere, e.g. PREFIX=/usr/local sudo ./install.sh
set -eu

here=$(cd "$(dirname "$0")" && pwd)
prefix=${PREFIX:-"$HOME/.local"}

install -Dm755 "$here/musubi" "$prefix/bin/musubi"
install -Dm644 "$here/musubi.desktop" "$prefix/share/applications/musubi.desktop"
install -Dm644 "$here/musubi.svg" "$prefix/share/icons/hicolor/scalable/apps/musubi.svg"

echo "Installed musubi to $prefix/bin/musubi"
case ":$PATH:" in
    *":$prefix/bin:"*) ;;
    *) echo "Note: $prefix/bin is not on your PATH yet." ;;
esac

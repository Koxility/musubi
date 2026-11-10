#!/bin/sh
# Installs Musubi from an unpacked release archive into the user's home directory.
# Set PREFIX to install elsewhere, e.g. PREFIX=/usr/local sudo ./install.sh
set -eu

here=$(cd "$(dirname "$0")" && pwd)
prefix=${PREFIX:-"$HOME/.local"}
bin="$prefix/bin"
applications="$prefix/share/applications"

install -Dm755 "$here/musubi" "$bin/musubi"
install -Dm644 "$here/musubi.svg" "$prefix/share/icons/hicolor/scalable/apps/musubi.svg"

# The launcher entry points at the installed binary directly, so it works even when the
# desktop session's PATH does not include $bin.
mkdir -p "$applications"
sed "s|^Exec=musubi |Exec=$bin/musubi |" "$here/musubi.desktop" > "$applications/musubi.desktop"
chmod 644 "$applications/musubi.desktop"
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$applications" >/dev/null 2>&1 || true
fi

echo "Installed musubi to $bin/musubi"
echo "Musubi is now in your application launcher."

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

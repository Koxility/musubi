#!/bin/sh
# Packs a release build into dist/ and extracts its release notes from CHANGELOG.md.
#
#   cargo build --release --locked -p musubi
#   packaging/package.sh 0.1.0
#
# Produces dist/musubi-<version>-x86_64-linux.tar.gz, a .sha256 file next to it,
# and dist/notes.md with the CHANGELOG section of that version.
set -eu

version=${1:?usage: packaging/package.sh <version>}
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

crate_version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
if [ "$version" != "$crate_version" ]; then
    echo "Version $version does not match Cargo.toml ($crate_version)" >&2
    exit 1
fi

# Section from "## [<version>]" up to the next heading of the same level, without the
# heading itself and without the link definitions at the end of the file.
notes=$(awk -v v="$version" '
    index($0, "## [" v "]") == 1 { found = 1; next }
    found && /^## \[/ { exit }
    found && /^\[[^]]+\]: / { next }
    found { print }
' CHANGELOG.md)
if [ -z "$(printf '%s' "$notes" | tr -d '[:space:]')" ]; then
    echo "CHANGELOG.md has no section for $version" >&2
    exit 1
fi

name="musubi-$version-x86_64-linux"
rm -rf dist
mkdir -p "dist/$name"
install -m755 target/release/musubi "dist/$name/musubi"
install -m755 packaging/install.sh "dist/$name/install.sh"
install -m644 packaging/musubi.desktop packaging/musubi.svg \
    README.md CHANGELOG.md LICENSE "dist/$name/"
# The binary embeds these fonts; their licence asks to ship its text along.
install -m644 crates/musubi-gui/fonts/Sora-OFL.txt \
    crates/musubi-gui/fonts/HankenGrotesk-OFL.txt "dist/$name/"
tar -C dist -czf "dist/$name.tar.gz" "$name"
(cd dist && sha256sum "$name.tar.gz" > "$name.tar.gz.sha256")
printf '%s\n' "$notes" | sed -e '/./,$!d' > dist/notes.md

echo "dist/$name.tar.gz"

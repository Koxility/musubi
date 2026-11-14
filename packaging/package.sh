#!/bin/sh
# Packs a release build into dist/ and extracts its release notes from CHANGELOG.md.
#
#   cargo build --release --locked -p musubi
#   packaging/package.sh 0.1.0
#
# Produces dist/musubi-<version>-x86_64-linux.tar.gz, a .sha256 file next to it,
# the same archive as dist/musubi-x86_64-linux.tar.gz for the stable
# releases/latest/download/ link in the README, and dist/notes.md with the CHANGELOG
# section of that version.
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

# A PKGBUILD without checksums must not reach a release.
if grep -q "SKIP" packaging/aur/*/PKGBUILD; then
    echo "A PKGBUILD in packaging/aur/ has a checksum set to SKIP" >&2
    exit 1
fi

name="musubi-$version-x86_64-linux"
# Everything is built in a directory of this run and then renamed into dist/. A rename
# within one file system is atomic, so runs at the same time never see or leave a
# half-written archive, in particular not under the version-less name they share.
mkdir -p dist
stage=$(mktemp -d dist/.stage.XXXXXX)
trap 'rm -rf "$stage"' EXIT
mkdir "$stage/$name"
install -m755 target/release/musubi "$stage/$name/musubi"
install -m755 packaging/install.sh "$stage/$name/install.sh"
install -m644 packaging/musubi.desktop packaging/musubi.svg \
    README.md CHANGELOG.md LICENSE "$stage/$name/"
# The binary embeds these fonts; their licence asks to ship its text along.
install -m644 crates/musubi-gui/fonts/*-OFL.txt "$stage/$name/"
tar -C "$stage" -czf "$stage/$name.tar.gz" "$name"
cp "$stage/$name.tar.gz" "$stage/musubi-x86_64-linux.tar.gz"
(
    cd "$stage"
    sha256sum "$name.tar.gz" > "$name.tar.gz.sha256"
    sha256sum musubi-x86_64-linux.tar.gz > musubi-x86_64-linux.tar.gz.sha256
)
printf '%s\n' "$notes" | sed -e '/./,$!d' > "$stage/notes.md"
# One run at a time publishes, so an archive never ends up next to the checksum of
# another run's archive.
exec 9> dist/.lock
flock 9
for file in "$name.tar.gz" "$name.tar.gz.sha256" musubi-x86_64-linux.tar.gz \
    musubi-x86_64-linux.tar.gz.sha256 notes.md; do
    mv -f "$stage/$file" "dist/$file"
done
exec 9>&-

echo "dist/$name.tar.gz"

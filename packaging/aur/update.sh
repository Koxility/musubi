#!/bin/sh
# Moves both PKGBUILDs to a published release: sets pkgver, fills in the checksums of that
# release's source tarball and archive, and regenerates .SRCINFO. Needs makepkg and
# updpkgsums (pacman-contrib).
#
#   packaging/aur/update.sh 0.5.0
#
# The tarball and the archive only exist once the tag is pushed and the release workflow
# has finished. Until then the PKGBUILDs stay on the previous release, with its real
# checksums, so they never carry a disabled check. Commit the result of this script.
set -eu

version=${1:?usage: packaging/aur/update.sh <version>}
here=$(cd "$(dirname "$0")" && pwd)
for pkg in musubi musubi-bin; do
    (
        cd "$here/$pkg"
        sed -i -e "s/^pkgver=.*/pkgver=$version/" -e "s/^pkgrel=.*/pkgrel=1/" PKGBUILD
        updpkgsums
        if grep -q "SKIP" PKGBUILD; then
            echo "$pkg: a checksum is still SKIP" >&2
            exit 1
        fi
        makepkg --printsrcinfo > .SRCINFO
        # Downloaded sources stay out of the repository.
        rm -f -- *.tar.gz
    )
    echo "Updated $pkg: $(sed -n 's/^\tsha256sums = //p' "$here/$pkg/.SRCINFO")"
done

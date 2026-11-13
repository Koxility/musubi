#!/bin/sh
# Fills in the checksums of both PKGBUILDs and regenerates their .SRCINFO, once the release
# for their pkgver is published on GitHub. Needs makepkg and updpkgsums (pacman-contrib).
#
#   packaging/aur/update.sh
#
# The source tarball and the release archive only exist after the tag is pushed, so the
# PKGBUILDs in the tagged commit carry 'SKIP'; commit the result of this script afterwards.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
for pkg in musubi musubi-bin; do
    (
        cd "$here/$pkg"
        updpkgsums
        makepkg --printsrcinfo > .SRCINFO
        # Downloaded sources stay out of the repository.
        rm -f -- *.tar.gz
    )
    echo "Updated $pkg: $(sed -n 's/^\tsha256sums = //p' "$here/$pkg/.SRCINFO")"
done

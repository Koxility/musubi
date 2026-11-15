# Maintainer: Koxility <https://github.com/Koxility>
#
# Builds Musubi from this checkout, including any local changes:
#
#   git clone https://github.com/Koxility/musubi.git && cd musubi && makepkg -si
#
# There is no source array: every step works in $startdir, the repository itself.

pkgname=musubi
pkgver=0.5.0
pkgrel=1
pkgdesc='Stream anime from Jellyfin and keep your AniList in sync'
arch=('x86_64')
url='https://github.com/Koxility/musubi'
license=('MIT' 'OFL-1.1')
depends=(
  'gcc-libs'
  'glibc'
  'glib2'
  'gstreamer'
  'gst-plugins-base'
  'gst-plugins-base-libs'
  'gst-plugins-good'
  'gst-plugins-bad'
  'gst-libav'
  'hicolor-icon-theme'
  'libxkbcommon'
  'libxkbcommon-x11'
  'wayland'
)
makedepends=('cargo')
optdepends=(
  'org.freedesktop.secrets: keep login tokens in the desktop keyring (KWallet, GNOME Keyring)'
  'gst-plugin-va: hardware video decoding through VA-API'
  'vulkan-icd-loader: GPU rendering through Vulkan instead of OpenGL'
)
conflicts=('musubi-bin')
# makepkg's LTO flags would also reach the bundled C code (SQLite, aws-lc), a common cause
# of link failures in Rust packages; Cargo already does thin LTO. The release profile
# strips the binary, so a debug package would be empty.
options=('!lto' '!debug')

prepare() {
  cd "$startdir"
  export RUSTUP_TOOLCHAIN=stable
  cargo fetch --locked --target "$(rustc -vV | sed -n 's/host: //p')"
}

build() {
  cd "$startdir"
  export RUSTUP_TOOLCHAIN=stable
  export CARGO_TARGET_DIR="$startdir/target"
  cargo build --frozen --release -p musubi
}

check() {
  cd "$startdir"
  export RUSTUP_TOOLCHAIN=stable
  export CARGO_TARGET_DIR="$startdir/target"
  cargo test --frozen --release --workspace
}

package() {
  cd "$startdir"
  install -Dm755 target/release/musubi -t "$pkgdir/usr/bin/"
  install -Dm644 packaging/musubi.desktop -t "$pkgdir/usr/share/applications/"
  install -Dm644 packaging/musubi.svg -t "$pkgdir/usr/share/icons/hicolor/scalable/apps/"
  install -Dm644 README.md CHANGELOG.md -t "$pkgdir/usr/share/doc/$pkgname/"
  # The binary embeds fonts under the OFL; which ones differs between versions.
  install -Dm644 LICENSE crates/musubi-gui/fonts/*-OFL.txt \
    -t "$pkgdir/usr/share/licenses/$pkgname/"
}

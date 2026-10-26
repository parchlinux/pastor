# Maintainer: Sohrab Behdani <behdanisohrab@gmail.com>
# Maintainer: ParchLinux Team <contact@parchlinux.com>

pkgname=pastor
pkgver=0.3.1
pkgrel=1
pkgdesc="Modern, native software management center for ParchLinux"
arch=('x86_64')
url="https://github.com/parchlinux/pastor"
license=('GPL-3.0-only')
depends=(
    'gtk4'
    'libadwaita'
    'flatpak'
    'appstream'
    'openssl'
)
makedepends=(
    'cargo'
    'git'
    'gettext'
)
optdepends=(
    'snapper: for btrfs system snapshot protection and recovery'
    'btrfs-progs: for btrfs snapshot maintenance'
)
source=("git+https://github.com/parchlinux/pastor.git#branch=main")
sha256sums=('SKIP')

prepare() {
    cd "${srcdir}/${pkgname}"
    export RUSTUP_TOOLCHAIN=stable
    cargo fetch --locked --target "$(rustc -vV | sed -n 's/host: //p')"
}

build() {
    cd "${srcdir}/${pkgname}"
    export RUSTUP_TOOLCHAIN=stable
    export CARGO_TARGET_DIR=target

    # Compile gettext translation catalogs
    if [ -f po/LINGUAS ]; then
        while read -r lang; do
            [ -z "$lang" ] && continue
            if [ -f "po/${lang}.po" ]; then
                msgfmt -c "po/${lang}.po" -o "po/${lang}.mo"
            fi
        done < po/LINGUAS
    fi

    cargo build --frozen --release -p pastor-ui
}

check() {
    cd "${srcdir}/${pkgname}"
    export RUSTUP_TOOLCHAIN=stable
    cargo test --frozen --workspace
}

package() {
    cd "${srcdir}/${pkgname}"

    # Install binary
    install -Dm755 "target/release/pastor-ui" "${pkgdir}/usr/bin/pastor"
    ln -sf /usr/bin/pastor "${pkgdir}/usr/bin/pastor-ui"
    ln -sf /usr/bin/pastor "${pkgdir}/usr/bin/pastor-store"

    # Install translations
    if [ -f po/LINGUAS ]; then
        while read -r lang; do
            [ -z "$lang" ] && continue
            if [ -f "po/${lang}.mo" ]; then
                install -Dm644 "po/${lang}.mo" "${pkgdir}/usr/share/locale/${lang}/LC_MESSAGES/${pkgname}.mo"
            fi
        done < po/LINGUAS
    fi

    # Install desktop entry
    install -Dm644 "data/com.parchlinux.pastor.desktop" \
        "${pkgdir}/usr/share/applications/com.parchlinux.pastor.desktop"

    # Install autostart entry for background indicator
    install -Dm644 "data/com.parchlinux.pastor-autostart.desktop" \
        "${pkgdir}/etc/xdg/autostart/com.parchlinux.pastor.desktop"

    # Install icon
    install -Dm644 "data/icons/com.parchlinux.pastor.svg" \
        "${pkgdir}/usr/share/icons/hicolor/scalable/apps/com.parchlinux.pastor.svg"
    install -Dm644 "data/icons/com.parchlinux.pastor.svg" \
        "${pkgdir}/usr/share/pixmaps/com.parchlinux.pastor.svg"

    # Install license
    install -Dm644 LICENSE "${pkgdir}/usr/share/licenses/${pkgname}/LICENSE"
}

#!/usr/bin/env bash
set -euo pipefail

ARCH="${1:-x86_64}"
VERSION="${2:-0.4.0}"
START_DIR="$(pwd)"

# If running as root (like in Arch Docker containers), ensure a builder user exists
if [ "$(id -u)" -eq 0 ]; then
    if ! id -u builder >/dev/null 2>&1; then
        useradd -m -s /bin/bash builder
        echo "builder ALL=(ALL) NOPASSWD: ALL" >> /etc/sudoers
    fi
    chown -R builder:builder "$START_DIR"
    RUN_PREFIX="sudo -u builder"
else
    RUN_PREFIX=""
fi

$RUN_PREFIX bash -s -- "$ARCH" "$VERSION" "$START_DIR" << 'EOF'
set -euo pipefail

TARGET_ARCH="$1"
TARGET_VER="$2"
ROOT_DIR="$3"

cd "$ROOT_DIR"

cat << PKGEOD > PKGBUILD.packaging
pkgname=pastor
pkgver=${TARGET_VER}
pkgrel=1
pkgdesc="Modern, native software management center for ParchLinux"
arch=('${TARGET_ARCH}')
url="https://github.com/parchlinux/pastor"
license=('GPL-3.0-only')
depends=('gtk4' 'libadwaita' 'flatpak' 'appstream' 'openssl')
optdepends=(
    'snapper: for btrfs system snapshot protection and recovery'
    'btrfs-progs: for btrfs snapshot maintenance'
)
options=(!debug)

package() {
    install -Dm755 "\${startdir}/target/release/pastor-ui" "\${pkgdir}/usr/bin/pastor"
    ln -sf /usr/bin/pastor "\${pkgdir}/usr/bin/pastor-ui"
    ln -sf /usr/bin/pastor "\${pkgdir}/usr/bin/pastor-store"

    if [ -f "\${startdir}/po/LINGUAS" ]; then
        while read -r lang; do
            [ -z "\$lang" ] && continue
            if [ -f "\${startdir}/po/\${lang}.mo" ]; then
                install -Dm644 "\${startdir}/po/\${lang}.mo" "\${pkgdir}/usr/share/locale/\${lang}/LC_MESSAGES/\${pkgname}.mo"
                if [ "\$lang" = "fa" ]; then
                    install -d "\${pkgdir}/usr/share/locale/fa_IR/LC_MESSAGES"
                    ln -sf "../../../locale/fa/LC_MESSAGES/\${pkgname}.mo" \
                        "\${pkgdir}/usr/share/locale/fa_IR/LC_MESSAGES/\${pkgname}.mo"
                fi
            fi
        done < "\${startdir}/po/LINGUAS"
    fi

    install -Dm644 "\${startdir}/data/com.parchlinux.pastor.desktop" \
        "\${pkgdir}/usr/share/applications/com.parchlinux.pastor.desktop"
    install -Dm644 "\${startdir}/data/com.parchlinux.pastor-autostart.desktop" \
        "\${pkgdir}/etc/xdg/autostart/com.parchlinux.pastor.desktop"
    install -Dm644 "\${startdir}/data/icons/com.parchlinux.pastor.svg" \
        "\${pkgdir}/usr/share/icons/hicolor/scalable/apps/com.parchlinux.pastor.svg"
    install -Dm644 "\${startdir}/data/icons/com.parchlinux.pastor.svg" \
        "\${pkgdir}/usr/share/pixmaps/com.parchlinux.pastor.svg"
    install -Dm644 "\${startdir}/LICENSE" "\${pkgdir}/usr/share/licenses/\${pkgname}/LICENSE"
}
PKGEOD

makepkg -f --nodeps -p PKGBUILD.packaging
rm -f PKGBUILD.packaging
EOF

chmod -R a+rw "$START_DIR"

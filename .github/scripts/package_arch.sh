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

# Ensure /etc/makepkg.conf enforces .pkg.tar.zst if writable
if [ -f /etc/makepkg.conf ] && [ -w /etc/makepkg.conf ]; then
    sed -i "s/^PKGEXT=.*/PKGEXT='.pkg.tar.zst'/" /etc/makepkg.conf || true
    sed -i "s|^COMPRESSZST=.*|COMPRESSZST=(zstd -c -T0 --fast -)|" /etc/makepkg.conf || true
elif [ -f /etc/makepkg.conf ] && [ "$(id -u)" -eq 0 ]; then
    sed -i "s/^PKGEXT=.*/PKGEXT='.pkg.tar.zst'/" /etc/makepkg.conf || true
    sed -i "s|^COMPRESSZST=.*|COMPRESSZST=(zstd -c -T0 --fast -)|" /etc/makepkg.conf || true
fi

$RUN_PREFIX bash -s -- "$ARCH" "$VERSION" "$START_DIR" << 'EOF'
set -euo pipefail

TARGET_ARCH="$1"
TARGET_VER="$2"
ROOT_DIR="$3"

cd "$ROOT_DIR"

# Create a temporary makepkg config so we never clobber $HOME/.makepkg.conf on local hosts
TMP_MAKEPKG_CONF="$(mktemp /tmp/pastor-makepkg.XXXXXX.conf)"
if [ -f /etc/makepkg.conf ]; then
    cp /etc/makepkg.conf "$TMP_MAKEPKG_CONF"
fi
cat << MKEOD >> "$TMP_MAKEPKG_CONF"
PKGDEST="$ROOT_DIR"
PKGEXT='.pkg.tar.zst'
COMPRESSZST=(zstd -c -T0 --fast -)
MKEOD

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
    'squashfs-tools: for fast AppImage metadata and icon extraction'
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

# Ensure makepkg puts package into ROOT_DIR regardless of system makepkg.conf
PKGDEST="$ROOT_DIR" PKGEXT='.pkg.tar.zst' makepkg --config "$TMP_MAKEPKG_CONF" -f --nodeps -p PKGBUILD.packaging
rm -f PKGBUILD.packaging "$TMP_MAKEPKG_CONF"

# If makepkg created .pkg.tar.xz (e.g. from system default in Arch Linux ARM), convert it to .pkg.tar.zst
for pkg_xz in $(find "$ROOT_DIR" -maxdepth 2 -name "*.pkg.tar.xz"); do
    echo "Converting $pkg_xz to .pkg.tar.zst..."
    pkg_zst="${pkg_xz%.xz}.zst"
    xz -dc "$pkg_xz" | zstd -T0 -3 -c > "$pkg_zst"
    rm -f "$pkg_xz"
done

# If makepkg created uncompressed .pkg.tar, compress it with zstd
for pkg_tar in $(find "$ROOT_DIR" -maxdepth 2 -name "*.pkg.tar" ! -name "*.pkg.tar.*"); do
    echo "Compressing $pkg_tar to .pkg.tar.zst..."
    zstd -T0 -3 --rm "$pkg_tar" -o "${pkg_tar}.zst"
done

# Ensure package is at the root level if placed in a subdirectory
find "$ROOT_DIR" -maxdepth 2 -name "*.pkg.tar.zst" -exec cp -u {} "$ROOT_DIR/" \; 2>/dev/null || true

if ! ls "$ROOT_DIR"/*.pkg.tar.zst >/dev/null 2>&1; then
    echo "ERROR: No .pkg.tar.zst package found in $ROOT_DIR!"
    ls -la "$ROOT_DIR"
    exit 1
fi

echo "Arch package generated successfully:"
ls -la "$ROOT_DIR"/*.pkg.tar.zst
EOF

chmod -R a+rw "$START_DIR"

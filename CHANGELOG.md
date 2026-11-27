# Changelog

All notable changes to Pastor (Parch Store) are documented in this file.

## [0.4.0] - 2026-11-19

### Added
- **Parch Doctor Health Suite**:
  - Pacman database lock detector with running process diagnostics and safe unlock action.
  - Pacnew configuration file scanner with colorful side-by-side and mobile-friendly diff viewer highlighting additions and deletions.
  - Pacman package cache manager calculating disk reclamation and cleaning obsolete cached archives.
- **Smart Debian (`.deb`) Package Installer & Converter**:
  - Native Rust `.deb` to Arch `.pkg.tar.zst` converter with merged-`/usr` path normalization, `.PKGINFO`/`.MTREE` generation, and live ALPM dependency mapping.
  - Smart alternative discovery that automatically checks Official Repositories, Flatpak, and AUR before converting and recommends native/community packages when available.
  - File manager integration (`application/vnd.debian.binary-package`, `application/x-deb`), CLI file opening (`pastor app.deb`), and "Save as .pkg.tar.zst" export.
- **AppImage Manager & Desktop Integration**:
  - Inspect Type 1 (ISO) and Type 2 (SquashFS/DwarFS) `.AppImage` packages safely, extract `.desktop` metadata and icons, and check Official Repositories, Flatpak, and AUR for native alternatives.
  - Manage installed AppImages in `~/Applications` and `~/.local/share/applications` (including AppImages installed by `AppManager`): launch, update/replace, and uninstall.
  - Registered Parch Store as the default handler for `.deb` and `.AppImage` files.
- **Arabic Language Support**:
  - Full Arabic (`ar`) localization covering all 389 application strings.
  - Automatic right-to-left (RTL) layout mirroring and font handling for Arabic locales.
- **Complete Persian Translations**:
  - Updated Persian (`fa`) translations covering all Parch Doctor diagnostics, diff viewer, Snapper snapshots, and preferences.
- **Embedded Translation Fallback**:
  - Hardened runtime catalog resolution with built-in embedded gettext fallback preventing older system `.mo` files from overriding new feature translations.
- **Package List / Grid View Switcher**:
  - Toggle button in catalog headers allowing users to switch between card grid and compact list views.
- **Rich About Dialog**:
  - Upgraded Libadwaita About dialog with integrated release notes, credits, and direct links to Parch Apps Web, Wiki, Forum, and GitHub.
- **Architecture & CI/CD Support**:
  - Added `aarch64` architecture to `PKGBUILD`.
  - Added GitHub Actions release workflow building automated release tarballs for `x86_64` and `aarch64`.

### Fixed
- **Desktop Application Launcher**:
  - Fixed application launching failure where `gtk-launch` failed on applications with mismatched package names by discovering desktop file IDs.
- **Mobile Responsiveness & Layout Warnings**:
  - Eliminated `AdwOverlaySplitView exceeds AdwApplicationWindow width` layout warnings when resized below 407px.
  - Resolved `gtk-interface-color-scheme` deprecation warnings on modern GTK/Adwaita stacks.
  - Ensured consistent row spacing and card sizing across compact mobile and wide desktop viewports.
- **ALPM Backend Hardening**:
  - Improved error recovery when repository databases or lock files are in an unexpected state.

---

## [0.3.2] - 2026-11-18

### Added
- Link sharing popover with direct web URLs (`apps.parchlinux.com`), Flathub, AUR, and AppStream links.
- Embedded Persian gettext translation catalog fallback.

### Fixed
- Cache cleanup routine removing old and corrupt package caches.
- AppStream descriptions filtered to respect preferred user locales.

---

## [0.3.1] - 2026-11-10

### Added
- Dynamic translation compilation in `PKGBUILD` based on `po/LINGUAS`.
- RTL layout direction support for right-to-left languages.

---

## [0.3.0] - 2026-11-05

### Added
- Native ALPM dependency resolution for AUR builds.
- Transaction queuing for pacman and flatpak operations.
- Keyboard shortcuts cheat sheet dialog (`Ctrl+?`).

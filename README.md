# Pastor

> The modern, native software center for **ParchLinux**.

Pastor is a fast, responsive software center designed specifically for ParchLinux. Built with **Rust**, **GTK4**, and **Libadwaita**, it unifies native package management (ALPM / AUR) with Flatpak into a clean, desktop-integrated experience.

---

## Features

- **Unified Software Discovery**: Browse and search native Arch/Parch repository packages alongside Flathub Flatpaks with seamless deduplication.
- **Multi-Source Switching**: View details for any application and easily switch between native and Flatpak versions directly on the app page.
- **Asynchronous & Non-Blocking**: Responsive background transactions with real-time download and installation progress.
- **Quick Uninstall**: Manage and remove installed software with a single click from the Installed view.
- **Integrated System Snapshots**: Automatic Snapper snapshot protection before and after package changes with one-click recovery.
- **Built-in Downgrade Tool**: Easily roll back packages to previous working versions when needed.
- **Keyboard-Driven**: Instant access with shortcuts like <kbd>Ctrl</kbd>+<kbd>K</kbd> for search and <kbd>F9</kbd> for sidebar toggle.

---

## Installation & Build

### Dependencies

On ParchLinux / Arch Linux:

```bash
sudo pacman -S --needed rust cargo gtk4 libadwaita flatpak appstream openssl btrfs-progs
```

### Build from Source

```bash
git clone https://github.com/parchlinux/pastor.git
cd pastor
cargo build --release -p pastor-ui
```

The resulting binary will be available at `target/release/pastor-ui`.

### Run Development Build

```bash
cargo run -p pastor-ui
```

---

## Keyboard Shortcuts

| Shortcut | Action |
| --- | --- |
| <kbd>Ctrl</kbd> + <kbd>K</kbd> | Focus search |
| <kbd>F9</kbd> / <kbd>Ctrl</kbd> + <kbd>B</kbd> | Toggle sidebar |
| <kbd>Alt</kbd> + <kbd>←</kbd> | Back to previous view |
| <kbd>Ctrl</kbd> + <kbd>Q</kbd> | Quit |

---

## License

GPL-3.0-only © [ParchLinux Team](https://parchlinux.com)

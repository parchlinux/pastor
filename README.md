# Pastor 🕊️
> The modern, native software management center designed for **ParchLinux**.

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)
[![Built with Rust](https://img.shields.io/badge/Built%20with-Rust-orange.svg)](https://www.rust-lang.org/)
[![GTK4 & Libadwaita](https://img.shields.io/badge/GUI-GTK4%20%7C%20Libadwaita-purple.svg)](https://gnome.pages.gitlab.gnome.org/libadwaita/)
[![ParchLinux](https://img.shields.io/badge/OS-ParchLinux-613583.svg)](https://parchlinux.com)

**Pastor** (Parch Store) is a native, high-performance software center tailored specifically for ParchLinux. It combines native system packages (`[world]`, `[void]`, upstream Arch repos, and AUR) with sandboxed Flatpaks into a unified, elegant desktop experience built on **GTK4** and **Libadwaita**.

---

## ✨ Features

- **Unified Multi-Ecosystem Catalog**:
  - Seamlessly searches and discovers software across Parch official repositories (`[world]`, `[void]`), Arch Linux upstream (`core`, `extra`, `multilib`), the Arch User Repository (AUR), and Flathub.
  - Smart deduplication prevents duplicate listings between native packages and Flatpaks in search and explore views.
  - Provable source origin and multi-source switcher on the package details page (switch between native package and sandboxed Flatpak with one click).

- **Modern GNOME Human Interface Guidelines (HIG)**:
  - Clean visual hierarchy with spotlight banners, curated picks, and categorized catalog views.
  - Responsive **Overlay SplitView** sidebar with an integrated collapse/reveal controller and keyboard navigation (<kbd>F9</kbd> / <kbd>Ctrl</kbd>+<kbd>B</kbd>).
  - Quick-action search entry with global <kbd>Ctrl</kbd>+<kbd>K</kbd> accelerator.

- **Non-Blocking Reactive Transactions**:
  - Fully asynchronous package installation, updates, and removal.
  - In-row circular progress rings displaying real-time percentages and download speeds.
  - Instant cancellation support for in-flight transactions.
  - Global status bar and header operations popover for tracking simultaneous background tasks.

- **Installed Software & Update Management**:
  - Dedicated **Installed** view with 1-click **Quick Uninstall** buttons directly in the row.
  - Automated detection and distinct **Update Available** indicators.
  - Multi-package update management with change tracking.

- **Integrated System Protection & Recovery**:
  - Built-in **Snapper snapshot** integration creating pre- and post-installation system recovery points.
  - Interactive snapshot viewer and one-click restoration tool.
  - Integrated **Package Downgrade** view for easily rolling back problematic updates.
  - Repository and mirror speed management.

---

## 🏗️ Architecture

The workspace is organized into clean, modular Rust crates:

```text
pastor/
├── Cargo.toml
├── LICENSE
├── README.md
├── crates/
│   ├── pastor-core/       # Core domain types (Package, PackageId, PackageSource,
│   │                      # PackageCategory, TransactionEvent, SnapshotBackend trait)
│   ├── pastor-flatpak/    # Native libflatpak integration, AppStream XML parser,
│   │                      # Flathub remote querying, runtime & driver support
│   ├── pastor-store/      # Multi-backend aggregator, search deduplication,
│   │                      # priority sorting, and broadcast event coordination
│   ├── pastor-mock/       # In-memory mock backends for unit testing & CI
│   └── pastor-ui/         # GTK4 + Libadwaita frontend application
```

---

## 🚀 Getting Started

### Prerequisites

On ParchLinux / Arch Linux:

```bash
sudo pacman -S --needed \
    rust \
    cargo \
    gtk4 \
    libadwaita \
    flatpak \
    appstream \
    btrfs-progs \
    openssl \
    pkg-config
```

### Build & Run

1. Clone the repository:
   ```bash
   git clone https://github.com/parchlinux/pastor.git
   cd pastor
   ```

2. Run the unit and integration tests:
   ```bash
   cargo test --workspace
   ```

3. Launch Pastor:
   ```bash
   cargo run -p pastor-ui
   ```

4. Build for release:
   ```bash
   cargo build --release -p pastor-ui
   # Binary located at target/release/pastor-ui
   ```

---

## ⌨️ Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| <kbd>Ctrl</kbd> + <kbd>K</kbd> | Focus global search entry |
| <kbd>F9</kbd> or <kbd>Ctrl</kbd> + <kbd>B</kbd> | Toggle navigation sidebar |
| <kbd>Alt</kbd> + <kbd>←</kbd> / <kbd>Backspace</kbd> | Navigate back from details view |
| <kbd>Ctrl</kbd> + <kbd>Q</kbd> | Quit application |

---

## 📜 License

Pastor is licensed under the **GNU General Public License v3.0 (GPL-3.0)**. See the [LICENSE](LICENSE) file for complete terms.

Copyright © 2026 **ParchLinux Team** (<https://parchlinux.com>).

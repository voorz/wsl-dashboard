# WSL Dashboard

<p align="center">
  <img src="assets/logo/logo.png" width="128" height="128" alt="WSL Dashboard Logo" />
</p>

A native Windows desktop application for managing WSL (Windows Subsystem for Linux)
distributions. Built with Rust and Slint, rendered through Skia for a fast and
polished experience.

## Features

- **Instance management** - Start, stop, terminate, unregister, export, clone,
  and migrate distributions between drives from a single dashboard.
- **Installation** - Deploy distributions from the Microsoft Store, GitHub
  releases, local files (RootFS/VHDX), or online mirrors with automatic
  speed testing.
- **USB passthrough** - Bind, attach, and manage USB devices across instances
  through `usbipd-win`, directly from the interface.
- **Networking** - Configure port forwarding with automatic firewall rules and
  a global HTTP proxy for unified connectivity.
- **Scheduler** - Run recurring maintenance tasks across your instances on a
  cron-like schedule.
- **Low memory footprint** - Stands by in the system tray using roughly 10 MB
  of RAM, with full context-menu and double-click-toggle support.
- **Native integrations** - One-click launch into Terminal, VS Code, or File
  Explorer, with configurable working directories and startup scripts.
- **Theming** - Light and dark mode with a grayscale-first visual design.

## System Requirements

- Windows 10 or 11 with WSL enabled (WSL 2 recommended)
- At least one WSL distribution installed
- 64-bit CPU, 4 GB RAM or more recommended

## Installation

### winget

```powershell
winget install Owu.WSLDashboard
```

### Prebuilt binary

Download the latest release from the
[Releases](https://github.com/voorz/wsl-dashboard/releases) page. The portable
build is a single executable; no installation required.

### Build from source

Requires the Rust toolchain (`x86_64-pc-windows-msvc`):

```powershell
git clone https://github.com/voorz/wsl-dashboard.git
cd wsl-dashboard
cargo build --release
```

## Tech Stack

| Component    | Technology       |
| ------------ | ---------------- |
| Core         | Rust             |
| UI           | Slint + Skia     |
| Async        | Tokio            |
| Platform     | Windows (Win32)  |

## License

[GPL-3.0](LICENSE)
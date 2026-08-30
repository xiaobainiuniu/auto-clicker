# Auto Clicker

<p align="center">
  <strong>English</strong> | <a href="README.zh-CN.md">简体中文</a>
</p>

[![Rust](https://img.shields.io/badge/Rust-1.75+-orange)](https://www.rust-lang.org)
[![Platform](https://img.shields.io/badge/Platform-Windows-blue)](https://github.com)
[![License: MIT](https://img.shields.io/badge/License-MIT-green)](LICENSE)

A lightweight Windows auto clicker that lets you pick any point on the screen and click it repeatedly at a configurable interval.

It supports **multi-monitor point selection**, **automatic stop countdown**, **global hotkeys**, and **background clicking without moving the cursor**. The app runs in the system tray and is distributed as a single executable with no runtime dependencies.

| Main Window | Full-screen Picker | Running |
|:---:|:---:|:---:|
| ![Main Window](docs/images/main.png) | ![Full-screen Picker](docs/images/picker.png) | ![Running](docs/images/running.png) |

## ✨ Features

- **Full-screen crosshair picker (`F2`)** — Covers the entire desktop with a crosshair and a 3× live magnifier for pixel-accurate point selection. The background uses a live desktop snapshot, so what you see is what you select.
- **Multi-monitor support** — The picker spans all monitors and handles mixed-DPI setups correctly, including secondary displays positioned in any direction.
- **Two click modes**:
  - **Universal mode** — Uses `SendInput`; works with most applications, but the mouse cursor briefly moves to the target position for each click.
  - **Background mode** — Uses `PostMessage`; the **cursor stays completely still**, so you can keep using the computer while clicks are sent to ordinary windows such as browsers.
- **Precise interval control** — Enter any interval from 0.001 to 60 seconds (default: 0.1 seconds).
- **Stop control** — Set an automatic stop timer from 10 seconds to 10 hours, use `0` for unlimited duration, or stop manually with `F6` at any time.
- **Global hotkeys** — `F2` to pick a point and `F6` to start/stop, even when the app is minimized to the tray.
- **System tray integration** — Left-click to restore the window; right-click for start/stop, point selection, click mode, interval, duration, always-on-top, and exit controls. Closing the window minimizes it to the tray instead of terminating active clicking.
- **Single-instance behavior** — Launching the executable again brings the existing window to the foreground instead of opening another process.
- **Runtime parameter locking** — Settings are disabled while clicking is active to prevent changes that would not take effect until the next run.
- **Persistent settings** — Position, interval, duration, and click mode are restored on the next launch.
- **Always-on-top option** plus real-time click count and remaining-time display.

## 🚀 Quick Start

### Option 1: Download the executable

Download `AutoClicker.exe` from the [Releases](../../releases) page and run it directly. No installation is required.

### Option 2: Build from source

Requires [Rust](https://rustup.rs).

```bash
cargo build --release
# Output: target\release\AutoClicker.exe
```

## 📖 Usage

1. Press `F2` (or click **Select Position**) → move the cursor to the target → **left-click to confirm** (`Esc` to cancel).
2. Configure the click interval and optional countdown timer.
3. Press `F6` to start clicking. Press `F6` again to stop.

| Hotkey | Action |
|---|---|
| `F2` | Open the full-screen point picker |
| `F6` | Start / stop auto clicking |

> 💡 For ordinary windows such as browsers, try **Background mode** if you want the mouse cursor to remain completely still while clicking continues.

## 🖱️ Click Modes

| Mode | Implementation | Cursor behavior | Best for |
|---|---|---|---|
| Universal (default) | `SendInput` | Briefly moves to the target | General compatibility, including some games |
| Background | `PostMessage` | **Does not move** | Browsers and ordinary desktop windows |

## ❓ FAQ

- **Windows SmartScreen warning** — The executable is not code-signed. On first launch, choose **More info → Run anyway** if you trust the downloaded release.
- **F2 / F6 does not work** — Another application may already be using the same global hotkey. Close the conflicting app and restart Auto Clicker.
- **Background mode does not click the target** — The target application may not accept message-based clicks. Switch to Universal mode.
- **Point selection is incorrect on a secondary monitor** — Multi-monitor setups are supported natively. If coordinates still look wrong, verify your Windows display scaling settings and select the point again.

## 🛠️ Development

```bash
cargo run                 # Run in debug mode
cargo build --release     # Build release executable
python tools/gen_icon.py  # Regenerate the icon (requires Pillow)
```

Tech stack: [Rust](https://www.rust-lang.org) + [egui/eframe](https://github.com/emilk/egui) + WinAPI (`SendInput` / `PostMessage` / `Shell_NotifyIcon` / `RegisterHotKey`).

## 🤝 Contributing

Contributions and feedback are welcome.

- 🐛 Found a bug or have a feature request? → [Open an issue](../../issues)
- 💡 Have an idea or usage question? → Start a discussion in Issues
- ⭐ If the project is useful to you, consider giving it a Star

## 📄 License

[MIT](LICENSE)

## ⚠️ Disclaimer

This project is intended for learning, research, and legitimate automation use cases such as testing and accessibility assistance. Please follow the terms of service of any software you interact with and comply with applicable laws and regulations. You are responsible for how you use this tool.

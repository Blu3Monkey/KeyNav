# KeyNav

Vimium-style keyboard hint navigation for Windows. Press a global hotkey, type letter hints overlaid on UI controls, and activate them without touching the mouse.

## How it works

1. KeyNav runs in the system tray.
2. Focus any window and press **Ctrl+Shift+Space** (default).
3. Letter badges appear on actionable controls (via UI Automation).
4. Type a hint to invoke the control. **Esc** cancels. **Backspace** narrows.

## Quick start (from source)

Requires [Rust](https://rustup.rs/) with the MSVC toolchain on Windows 10+.

```powershell
git clone <your-repo-url>
cd KeyNav
cargo build --release
cargo test
.\target\release\keynav.exe
```

## Install

### Installer (recommended)

Build the release and installer (requires [Inno Setup 6](https://jrsoftware.org/isinfo.php)):

```powershell
cd KeyNav
.\scripts\build-release.ps1
```

Run `dist\KeyNav-Setup.exe`. The installer can add a **Start at login** entry under `HKCU\...\Run`.

### Manual / development

```powershell
cargo build --release
.\target\release\keynav.exe
```

Config is created at `%APPDATA%\keynav\config.toml` on first run.

## Settings

Right-click the tray icon and choose **Settings…** (native Win32 dialog on the main thread — no freeze on Save).

| Section | Options |
|---------|---------|
| **Activation hotkey** | HotKey control + Win checkbox |
| **Hint letters** | Custom alphabet, Home row / QWERTY left presets |
| **Appearance** | Font size slider, badge colors, max elements |

Changes apply when you click **Save**. If `RegisterHotKey` fails (chord taken by another app), settings still save and the keyboard-hook fallback remains active (you'll see a warning).

Open settings from a shortcut with:

```powershell
keynav.exe --settings
```

Tray menu: **Hint now**, **Settings…**, **Start at login** (toggle), **Open config folder**, **Quit**.

## Configuration file

Advanced users can edit `%APPDATA%\keynav\config.toml`. See [config.example.toml](config.example.toml).

## Architecture

```
Global hotkey → UIA scan (foreground window) → hint assignment → layered overlay
     → keyboard hook (type hint) → InvokePattern / click fallback
```

Settings and hint engine share the main-thread message loop — config apply is synchronous.

## Known limitations

| App type | Expected behavior |
|----------|-------------------|
| Win32 / WPF / WinForms | Good |
| Chromium (Chrome, Edge, Electron) | Good for standard controls; poor in canvas/WebGL |
| Games, fullscreen DirectX | No UIA — hints won't appear |
| Elevated apps | Won't work unless KeyNav is also elevated |
| UWP / some modern apps | Mixed; click fallback may help |

## License

MIT

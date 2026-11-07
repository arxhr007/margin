# Margin

Turn the edges and corners of your Windows touchpad into controls — and choose what each one does.

Open the settings, click a section on the touchpad map, pick an action. That's it.

| Section | You can assign |
| --- | --- |
| **Left / right / top / bottom edge** (slide along it) | Volume · Screen brightness · Seek video/audio · Zoom · Switch tabs / windows (Alt+Tab) · Previous/next track · Switch virtual desktops · **Custom keys** (record any shortcut for each direction) · Nothing |
| **Four corners** (1, 2 or 3 clicks) | Mute · Play/pause · Next/previous track · Screenshot · Task View · Show desktop · Lock · Notifications · Open an app · Open a website · Press a shortcut · Run a command · Nothing |

Defaults: left = volume, right = brightness, top = switch tabs, bottom = switch virtual desktops. Corners: top-left = show desktop, top-right = Task View (1 click) / notifications (2), bottom-left = lock, bottom-right = play/pause (1) / next (2) / previous (3). Slide-in is the default start mode.

## Two ways to start a slide

Pick one in settings:

- **Press and slide** — rest a finger on an edge, press until the touchpad clicks, slide without letting go. Hardest to trigger by accident.
- **Slide in from the edge** — bring a finger in from the laptop body onto the edge, then move along it. No click. A finger that first lands inside the touchpad, or any two-finger contact, never triggers.

Corners always need a real press; taps are ignored. Everything outside the assigned sections is a normal touchpad.

While sliding, the pointer stays still and nothing is clicked underneath. Hold **Ctrl + Shift** for fine steps.

## Requirements

- Windows 10 or 11 with a **precision touchpad** (Settings → Bluetooth & devices → Touchpad says "Your PC has a precision touchpad")
- Nothing else to install: Margin is a single native `margin.exe` (about 0.6 MB)
- Brightness works on the built-in screen (external monitors aren't supported)

## Build and run

Margin is written in Rust (native Win32, no UI framework). You need the Rust toolchain for the MSVC target and the "Desktop development with C++" build tools (they include the Windows SDK).

```powershell
cargo build --release      # target\release\margin.exe
cargo test                 # unit tests: settings, geometry, shortcut parsing, the gesture engine
```

Run `margin.exe`. Settings open on first launch; afterwards click the tray icon. Right-click the tray icon to turn it off or quit. Launching Margin a second time opens the settings of the copy that is already running.

`margin.exe --diag` opens a console that prints every touch frame, the button state and each gesture — handy on a new touchpad.

Settings are stored in `%APPDATA%\Margin\settings.json`.

Tests that touch real hardware or the registry are marked `#[ignore]` and run on request:

```powershell
cargo test brightness_write -- --ignored --nocapture   # nudges the panel by 1 %, reads it back, restores it
cargo test startup_round_trip -- --ignored             # writes and removes the "start with Windows" value
```

## How it works

- **Touch input** — Raw Input for HID digitizer usage page `0x0D` / touchpad `0x05`, parsed with `HidP_*` (contact ID, tip switch, X/Y, contact count, button). Works in the background.
- **Hiding clicks, freezing the pointer** — a `WH_MOUSE_LL` low-level mouse hook.
- **Volume, media, shortcuts** — `SendInput` (the native Windows volume flyout appears).
- **Brightness** — WMI `WmiMonitorBrightnessMethods`, with its own small on-screen indicator.

No network access, no telemetry.

## Source layout

Everything runs on the UI thread (raw input, the mouse hook and the windows share one message loop). The gesture logic has no Win32 calls in it, so it is unit-tested without a touchpad.

| Module | Role |
| --- | --- |
| `settings.rs` | settings model and JSON (comments and trailing commas accepted) |
| `geometry.rs` | where each edge and corner sits on the pad |
| `engine.rs` | the gesture state machine; turns touch frames and clicks into effects (pure, tested) |
| `touchpad/` | Raw Input + HID parsing; fingers and button state |
| `hook.rs` | low-level mouse hook: hides edge clicks, freezes the pointer during a slide |
| `keys.rs`, `actions.rs` | `SendInput` key presses, shortcut parsing, corner click actions |
| `brightness.rs` | built-in panel brightness through WMI, on a worker thread |
| `hud.rs`, `tray.rs`, `icon_art.rs` | HUD overlay, notification-area icon and menu, the icon artwork |
| `app.rs` | owns the state and the message loop; runs the engine's effects after the hook returns |
| `ui/` | the settings window: native controls, touchpad map, shortcut recorder |

`build.rs` embeds the manifest (per-monitor DPI, visual styles), version info and an icon generated from the same drawing code the tray uses.

## History

Versions up to 0.2.1 were written in C# / .NET 8 (WinForms). That code is still in the git history and in the [v0.2.1 tag](../../tree/v0.2.1). The Rust version reads and writes the same `settings.json`, so existing settings carry over.

## License

MIT — see [LICENSE](LICENSE).

# Margin

Turn the edges and corners of your Windows touchpad into controls — and choose what each one does.

Open the settings, click a section on the touchpad map, pick an action. That's it.

| Section | You can assign |
| --- | --- |
| **Left / right / top / bottom edge** (slide along it) | Volume · Screen brightness · Seek video/audio · Zoom · Switch tabs · Previous/next track · Switch virtual desktops · Nothing |
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
- .NET 8 Desktop Runtime, x64
- Brightness works on the built-in screen (external monitors aren't supported)

## Build and run

```powershell
dotnet build -c Release
# single exe:
dotnet publish src/Margin/Margin.csproj -c Release -p:PublishSingleFile=true -o publish
```

Run `Margin.exe`. Settings open on first launch; afterwards click the tray icon. Right-click the tray icon to turn it off or quit.

`Margin.exe --diag` opens a console that prints every touch frame, the button state and each gesture — handy on a new touchpad.

Settings are stored in `%APPDATA%\Margin\settings.json`.

## How it works

- **Touch input** — Raw Input for HID digitizer usage page `0x0D` / touchpad `0x05`, parsed with `HidP_*` (contact ID, tip switch, X/Y, contact count, button). Works in the background.
- **Hiding clicks, freezing the pointer** — a `WH_MOUSE_LL` low-level mouse hook.
- **Volume, media, shortcuts** — `SendInput` (the native Windows volume flyout appears).
- **Brightness** — WMI `WmiMonitorBrightnessMethods`, with its own small on-screen indicator.

No network access, no telemetry.

## License

MIT — see [LICENSE](LICENSE).

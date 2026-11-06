# Margin

Turn the edges of your Windows touchpad into controls — and choose what each one does.

Open the settings, click an edge on the touchpad map, pick an action. That's it.

| Section | You can assign |
| --- | --- |
| **Left / right / top / bottom edge** (slide along it) | Volume · Screen brightness · Seek video/audio · Zoom · Switch tabs · Previous/next track · Switch virtual desktops · Nothing |

Defaults: left = brightness, right = volume, bottom = seek.

## Two ways to start a slide

Pick one in settings:

- **Press and slide** — rest a finger on an edge, press until the touchpad clicks, slide without letting go. Hardest to trigger by accident.
- **Slide in from the edge** — bring a finger in from the laptop body onto the edge, then move along it. No click. A finger that first lands inside the touchpad, or any two-finger contact, never triggers.

Everything outside the assigned edges is a normal touchpad.

While sliding, the pointer stays still and nothing is clicked underneath.

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

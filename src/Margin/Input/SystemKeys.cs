using System.Runtime.InteropServices;

namespace Margin.Input;

/// Presses keys through SendInput, exactly like a keyboard would.
internal static class SystemKeys
{
    public const ushort VolumeMute = 0xAD, VolumeDown = 0xAE, VolumeUp = 0xAF;
    public const ushort MediaPlayPause = 0xB3, MediaNext = 0xB0, MediaPrev = 0xB1;
    public const ushort Left = 0x25, Right = 0x27, Tab = 0x09;
    public const ushort Ctrl = 0xA2, Shift = 0xA0, Win = 0x5B, Alt = 0xA4;

    /// Ctrl + mouse wheel, which zooms in browsers, Office, Explorer and most viewers.
    public static void CtrlWheel(int notches)
    {
        if (notches == 0) return;
        var inputs = new[]
        {
            Key(Ctrl, up: false),
            new Native.INPUT
            {
                Type = 0, // INPUT_MOUSE
                U = new Native.InputUnion
                {
                    Mi = new Native.MOUSEINPUT { MouseData = unchecked((uint)(notches * 120)), Flags = 0x0800 /* WHEEL */ },
                },
            },
            Key(Ctrl, up: true),
        };
        Native.SendInput((uint)inputs.Length, inputs, Marshal.SizeOf<Native.INPUT>());
    }

    // Keys that carry the E0 prefix on a real keyboard.
    private static readonly HashSet<ushort> Extended = new()
    {
        0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, // page up/down, end, home, arrows
        0x2C, 0x2D, 0x2E,                               // print screen, insert, delete
        0x5B, 0x5C, 0x5D,                               // win keys, apps
        0xA3, 0xA5,                                     // right ctrl / alt
        0xAD, 0xAE, 0xAF, 0xB0, 0xB1, 0xB2, 0xB3,       // volume + media
    };

    /// Presses or releases a single key and leaves it that way (used to hold Alt through an Alt+Tab slide).
    public static void Hold(ushort vk, bool up)
    {
        var array = new[] { Key(vk, up) };
        if (Native.SendInput(1, array, Marshal.SizeOf<Native.INPUT>()) != 1)
            Log.Error($"SendInput failed: {Marshal.GetLastWin32Error()}");
    }

    public static void Tap(ushort vk, int times = 1) => Combo(vk, Array.Empty<ushort>(), times);

    /// Holds the modifiers, taps the key `times` times, releases the modifiers.
    public static void Combo(ushort vk, IReadOnlyList<ushort> modifiers, int times = 1)
    {
        var inputs = new List<Native.INPUT>();
        foreach (var m in modifiers) inputs.Add(Key(m, up: false));
        for (int i = 0; i < times; i++)
        {
            inputs.Add(Key(vk, up: false));
            inputs.Add(Key(vk, up: true));
        }
        for (int i = modifiers.Count - 1; i >= 0; i--) inputs.Add(Key(modifiers[i], up: true));
        var array = inputs.ToArray();
        if (Native.SendInput((uint)array.Length, array, Marshal.SizeOf<Native.INPUT>()) != array.Length)
            Log.Error($"SendInput failed: {Marshal.GetLastWin32Error()}");
    }

    private static Native.INPUT Key(ushort vk, bool up)
    {
        uint flags = up ? Native.KEYEVENTF_KEYUP : 0;
        if (Extended.Contains(vk)) flags |= Native.KEYEVENTF_EXTENDEDKEY;
        return new Native.INPUT
        {
            Type = Native.INPUT_KEYBOARD,
            U = new Native.InputUnion
            {
                Ki = new Native.KEYBDINPUT { Vk = vk, Scan = (ushort)Native.MapVirtualKey(vk, 0), Flags = flags },
            },
        };
    }

    /// Ctrl and Shift both held → fine steps.
    public static bool FineModifiersHeld() =>
        (Native.GetAsyncKeyState(0x11) & 0x8000) != 0 && (Native.GetAsyncKeyState(0x10) & 0x8000) != 0;
}

internal static class KeyMap
{
    private static readonly Dictionary<string, ushort> Codes = Build();

    private static Dictionary<string, ushort> Build()
    {
        var d = new Dictionary<string, ushort>(StringComparer.OrdinalIgnoreCase);
        for (char c = 'a'; c <= 'z'; c++) d[c.ToString()] = (ushort)char.ToUpperInvariant(c);
        for (char c = '0'; c <= '9'; c++) d[c.ToString()] = c;
        for (int i = 1; i <= 24; i++) d[$"f{i}"] = (ushort)(0x6F + i);
        var named = new Dictionary<string, ushort>
        {
            ["-"] = 0xBD, ["="] = 0xBB, ["["] = 0xDB, ["]"] = 0xDD, [";"] = 0xBA, ["'"] = 0xDE,
            [","] = 0xBC, ["."] = 0xBE, ["/"] = 0xBF, ["\\"] = 0xDC, ["`"] = 0xC0,
            ["return"] = 0x0D, ["enter"] = 0x0D, ["tab"] = 0x09, ["space"] = 0x20,
            ["backspace"] = 0x08, ["delete"] = 0x2E, ["del"] = 0x2E, ["insert"] = 0x2D,
            ["escape"] = 0x1B, ["esc"] = 0x1B,
            ["left"] = 0x25, ["up"] = 0x26, ["right"] = 0x27, ["down"] = 0x28,
            ["home"] = 0x24, ["end"] = 0x23, ["pageup"] = 0x21, ["pagedown"] = 0x22,
            ["printscreen"] = 0x2C, ["prtsc"] = 0x2C,
            ["volumeup"] = SystemKeys.VolumeUp, ["volumedown"] = SystemKeys.VolumeDown, ["mute"] = SystemKeys.VolumeMute,
            ["playpause"] = SystemKeys.MediaPlayPause, ["nexttrack"] = 0xB0, ["prevtrack"] = 0xB1,
        };
        foreach (var (k, v) in named) d[k] = v;
        return d;
    }

    /// The name Code() understands for a WinForms key, or null when the key can't be bound.
    public static string? Name(Keys key)
    {
        if (key >= Keys.A && key <= Keys.Z) return key.ToString().ToLowerInvariant();
        if (key >= Keys.D0 && key <= Keys.D9) return ((char)('0' + (key - Keys.D0))).ToString();
        if (key >= Keys.F1 && key <= Keys.F24) return "f" + (key - Keys.F1 + 1);
        return key switch
        {
            Keys.Tab => "tab", Keys.Return => "enter", Keys.Space => "space", Keys.Back => "backspace",
            Keys.Delete => "delete", Keys.Insert => "insert", Keys.Escape => "esc",
            Keys.Left => "left", Keys.Up => "up", Keys.Right => "right", Keys.Down => "down",
            Keys.Home => "home", Keys.End => "end", Keys.PageUp => "pageup", Keys.PageDown => "pagedown",
            Keys.PrintScreen => "printscreen",
            Keys.OemMinus => "-", Keys.Oemplus => "=", Keys.OemOpenBrackets => "[", Keys.OemCloseBrackets => "]",
            Keys.OemSemicolon => ";", Keys.OemQuotes => "'", Keys.Oemcomma => ",", Keys.OemPeriod => ".",
            Keys.OemQuestion => "/", Keys.OemPipe => "\\", Keys.Oemtilde => "`",
            Keys.VolumeUp => "volumeup", Keys.VolumeDown => "volumedown", Keys.VolumeMute => "mute",
            Keys.MediaPlayPause => "playpause", Keys.MediaNextTrack => "nexttrack", Keys.MediaPreviousTrack => "prevtrack",
            _ => null,
        };
    }

    public static ushort? Code(string name) => Codes.TryGetValue(name, out var vk) ? vk : null;

    public static List<ushort> Modifiers(IEnumerable<string> names)
    {
        var list = new List<ushort>();
        foreach (var m in names)
        {
            switch (m.ToLowerInvariant())
            {
                case "ctrl": case "control": case "cmd": case "command": list.Add(0xA2); break; // cmd → ctrl on Windows
                case "shift": list.Add(0xA0); break;
                case "alt": case "opt": case "option": list.Add(0xA4); break;
                case "win": case "windows": case "super": list.Add(0x5B); break;
                default: Log.Error($"unknown modifier {m}"); break;
            }
        }
        return list;
    }
}

using System.Runtime.InteropServices;

namespace Margin.Input;

/// Presses keys through SendInput, exactly like a keyboard would.
internal static class SystemKeys
{
    public const ushort VolumeMute = 0xAD, VolumeDown = 0xAE, VolumeUp = 0xAF;
    public const ushort MediaPlayPause = 0xB3, MediaNext = 0xB0, MediaPrev = 0xB1;
    public const ushort Left = 0x25, Right = 0x27, Tab = 0x09;
    public const ushort Ctrl = 0xA2, Shift = 0xA0, Win = 0x5B;

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
}

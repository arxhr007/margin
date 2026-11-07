using System.Diagnostics;
using System.Runtime.InteropServices;
using Margin.Input;

namespace Margin;

/// Carries out corner click actions.
internal static class ActionRunner
{
    [DllImport("user32.dll")]
    private static extern bool LockWorkStation();

    private const ushort Win = 0x5B, Shift = 0xA0;

    public static void Run(ClickSpec spec)
    {
        try
        {
            switch (spec.Do)
            {
                case ClickAction.Mute: SystemKeys.Tap(SystemKeys.VolumeMute); break;
                case ClickAction.PlayPause: SystemKeys.Tap(SystemKeys.MediaPlayPause); break;
                case ClickAction.NextTrack: SystemKeys.Tap(SystemKeys.MediaNext); break;
                case ClickAction.PrevTrack: SystemKeys.Tap(SystemKeys.MediaPrev); break;
                case ClickAction.Screenshot: SystemKeys.Combo('S', new[] { Win, Shift }); break;
                case ClickAction.TaskView: SystemKeys.Combo(0x09, new[] { Win }); break;
                case ClickAction.ShowDesktop: SystemKeys.Combo('D', new[] { Win }); break;
                case ClickAction.Notifications: SystemKeys.Combo('N', new[] { Win }); break;
                case ClickAction.Lock: LockWorkStation(); break;
                case ClickAction.App: ToggleApp(spec.Value); break;
                case ClickAction.Url: Open(spec.Value); break;
                case ClickAction.Keys: PressShortcut(spec.Value); break;
                case ClickAction.Command: RunCommand(spec.Value); break;
            }
        }
        catch (Exception e)
        {
            Log.Error($"{spec.Do} failed: {e.Message}");
        }
    }

    /// "Ctrl+Shift+Esc" → hold Ctrl and Shift, tap Esc.
    public static bool TryParseShortcut(string? text, out ushort key, out List<ushort> modifiers)
    {
        key = 0;
        modifiers = new();
        if (string.IsNullOrWhiteSpace(text)) return false;
        var parts = text.Split('+', StringSplitOptions.TrimEntries | StringSplitOptions.RemoveEmptyEntries);
        if (parts.Length == 0) return false;
        foreach (var m in parts[..^1])
        {
            var mods = KeyMap.Modifiers(new[] { m });
            if (mods.Count == 0) return false;
            modifiers.AddRange(mods);
        }
        if (KeyMap.Code(parts[^1]) is not { } vk) return false;
        key = vk;
        return true;
    }

    private static void PressShortcut(string? text)
    {
        if (TryParseShortcut(text, out var key, out var mods)) SystemKeys.Combo(key, mods);
        else Log.Error($"can't understand shortcut \"{text}\"");
    }

    /// Opens the app, or minimizes it when it's already the window in front.
    private static void ToggleApp(string? value)
    {
        if (string.IsNullOrWhiteSpace(value)) return;
        var exe = Environment.ExpandEnvironmentVariables(value.Trim().Trim('"'));
        var front = Native.GetForegroundWindow();
        Native.GetWindowThreadProcessId(front, out uint pid);
        try
        {
            using var p = Process.GetProcessById((int)pid);
            if (string.Equals(p.ProcessName, Path.GetFileNameWithoutExtension(exe), StringComparison.OrdinalIgnoreCase))
            {
                Native.ShowWindow(front, Native.SW_MINIMIZE);
                return;
            }
        }
        catch (ArgumentException)
        {
            // the foreground process exited meanwhile
        }
        Process.Start(new ProcessStartInfo(exe) { UseShellExecute = true })?.Dispose();
    }

    private static void Open(string? value)
    {
        if (string.IsNullOrWhiteSpace(value)) return;
        var target = value.Trim();
        if (!target.Contains("://") && !target.Contains('\\') && target.Contains('.')) target = "https://" + target;
        Process.Start(new ProcessStartInfo(Environment.ExpandEnvironmentVariables(target)) { UseShellExecute = true })?.Dispose();
    }

    private static void RunCommand(string? command)
    {
        if (string.IsNullOrWhiteSpace(command)) return;
        Process.Start(new ProcessStartInfo("cmd.exe")
        {
            Arguments = "/c " + command,
            CreateNoWindow = true,
            UseShellExecute = false,
        })?.Dispose();
    }
}

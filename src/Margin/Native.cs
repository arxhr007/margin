using System.Runtime.InteropServices;

namespace Margin;

/// Win32 / HID declarations used across the app.
internal static class Native
{
    // ---- Raw input ----
    public const int WM_INPUT = 0x00FF;
    public const int WM_INPUT_DEVICE_CHANGE = 0x00FE;
    public const uint RIDEV_INPUTSINK = 0x00000100;
    public const uint RIDEV_DEVNOTIFY = 0x00002000;
    public const uint RID_INPUT = 0x10000003;
    public const uint RIDI_PREPARSEDDATA = 0x20000005;
    public const uint RIDI_DEVICENAME = 0x20000007;
    public const uint RIM_TYPEHID = 2;

    [StructLayout(LayoutKind.Sequential)]
    public struct RAWINPUTDEVICE
    {
        public ushort UsagePage;
        public ushort Usage;
        public uint Flags;
        public IntPtr Target;
    }

    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool RegisterRawInputDevices(RAWINPUTDEVICE[] devices, uint count, uint size);

    [DllImport("user32.dll")]
    public static extern uint GetRawInputData(IntPtr rawInput, uint command, IntPtr data, ref uint size, uint headerSize);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern uint GetRawInputDeviceInfo(IntPtr device, uint command, IntPtr data, ref uint size);

    // ---- HID parsing (hid.dll) ----
    public const int HidP_Input = 0;
    public const int HIDP_STATUS_SUCCESS = 0x00110000;
    public const int HidpCapsSize = 64;
    public const int HidpValueCapsSize = 72;
    public const int HidpButtonCapsSize = 72;

    [DllImport("hid.dll")]
    public static extern int HidP_GetCaps(IntPtr preparsed, byte[] caps);

    [DllImport("hid.dll")]
    public static extern int HidP_GetValueCaps(int reportType, byte[] caps, ref ushort length, IntPtr preparsed);

    [DllImport("hid.dll")]
    public static extern int HidP_GetButtonCaps(int reportType, byte[] caps, ref ushort length, IntPtr preparsed);

    [DllImport("hid.dll")]
    public static extern int HidP_GetUsageValue(int reportType, ushort usagePage, ushort linkCollection, ushort usage,
        out uint value, IntPtr preparsed, byte[] report, uint reportLength);

    [DllImport("hid.dll")]
    public static extern int HidP_GetUsages(int reportType, ushort usagePage, ushort linkCollection, ushort[] usages,
        ref uint usageLength, IntPtr preparsed, byte[] report, uint reportLength);

    // ---- Messages ----
    public const uint PM_REMOVE = 1;

    [StructLayout(LayoutKind.Sequential)]
    public struct POINT { public int X, Y; }

    [StructLayout(LayoutKind.Sequential)]
    public struct MSG
    {
        public IntPtr Hwnd;
        public uint Message;
        public IntPtr WParam;
        public IntPtr LParam;
        public uint Time;
        public POINT Pt;
        public uint Private;
    }

    [DllImport("user32.dll")]
    public static extern bool PeekMessage(out MSG msg, IntPtr hwnd, uint filterMin, uint filterMax, uint remove);

    [DllImport("user32.dll")]
    public static extern IntPtr DefWindowProc(IntPtr hwnd, uint msg, IntPtr wParam, IntPtr lParam);

    // ---- Low-level mouse hook ----
    public const int WH_MOUSE_LL = 14;
    public const int WM_MOUSEMOVE = 0x0200;
    public const int WM_LBUTTONDOWN = 0x0201;
    public const int WM_LBUTTONUP = 0x0202;
    public const int WM_RBUTTONDOWN = 0x0204;
    public const int WM_RBUTTONUP = 0x0205;
    public const uint LLMHF_INJECTED = 0x1;

    public delegate IntPtr LowLevelProc(int code, IntPtr wParam, IntPtr lParam);

    [StructLayout(LayoutKind.Sequential)]
    public struct MSLLHOOKSTRUCT
    {
        public POINT Pt;
        public uint MouseData;
        public uint Flags;
        public uint Time;
        public IntPtr ExtraInfo;
    }

    [DllImport("user32.dll", SetLastError = true)]
    public static extern IntPtr SetWindowsHookEx(int idHook, LowLevelProc fn, IntPtr module, uint threadId);

    [DllImport("user32.dll")]
    public static extern bool UnhookWindowsHookEx(IntPtr hook);

    [DllImport("user32.dll")]
    public static extern IntPtr CallNextHookEx(IntPtr hook, int code, IntPtr wParam, IntPtr lParam);

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)]
    public static extern IntPtr GetModuleHandle(string? name);

    // ---- Keyboard injection ----
    public const uint INPUT_KEYBOARD = 1;
    public const uint KEYEVENTF_EXTENDEDKEY = 0x1;
    public const uint KEYEVENTF_KEYUP = 0x2;

    [StructLayout(LayoutKind.Sequential)]
    public struct INPUT
    {
        public uint Type;
        public InputUnion U;
    }

    [StructLayout(LayoutKind.Explicit)]
    public struct InputUnion
    {
        [FieldOffset(0)] public MOUSEINPUT Mi;
        [FieldOffset(0)] public KEYBDINPUT Ki;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct MOUSEINPUT
    {
        public int Dx, Dy;
        public uint MouseData, Flags, Time;
        public IntPtr ExtraInfo;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct KEYBDINPUT
    {
        public ushort Vk;
        public ushort Scan;
        public uint Flags;
        public uint Time;
        public IntPtr ExtraInfo;
    }

    [DllImport("user32.dll", SetLastError = true)]
    public static extern uint SendInput(uint count, INPUT[] inputs, int size);

    [DllImport("user32.dll")]
    public static extern uint MapVirtualKey(uint code, uint mapType);

    [DllImport("user32.dll")]
    public static extern short GetAsyncKeyState(int vk);

    // ---- Windows ----
    public const int SW_MINIMIZE = 6;
    public const int SW_SHOWNOACTIVATE = 4;
    public const int SW_HIDE = 0;

    [DllImport("user32.dll")]
    public static extern IntPtr GetForegroundWindow();

    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);

    [DllImport("user32.dll")]
    public static extern bool ShowWindow(IntPtr hwnd, int cmd);

    public static readonly IntPtr HWND_TOPMOST = new(-1);
    public const uint SWP_NOSIZE = 0x1, SWP_NOMOVE = 0x2, SWP_NOACTIVATE = 0x10, SWP_SHOWWINDOW = 0x40;

    [DllImport("user32.dll")]
    public static extern bool SetWindowPos(IntPtr hwnd, IntPtr after, int x, int y, int cx, int cy, uint flags);

    [DllImport("user32.dll")]
    public static extern bool DestroyIcon(IntPtr icon);

    [DllImport("kernel32.dll")]
    public static extern bool AllocConsole();
}

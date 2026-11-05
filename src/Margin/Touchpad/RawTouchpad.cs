using System.Runtime.InteropServices;

namespace Margin.Touchpad;

/// Reads finger positions and the physical button from Windows Precision Touchpads via Raw Input.
///
/// A precision touchpad is a HID digitizer (usage page 0x0D, usage 0x05). Each finger is its own link
/// collection holding Contact ID, Tip Switch, X and Y. Reports can carry all fingers at once or be split
/// across several reports ("hybrid" mode), where only the first report has a non-zero Contact Count.
internal sealed class RawTouchpad : NativeWindow, IDisposable
{
    private const ushort PageGeneric = 0x01, UsageX = 0x30, UsageY = 0x31;
    private const ushort PageDigitizer = 0x0D, UsageTouchpad = 0x05, UsageTip = 0x42, UsageContactId = 0x51, UsageContactCount = 0x54;
    private const ushort PageButton = 0x09;

    private sealed record Axis(int Min, int Max)
    {
        public float Normalize(uint raw) => Max > Min ? Math.Clamp((float)((int)raw - Min) / (Max - Min), 0f, 1f) : 0f;
    }

    private sealed record Finger(ushort Link, Axis X, Axis Y);

    private sealed class Device : IDisposable
    {
        public IntPtr Preparsed;
        public List<Finger> Fingers = new();
        public bool HasContactCount;
        public bool HasButton;
        public string Name = "";

        // hybrid-mode frame assembly
        public int Expected;
        public readonly List<(int Id, float X, float Y)> Down = new();
        public int Collected;
        public bool Button;

        public void Dispose()
        {
            if (Preparsed != IntPtr.Zero) Marshal.FreeHGlobal(Preparsed);
            Preparsed = IntPtr.Zero;
        }
    }

    private readonly TouchTracker tracker;
    private readonly Dictionary<IntPtr, Device?> devices = new();
    private readonly ushort[] usageBuffer = new ushort[64];
    private IntPtr buffer = IntPtr.Zero;
    private uint bufferSize;
    private IntPtr activeDevice = IntPtr.Zero;
    private long lastDiag;

    public RawTouchpad(TouchTracker tracker)
    {
        this.tracker = tracker;
        CreateHandle(new CreateParams { Caption = "MarginRawInput", Parent = new IntPtr(-3) }); // HWND_MESSAGE
        Register();
    }

    public string? DeviceName => devices.Values.FirstOrDefault(d => d != null)?.Name;

    public void Register()
    {
        var rid = new[]
        {
            new Native.RAWINPUTDEVICE
            {
                UsagePage = PageDigitizer, Usage = UsageTouchpad,
                Flags = Native.RIDEV_INPUTSINK | Native.RIDEV_DEVNOTIFY, Target = Handle,
            },
        };
        if (!Native.RegisterRawInputDevices(rid, 1, (uint)Marshal.SizeOf<Native.RAWINPUTDEVICE>()))
            Log.Error($"RegisterRawInputDevices failed: {Marshal.GetLastWin32Error()}");
        else
            Log.Info("listening for precision touchpad input");
    }

    /// Processes WM_INPUT messages already waiting in the queue, so a click handled by the mouse hook
    /// sees the touch frame that produced it.
    public void Drain()
    {
        while (Native.PeekMessage(out var msg, Handle, Native.WM_INPUT, Native.WM_INPUT, Native.PM_REMOVE))
        {
            Process(msg.LParam);
            Native.DefWindowProc(msg.Hwnd, msg.Message, msg.WParam, msg.LParam);
        }
    }

    protected override void WndProc(ref Message m)
    {
        if (m.Msg == Native.WM_INPUT)
        {
            try { Process(m.LParam); }
            catch (Exception e) { Log.Error($"raw input: {e}"); }
        }
        else if (m.Msg == Native.WM_INPUT_DEVICE_CHANGE)
        {
            if (devices.Remove(m.LParam, out var d)) d?.Dispose();
            if (m.LParam == activeDevice) { activeDevice = IntPtr.Zero; tracker.Clear(); }
        }
        base.WndProc(ref m);
    }

    private void Process(IntPtr rawInput)
    {
        uint headerSize = (uint)(IntPtr.Size == 8 ? 24 : 16);
        uint size = 0;
        Native.GetRawInputData(rawInput, Native.RID_INPUT, IntPtr.Zero, ref size, headerSize);
        if (size == 0) return;
        if (size > bufferSize)
        {
            if (buffer != IntPtr.Zero) Marshal.FreeHGlobal(buffer);
            buffer = Marshal.AllocHGlobal((int)size);
            bufferSize = size;
        }
        if (Native.GetRawInputData(rawInput, Native.RID_INPUT, buffer, ref size, headerSize) == unchecked((uint)-1)) return;
        if ((uint)Marshal.ReadInt32(buffer, 0) != Native.RIM_TYPEHID) return;

        IntPtr hDevice = Marshal.ReadIntPtr(buffer, 8);
        var device = GetDevice(hDevice);
        if (device == null) return;

        // With two touchpads, the one touched first is in charge until all its fingers lift.
        if (activeDevice != IntPtr.Zero && activeDevice != hDevice && tracker.Count > 0) return;

        int reportSize = Marshal.ReadInt32(buffer, (int)headerSize);
        int reportCount = Marshal.ReadInt32(buffer, (int)headerSize + 4);
        var report = new byte[reportSize];
        for (int r = 0; r < reportCount; r++)
        {
            Marshal.Copy(buffer + (int)headerSize + 8 + r * reportSize, report, 0, reportSize);
            ParseReport(hDevice, device, report);
        }
    }

    private void ParseReport(IntPtr hDevice, Device d, byte[] report)
    {
        uint len = (uint)report.Length;
        bool button = false;
        if (d.HasButton)
        {
            uint n = (uint)usageBuffer.Length;
            if (Native.HidP_GetUsages(Native.HidP_Input, PageButton, 0, usageBuffer, ref n, d.Preparsed, report, len) == Native.HIDP_STATUS_SUCCESS)
                button = n > 0;
        }

        int count = -1;
        if (d.HasContactCount &&
            Native.HidP_GetUsageValue(Native.HidP_Input, PageDigitizer, 0, UsageContactCount, out uint c, d.Preparsed, report, len) == Native.HIDP_STATUS_SUCCESS)
            count = (int)c;

        if (count > 0 || count == -1 || d.Expected == 0)
        {
            // first report of a new frame
            d.Expected = count < 0 ? d.Fingers.Count : count;
            d.Down.Clear();
            d.Collected = 0;
            d.Button = false;
        }
        d.Button |= button;

        foreach (var f in d.Fingers)
        {
            if (d.Collected >= d.Expected) break;
            if (Native.HidP_GetUsageValue(Native.HidP_Input, PageDigitizer, f.Link, UsageContactId, out uint id, d.Preparsed, report, len) != Native.HIDP_STATUS_SUCCESS) continue;
            if (Native.HidP_GetUsageValue(Native.HidP_Input, PageGeneric, f.Link, UsageX, out uint rx, d.Preparsed, report, len) != Native.HIDP_STATUS_SUCCESS) continue;
            if (Native.HidP_GetUsageValue(Native.HidP_Input, PageGeneric, f.Link, UsageY, out uint ry, d.Preparsed, report, len) != Native.HIDP_STATUS_SUCCESS) continue;

            uint n = (uint)usageBuffer.Length;
            bool tip = Native.HidP_GetUsages(Native.HidP_Input, PageDigitizer, f.Link, usageBuffer, ref n, d.Preparsed, report, len) == Native.HIDP_STATUS_SUCCESS
                       && usageBuffer.AsSpan(0, (int)n).Contains(UsageTip);

            d.Collected++;
            // Windows reports Y growing downward; flip so y=1 is the top edge.
            if (tip) d.Down.Add(((int)id, f.X.Normalize(rx), 1f - f.Y.Normalize(ry)));
        }

        if (d.Collected >= d.Expected)
        {
            d.Expected = 0;
            activeDevice = d.Down.Count > 0 ? hDevice : IntPtr.Zero;
            tracker.Commit(d.Down, d.Button);
            LogFrame();
        }
    }

    private void LogFrame()
    {
        if (!Log.Verbose) return;
        long now = Environment.TickCount64;
        if (now - lastDiag < 60 && !tracker.ButtonDown) return;
        lastDiag = now;
        var parts = tracker.Snapshot().Select(t => $"#{t.Id} ({t.Current.X:0.00},{t.Current.Y:0.00}) from ({t.Start.X:0.00},{t.Start.Y:0.00})");
        Log.Debug($"frame btn={(tracker.ButtonDown ? 1 : 0)} n={tracker.Count} {string.Join(" ", parts)}");
    }

    private Device? GetDevice(IntPtr hDevice)
    {
        if (devices.TryGetValue(hDevice, out var cached)) return cached;
        Device? d = null;
        try { d = Describe(hDevice); }
        catch (Exception e) { Log.Error($"could not read touchpad description: {e.Message}"); }
        devices[hDevice] = d;
        if (d != null)
        {
            tracker.DeviceHasButton = d.HasButton;
            Log.Info($"touchpad: {d.Name} — {d.Fingers.Count} contact slots, button={d.HasButton}, contactCount={d.HasContactCount}");
            foreach (var f in d.Fingers) Log.Debug($"  slot link={f.Link} x={f.X.Min}..{f.X.Max} y={f.Y.Min}..{f.Y.Max}");
        }
        return d;
    }

    private static Device? Describe(IntPtr hDevice)
    {
        uint size = 0;
        Native.GetRawInputDeviceInfo(hDevice, Native.RIDI_PREPARSEDDATA, IntPtr.Zero, ref size);
        if (size == 0) return null;
        var d = new Device { Preparsed = Marshal.AllocHGlobal((int)size) };
        if (Native.GetRawInputDeviceInfo(hDevice, Native.RIDI_PREPARSEDDATA, d.Preparsed, ref size) == unchecked((uint)-1))
        {
            d.Dispose();
            return null;
        }

        uint nameSize = 0;
        Native.GetRawInputDeviceInfo(hDevice, Native.RIDI_DEVICENAME, IntPtr.Zero, ref nameSize);
        if (nameSize > 0)
        {
            var p = Marshal.AllocHGlobal((int)nameSize * 2);
            Native.GetRawInputDeviceInfo(hDevice, Native.RIDI_DEVICENAME, p, ref nameSize);
            d.Name = Marshal.PtrToStringUni(p) ?? "";
            Marshal.FreeHGlobal(p);
        }

        var caps = new byte[Native.HidpCapsSize];
        if (Native.HidP_GetCaps(d.Preparsed, caps) != Native.HIDP_STATUS_SUCCESS) { d.Dispose(); return null; }
        ushort valueCount = BitConverter.ToUInt16(caps, 48);
        ushort buttonCount = BitConverter.ToUInt16(caps, 46);

        // Value caps: find X/Y per link collection, and the top-level contact count.
        var xs = new Dictionary<ushort, Axis>();
        var ys = new Dictionary<ushort, Axis>();
        var ids = new HashSet<ushort>();
        if (valueCount > 0)
        {
            var vc = new byte[valueCount * Native.HidpValueCapsSize];
            ushort n = valueCount;
            Native.HidP_GetValueCaps(Native.HidP_Input, vc, ref n, d.Preparsed);
            for (int i = 0; i < n; i++)
            {
                int o = i * Native.HidpValueCapsSize;
                ushort page = BitConverter.ToUInt16(vc, o);
                ushort link = BitConverter.ToUInt16(vc, o + 6);
                bool isRange = vc[o + 12] != 0;
                ushort bitSize = BitConverter.ToUInt16(vc, o + 18);
                int min = BitConverter.ToInt32(vc, o + 40);
                int max = BitConverter.ToInt32(vc, o + 44);
                ushort uMin = BitConverter.ToUInt16(vc, o + 56);
                ushort uMax = isRange ? BitConverter.ToUInt16(vc, o + 58) : uMin;
                if (max <= min && bitSize is > 0 and < 32) max = (1 << bitSize) - 1; // unsigned field declared oddly

                bool Has(ushort usage) => usage >= uMin && usage <= uMax;
                if (page == PageGeneric && Has(UsageX)) xs[link] = new Axis(min, max);
                if (page == PageGeneric && Has(UsageY)) ys[link] = new Axis(min, max);
                if (page == PageDigitizer && Has(UsageContactId)) ids.Add(link);
                if (page == PageDigitizer && Has(UsageContactCount)) d.HasContactCount = true;
            }
        }
        foreach (var link in xs.Keys.OrderBy(k => k))
            if (ys.TryGetValue(link, out var y) && ids.Contains(link))
                d.Fingers.Add(new Finger(link, xs[link], y));

        if (buttonCount > 0)
        {
            var bc = new byte[buttonCount * Native.HidpButtonCapsSize];
            ushort n = buttonCount;
            Native.HidP_GetButtonCaps(Native.HidP_Input, bc, ref n, d.Preparsed);
            for (int i = 0; i < n; i++)
                if (BitConverter.ToUInt16(bc, i * Native.HidpButtonCapsSize) == PageButton) d.HasButton = true;
        }

        if (d.Fingers.Count == 0) { d.Dispose(); return null; }
        return d;
    }

    public void Dispose()
    {
        foreach (var d in devices.Values) d?.Dispose();
        devices.Clear();
        if (buffer != IntPtr.Zero) Marshal.FreeHGlobal(buffer);
        buffer = IntPtr.Zero;
        DestroyHandle();
    }
}

using System.Management;

namespace Margin;

/// Built-in display brightness through WMI. WMI calls take tens of milliseconds, so they run on a
/// worker thread and writes are coalesced: only the latest target is applied.
internal sealed class Brightness : IDisposable
{
    private readonly object gate = new();
    private readonly AutoResetEvent signal = new(false);
    private readonly Thread worker;
    private volatile bool stopping;
    private int current = -1;   // last known value, -1 = unknown
    private int pending = -1;   // value waiting to be written
    private bool refreshWanted = true;

    public bool Available { get; private set; } = true;

    public Brightness()
    {
        worker = new Thread(Run) { IsBackground = true, Name = "Margin brightness" };
        worker.Start();
        signal.Set();
    }

    /// Re-reads the hardware value (it may have changed through the keyboard keys).
    public void Refresh()
    {
        lock (gate) refreshWanted = true;
        signal.Set();
    }

    /// Moves brightness by `delta` percent and returns the new target (or -1 if unknown/unavailable).
    public int Nudge(int delta)
    {
        lock (gate)
        {
            if (current < 0) return -1;
            current = Math.Clamp(current + delta, 0, 100);
            pending = current;
        }
        signal.Set();
        return current;
    }

    private void Run()
    {
        while (!stopping)
        {
            signal.WaitOne();
            if (stopping) break;

            bool refresh;
            int write;
            lock (gate)
            {
                refresh = refreshWanted && pending < 0;
                refreshWanted = false;
                write = pending;
                pending = -1;
            }

            try
            {
                if (write >= 0) Write(write);
                else if (refresh)
                {
                    int value = Read();
                    lock (gate) if (pending < 0) current = value;
                    Available = value >= 0;
                }
            }
            catch (Exception e)
            {
                Log.Error($"brightness: {e.Message}");
            }
        }
    }

    private static int Read()
    {
        using var searcher = new ManagementObjectSearcher(@"root\wmi", "SELECT CurrentBrightness FROM WmiMonitorBrightness WHERE Active=TRUE");
        foreach (ManagementObject o in searcher.Get())
            using (o) return Convert.ToInt32(o["CurrentBrightness"]);
        return -1;
    }

    private static void Write(int value)
    {
        using var cls = new ManagementClass(@"root\wmi", "WmiMonitorBrightnessMethods", null);
        foreach (ManagementObject o in cls.GetInstances())
            using (o) o.InvokeMethod("WmiSetBrightness", new object[] { (uint)1, (byte)value });
    }

    public void Dispose()
    {
        stopping = true;
        signal.Set();
    }
}

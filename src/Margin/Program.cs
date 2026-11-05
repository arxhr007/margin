namespace Margin;

internal static class Program
{
    [STAThread]
    private static int Main(string[] args)
    {
        bool diag = args.Any(a => a.Equals("--diag", StringComparison.OrdinalIgnoreCase));
        using var mutex = new Mutex(true, @"Local\Margin.TouchpadEdges", out bool first);
        using var showSignal = new EventWaitHandle(false, EventResetMode.AutoReset, @"Local\Margin.ShowSettings");
        if (!first)
        {
            // Already running: ask that copy to open its settings window, then leave.
            showSignal.Set();
            return 0;
        }

        if (diag)
        {
            Native.AllocConsole();
            Log.ToConsole = true;
            Log.Verbose = true;
            Log.Info("diagnostic mode: every touch frame and gesture is printed here and in the log");
        }

        bool firstRun = !SettingsStore.Exists;
        ApplicationConfiguration.Initialize();
        Application.SetUnhandledExceptionMode(UnhandledExceptionMode.CatchException);
        Application.ThreadException += (_, e) => Log.Error($"unhandled: {e.Exception}");
        Application.Run(new TrayApp(firstRun, showSignal));
        return 0;
    }
}

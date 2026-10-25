namespace Margin;

internal static class Program
{
    [STAThread]
    private static int Main()
    {
        using var mutex = new Mutex(true, @"Local\Margin.TouchpadEdges", out bool first);
        if (!first) return 0;

        ApplicationConfiguration.Initialize();
        Application.Run(new TrayApp());
        return 0;
    }
}

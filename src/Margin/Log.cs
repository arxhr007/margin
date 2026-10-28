namespace Margin;

/// Tiny file logger: %APPDATA%\Margin\margin.log, truncated when it passes 1 MB.
internal static class Log
{
    private static readonly object Gate = new();

    public static bool ToConsole { get; set; }
    public static bool Verbose { get; set; }

    public static string FilePath => Path.Combine(SettingsStore.Directory, "margin.log");

    public static void Info(string message) => Write("info", message);
    public static void Error(string message) => Write("error", message);

    public static void Debug(string message)
    {
        if (Verbose) Write("debug", message);
    }

    private static void Write(string level, string message)
    {
        var line = $"{DateTime.Now:HH:mm:ss.fff} [{level}] {message}";
        lock (Gate)
        {
            if (ToConsole) Console.WriteLine(line);
            try
            {
                System.IO.Directory.CreateDirectory(SettingsStore.Directory);
                var info = new FileInfo(FilePath);
                if (info.Exists && info.Length > 1_000_000) File.WriteAllText(FilePath, "");
                File.AppendAllText(FilePath, line + Environment.NewLine);
            }
            catch
            {
                // logging must never take the app down
            }
        }
    }
}

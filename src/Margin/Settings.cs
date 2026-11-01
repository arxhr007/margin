using System.Text.Json;
using System.Text.Json.Serialization;

namespace Margin;

/// The four touchpad edges a user can assign.
public enum Section { Left, Right, Top, Bottom }

/// What sliding along an edge controls.
public enum SlideAction { Off, Volume, Brightness, Seek, Zoom, Tabs, Tracks, Desktops }

/// How an edge slide begins.
public enum Activation
{
    /// Press the edge until the touchpad clicks, then slide while holding.
    Press,
    /// Slide a finger in from the laptop body onto the edge, then along it. No click.
    SlideIn,
}

public sealed class SectionSettings
{
    /// Used by the four edges.
    public SlideAction Slide { get; set; }
}

public sealed class Settings
{
    public bool Enabled { get; set; } = true;
    public Activation Activation { get; set; } = Activation.Press;
    /// Depth of the edge strips, as a fraction of the touchpad.
    public double EdgeDepth { get; set; } = 0.10;
    /// Slide-in mode: a finger must first appear this close to the edge.
    public double EntryStrip { get; set; } = 0.04;

    public Dictionary<string, SectionSettings> Sections { get; set; } = Defaults();

    public SectionSettings this[Section s]
    {
        get
        {
            var key = Key(s);
            if (!Sections.TryGetValue(key, out var v)) Sections[key] = v = new SectionSettings();
            return v;
        }
    }

    public static string Key(Section s) => JsonNamingPolicy.CamelCase.ConvertName(s.ToString());

    private static Dictionary<string, SectionSettings> Defaults() => new()
    {
        [Key(Section.Left)] = new() { Slide = SlideAction.Brightness },
        [Key(Section.Right)] = new() { Slide = SlideAction.Volume },
        [Key(Section.Top)] = new() { Slide = SlideAction.Off },
        [Key(Section.Bottom)] = new() { Slide = SlideAction.Seek },
    };

    public void Normalize()
    {
        EdgeDepth = Math.Clamp(EdgeDepth, 0.05, 0.25);
        EntryStrip = Math.Clamp(EntryStrip, 0.01, 0.10);
        Sections ??= Defaults();
    }
}

internal static class SettingsStore
{
    public static readonly JsonSerializerOptions Options = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        PropertyNameCaseInsensitive = true,
        WriteIndented = true,
        ReadCommentHandling = JsonCommentHandling.Skip,
        AllowTrailingCommas = true,
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
        Converters = { new JsonStringEnumConverter(JsonNamingPolicy.CamelCase) },
    };

    public static string Directory =>
        Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "Margin");

    public static string FilePath => Path.Combine(Directory, "settings.json");

    public static bool Exists => File.Exists(FilePath);

    public static Settings Load()
    {
        Settings settings;
        try
        {
            settings = File.Exists(FilePath)
                ? JsonSerializer.Deserialize<Settings>(File.ReadAllText(FilePath), Options) ?? new Settings()
                : new Settings();
        }
        catch (Exception e)
        {
            Log.Error($"settings.json unreadable, using defaults: {e.Message}");
            settings = new Settings();
        }
        settings.Normalize();
        return settings;
    }

    public static void Save(Settings settings)
    {
        try
        {
            System.IO.Directory.CreateDirectory(Directory);
            var tmp = FilePath + ".tmp";
            File.WriteAllText(tmp, JsonSerializer.Serialize(settings, Options));
            File.Move(tmp, FilePath, overwrite: true);
        }
        catch (Exception e)
        {
            Log.Error($"could not save settings: {e.Message}");
        }
    }
}

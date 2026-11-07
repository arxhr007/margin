using System.Text.Json;
using System.Text.Json.Serialization;

namespace Margin;

/// The eight touchpad regions a user can assign.
public enum Section { Left, Right, Top, Bottom, TopLeft, TopRight, BottomLeft, BottomRight }

/// What sliding along an edge controls.
public enum SlideAction { Off, Volume, Brightness, Seek, Zoom, Tabs, Tracks, Desktops, Custom }

/// What clicking a corner does.
public enum ClickAction
{
    None, Mute, PlayPause, NextTrack, PrevTrack, Screenshot, TaskView, ShowDesktop, Lock, Notifications,
    App, Url, Keys, Command,
}

/// How an edge slide begins.
public enum Activation
{
    /// Press the edge until the touchpad clicks, then slide while holding.
    Press,
    /// Slide a finger in from the laptop body onto the edge, then along it. No click.
    SlideIn,
}

public enum Speed { Slow, Medium, Fast }

public sealed class ClickSpec
{
    public ClickAction Do { get; set; }
    /// App path, URL, key combo ("Ctrl+Shift+Esc") or command line, depending on the action.
    public string? Value { get; set; }
}

public sealed class SectionSettings
{
    /// Used by the four edges.
    public SlideAction Slide { get; set; }
    /// Custom keys: shortcut sent per step when sliding right / up ("Alt+Tab", "Ctrl+PageDown", ...).
    public string? Forward { get; set; }
    /// Custom keys: shortcut sent per step when sliding left / down.
    public string? Back { get; set; }
    /// Custom keys: keep the modifiers both shortcuts share (e.g. Alt) held for the whole slide, like Alt+Tab.
    public bool HoldModifiers { get; set; }
    /// Used by the four corners: index 0 = single click, 1 = double, 2 = triple.
    public List<ClickSpec> Clicks { get; set; } = new();
}

public sealed class Settings
{
    public bool Enabled { get; set; } = true;
    public Activation Activation { get; set; } = Activation.SlideIn;
    public Speed Speed { get; set; } = Speed.Medium;
    /// Depth of the edge strips, as a fraction of the touchpad.
    public double EdgeDepth { get; set; } = 0.10;
    /// Side of each corner square.
    public double CornerSize { get; set; } = 0.15;
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
        [Key(Section.Left)] = new() { Slide = SlideAction.Volume },
        [Key(Section.Right)] = new() { Slide = SlideAction.Brightness },
        [Key(Section.Top)] = new() { Slide = SlideAction.Tabs },
        [Key(Section.Bottom)] = new() { Slide = SlideAction.Desktops },
        [Key(Section.TopLeft)] = new() { Clicks = { new() { Do = ClickAction.ShowDesktop } } },
        [Key(Section.TopRight)] = new() { Clicks = { new() { Do = ClickAction.TaskView }, new() { Do = ClickAction.Notifications } } },
        [Key(Section.BottomLeft)] = new() { Clicks = { new() { Do = ClickAction.Lock } } },
        [Key(Section.BottomRight)] = new()
        {
            Clicks = { new() { Do = ClickAction.PlayPause }, new() { Do = ClickAction.NextTrack }, new() { Do = ClickAction.PrevTrack } },
        },
    };

    public void Normalize()
    {
        EdgeDepth = Math.Clamp(EdgeDepth, 0.05, 0.25);
        CornerSize = Math.Clamp(CornerSize, 0.08, 0.30);
        EntryStrip = Math.Clamp(EntryStrip, 0.01, 0.10);
        Sections ??= Defaults();
        foreach (var s in Enum.GetValues<Section>())
        {
            var v = this[s];
            v.Clicks ??= new();
            if (v.Clicks.Count > 3) v.Clicks.RemoveRange(3, v.Clicks.Count - 3);
        }
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

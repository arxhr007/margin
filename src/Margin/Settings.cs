using System.Text.Json;

namespace Margin;

/// The four touchpad edges a user can assign.
public enum Section { Left, Right, Top, Bottom }

/// What sliding along an edge controls.
public enum SlideAction { Off, Volume, Brightness, Seek }

public sealed class SectionSettings
{
    /// Used by the four edges.
    public SlideAction Slide { get; set; }
}

public sealed class Settings
{
    public bool Enabled { get; set; } = true;
    /// Depth of the edge strips, as a fraction of the touchpad.
    public double EdgeDepth { get; set; } = 0.10;

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
        Sections ??= Defaults();
    }
}

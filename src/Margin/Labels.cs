namespace Margin;

/// Human-readable names for actions, used by the settings window and the touchpad map.
internal static class Labels
{
    public static string Long(SlideAction a) => a switch
    {
        SlideAction.Off => "Nothing (normal touchpad)",
        SlideAction.Volume => "Volume",
        SlideAction.Brightness => "Screen brightness",
        SlideAction.Seek => "Seek video / audio",
        SlideAction.Zoom => "Zoom in / out",
        SlideAction.Tabs => "Switch tabs",
        SlideAction.Tracks => "Previous / next track",
        SlideAction.Desktops => "Switch virtual desktops",
        _ => a.ToString(),
    };

    public static string Short(SlideAction a) => a switch
    {
        SlideAction.Off => "",
        SlideAction.Brightness => "Brightness",
        SlideAction.Tracks => "Tracks",
        SlideAction.Desktops => "Desktops",
        _ => a.ToString(),
    };
}

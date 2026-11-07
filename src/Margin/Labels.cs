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

    public static string Long(ClickAction a) => a switch
    {
        ClickAction.None => "Nothing",
        ClickAction.Mute => "Mute / unmute",
        ClickAction.PlayPause => "Play / pause",
        ClickAction.NextTrack => "Next track",
        ClickAction.PrevTrack => "Previous track",
        ClickAction.Screenshot => "Screenshot (Snipping Tool)",
        ClickAction.TaskView => "Task View",
        ClickAction.ShowDesktop => "Show desktop",
        ClickAction.Lock => "Lock the PC",
        ClickAction.Notifications => "Notifications",
        ClickAction.App => "Open an app…",
        ClickAction.Url => "Open a website…",
        ClickAction.Keys => "Press a shortcut…",
        ClickAction.Command => "Run a command…",
        _ => a.ToString(),
    };

    public static string Short(ClickAction a) => a switch
    {
        ClickAction.PlayPause => "Play",
        ClickAction.NextTrack => "Next",
        ClickAction.PrevTrack => "Prev",
        ClickAction.Screenshot => "Snip",
        ClickAction.TaskView => "Tasks",
        ClickAction.ShowDesktop => "Desktop",
        ClickAction.Notifications => "Notifs",
        ClickAction.Command => "Run",
        _ => a.ToString(),
    };

    public static bool NeedsValue(ClickAction a) => a is ClickAction.App or ClickAction.Url or ClickAction.Keys or ClickAction.Command;

    public static string Placeholder(ClickAction a) => a switch
    {
        ClickAction.App => "e.g. notepad.exe or a full path",
        ClickAction.Url => "e.g. https://youtube.com",
        ClickAction.Keys => "e.g. Ctrl+Shift+Esc",
        ClickAction.Command => "e.g. explorer.exe shell:Downloads",
        _ => "",
    };
}

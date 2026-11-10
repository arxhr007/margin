//! Human-readable names for actions, used by the settings window and the touchpad map.

use crate::settings::{ClickAction, SlideAction};

pub fn slide_long(a: SlideAction) -> &'static str {
    match a {
        SlideAction::Off => "Nothing (normal touchpad)",
        SlideAction::Volume => "Volume",
        SlideAction::Brightness => "Screen brightness",
        SlideAction::Seek => "Seek video / audio",
        SlideAction::Scroll => "Scroll (like a scroll bar)",
        SlideAction::Zoom => "Zoom in / out",
        SlideAction::Tabs => "Switch tabs (Alt+Tab)",
        SlideAction::Tracks => "Previous / next track",
        SlideAction::Desktops => "Switch virtual desktops",
        SlideAction::Custom => "Custom keys\u{2026}",
    }
}

pub fn slide_short(a: SlideAction) -> &'static str {
    match a {
        SlideAction::Off => "",
        SlideAction::Volume => "Volume",
        SlideAction::Brightness => "Brightness",
        SlideAction::Seek => "Seek",
        SlideAction::Scroll => "Scroll",
        SlideAction::Zoom => "Zoom",
        SlideAction::Tabs => "Tabs",
        SlideAction::Tracks => "Tracks",
        SlideAction::Desktops => "Desktops",
        SlideAction::Custom => "Keys",
    }
}

pub fn click_long(a: ClickAction) -> &'static str {
    match a {
        ClickAction::None => "Nothing",
        ClickAction::Mute => "Mute / unmute",
        ClickAction::PlayPause => "Play / pause",
        ClickAction::NextTrack => "Next track",
        ClickAction::PrevTrack => "Previous track",
        ClickAction::Screenshot => "Screenshot (Snipping Tool)",
        ClickAction::TaskView => "Task View",
        ClickAction::ShowDesktop => "Show desktop",
        ClickAction::Lock => "Lock the PC",
        ClickAction::Notifications => "Notifications",
        ClickAction::App => "Open an app\u{2026}",
        ClickAction::Url => "Open a website\u{2026}",
        ClickAction::Keys => "Press a shortcut\u{2026}",
        ClickAction::Command => "Run a command\u{2026}",
    }
}

pub fn click_short(a: ClickAction) -> &'static str {
    match a {
        ClickAction::None => "",
        ClickAction::Mute => "Mute",
        ClickAction::PlayPause => "Play",
        ClickAction::NextTrack => "Next",
        ClickAction::PrevTrack => "Prev",
        ClickAction::Screenshot => "Snip",
        ClickAction::TaskView => "Tasks",
        ClickAction::ShowDesktop => "Desktop",
        ClickAction::Lock => "Lock",
        ClickAction::Notifications => "Notifs",
        ClickAction::App => "App",
        ClickAction::Url => "Url",
        ClickAction::Keys => "Keys",
        ClickAction::Command => "Run",
    }
}

pub fn needs_value(a: ClickAction) -> bool {
    matches!(
        a,
        ClickAction::App | ClickAction::Url | ClickAction::Keys | ClickAction::Command
    )
}

pub fn placeholder(a: ClickAction) -> &'static str {
    match a {
        ClickAction::App => "e.g. notepad.exe or a full path",
        ClickAction::Url => "e.g. https://youtube.com",
        ClickAction::Keys => "e.g. Ctrl+Shift+Esc",
        ClickAction::Command => "e.g. explorer.exe shell:Downloads",
        _ => "",
    }
}

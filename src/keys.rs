//! Key codes, shortcut text ("Ctrl+Shift+Tab") parsing, and key presses through SendInput.

pub const VOLUME_MUTE: u16 = 0xAD;
pub const VOLUME_DOWN: u16 = 0xAE;
pub const VOLUME_UP: u16 = 0xAF;
pub const MEDIA_PLAY_PAUSE: u16 = 0xB3;
pub const MEDIA_NEXT: u16 = 0xB0;
pub const MEDIA_PREV: u16 = 0xB1;
pub const LEFT: u16 = 0x25;
pub const RIGHT: u16 = 0x27;
pub const TAB: u16 = 0x09;
pub const CTRL: u16 = 0xA2;
pub const SHIFT: u16 = 0xA0;
pub const WIN: u16 = 0x5B;
pub const ALT: u16 = 0xA4;

/// Keys that carry the E0 prefix on a real keyboard.
const EXTENDED: &[u16] = &[
    0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, // page up/down, end, home, arrows
    0x2C, 0x2D, 0x2E, // print screen, insert, delete
    0x5B, 0x5C, 0x5D, // win keys, apps
    0xA3, 0xA5, // right ctrl / alt
    0xAD, 0xAE, 0xAF, 0xB0, 0xB1, 0xB2, 0xB3, // volume + media
];

pub fn is_extended(vk: u16) -> bool {
    EXTENDED.contains(&vk)
}

/// Names understood in shortcut text, besides single letters/digits and F1-F24.
const NAMED: &[(&str, u16)] = &[
    ("-", 0xBD), ("=", 0xBB), ("[", 0xDB), ("]", 0xDD), (";", 0xBA), ("'", 0xDE),
    (",", 0xBC), (".", 0xBE), ("/", 0xBF), ("\\", 0xDC), ("`", 0xC0),
    ("return", 0x0D), ("enter", 0x0D), ("tab", 0x09), ("space", 0x20),
    ("backspace", 0x08), ("delete", 0x2E), ("del", 0x2E), ("insert", 0x2D),
    ("escape", 0x1B), ("esc", 0x1B),
    ("left", 0x25), ("up", 0x26), ("right", 0x27), ("down", 0x28),
    ("home", 0x24), ("end", 0x23), ("pageup", 0x21), ("pagedown", 0x22),
    ("printscreen", 0x2C), ("prtsc", 0x2C),
    ("volumeup", VOLUME_UP), ("volumedown", VOLUME_DOWN), ("mute", VOLUME_MUTE),
    ("playpause", MEDIA_PLAY_PAUSE), ("nexttrack", 0xB0), ("prevtrack", 0xB1),
];

/// Virtual-key code for a key name ("a", "5", "f11", "PageDown", "tab"...), case-insensitive.
pub fn key_code(name: &str) -> Option<u16> {
    let lower = name.to_lowercase();
    let mut chars = lower.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        if c.is_ascii_lowercase() {
            return Some(c.to_ascii_uppercase() as u16);
        }
        if c.is_ascii_digit() {
            return Some(c as u16);
        }
    }
    if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u16>().ok()) {
        if (1..=24).contains(&n) {
            return Some(0x6F + n);
        }
    }
    NAMED.iter().find(|(n, _)| *n == lower).map(|&(_, vk)| vk)
}

/// Modifier name -> virtual key (`cmd` maps to Ctrl and `opt` to Alt, for configs shared with the Mac app).
pub fn modifier_code(name: &str) -> Option<u16> {
    match name.to_lowercase().as_str() {
        "ctrl" | "control" | "cmd" | "command" => Some(CTRL),
        "shift" => Some(SHIFT),
        "alt" | "opt" | "option" => Some(ALT),
        "win" | "windows" | "super" => Some(WIN),
        _ => None,
    }
}

/// "Ctrl+Shift+Esc" -> (Esc, [Ctrl, Shift]). None when the text is empty or any part is unknown.
pub fn parse_shortcut(text: &str) -> Option<(u16, Vec<u16>)> {
    if text.trim().is_empty() {
        return None;
    }
    let parts: Vec<&str> = text.split('+').map(str::trim).filter(|p| !p.is_empty()).collect();
    let (last, modifiers) = parts.split_last()?;
    let mut mods = Vec::new();
    for m in modifiers {
        mods.push(modifier_code(m)?);
    }
    Some((key_code(last)?, mods))
}

/// Display name of a virtual key as written in shortcut text (used by the Record button).
pub fn key_name(vk: u32) -> Option<String> {
    let vk16 = u16::try_from(vk).ok()?;
    match vk16 {
        0x41..=0x5A => return Some(((vk16 as u8) as char).to_string()),
        0x30..=0x39 => return Some(((vk16 as u8) as char).to_string()),
        0x70..=0x87 => return Some(format!("F{}", vk16 - 0x6F)),
        _ => {}
    }
    let name = match vk16 {
        0x09 => "Tab",
        0x0D => "Enter",
        0x20 => "Space",
        0x08 => "Backspace",
        0x2E => "Delete",
        0x2D => "Insert",
        0x1B => "Esc",
        0x25 => "Left",
        0x26 => "Up",
        0x27 => "Right",
        0x28 => "Down",
        0x24 => "Home",
        0x23 => "End",
        0x21 => "PageUp",
        0x22 => "PageDown",
        0x2C => "PrintScreen",
        0xBD => "-",
        0xBB => "=",
        0xDB => "[",
        0xDD => "]",
        0xBA => ";",
        0xDE => "'",
        0xBC => ",",
        0xBE => ".",
        0xBF => "/",
        0xDC => "\\",
        0xC0 => "`",
        VOLUME_UP => "VolumeUp",
        VOLUME_DOWN => "VolumeDown",
        VOLUME_MUTE => "Mute",
        MEDIA_PLAY_PAUSE => "PlayPause",
        MEDIA_NEXT => "NextTrack",
        MEDIA_PREV => "PrevTrack",
        _ => return None,
    };
    Some(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_shortcuts() {
        assert_eq!(parse_shortcut("Alt+Tab"), Some((TAB, vec![ALT])));
        assert_eq!(parse_shortcut("alt+shift+tab"), Some((TAB, vec![ALT, SHIFT])));
        assert_eq!(parse_shortcut("Ctrl + Shift + Esc"), Some((0x1B, vec![CTRL, SHIFT])));
        assert_eq!(parse_shortcut("Win+Ctrl+Right"), Some((RIGHT, vec![WIN, CTRL])));
        assert_eq!(parse_shortcut("cmd+shift+5"), Some((b'5' as u16, vec![CTRL, SHIFT])));
        assert_eq!(parse_shortcut("F11"), Some((0x7A, vec![])));
        assert_eq!(parse_shortcut("Ctrl+PageDown"), Some((0x22, vec![CTRL])));
    }

    #[test]
    fn rejects_bad_shortcuts() {
        assert_eq!(parse_shortcut(""), None);
        assert_eq!(parse_shortcut("   "), None);
        assert_eq!(parse_shortcut("Ctrl+"), None, "a modifier alone is not a key");
        assert_eq!(parse_shortcut("Hyper+A"), None);
        assert_eq!(parse_shortcut("Ctrl+Banana"), None);
        assert_eq!(parse_shortcut("F25"), None);
    }

    #[test]
    fn recorded_names_parse_back() {
        for vk in [0x41u32, 0x5A, 0x30, 0x39, 0x70, 0x87, 0x09, 0x21, 0x22, 0x25, 0xBD, 0xDC, 0xC0, 0xAF] {
            let name = key_name(vk).unwrap();
            assert_eq!(key_code(&name), Some(vk as u16), "{name}");
        }
        assert_eq!(key_name(0x07), None);
        assert_eq!(key_name(0x10), None, "shift alone is not bindable");
    }
}

// ---------------------------------------------------------------- SendInput

#[cfg(windows)]
mod send {
    use super::is_extended;
    use windows::Win32::UI::Input::KeyboardAndMouse::*;

    fn key(vk: u16, up: bool) -> INPUT {
        let mut flags = if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) };
        if is_extended(vk) {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
        let scan = unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC) } as u16;
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT { wVk: VIRTUAL_KEY(vk), wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: 0 },
            },
        }
    }

    fn send(inputs: &[INPUT]) {
        let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
        if sent as usize != inputs.len() {
            crate::log::error(format!("SendInput sent {sent} of {}", inputs.len()));
        }
    }

    /// Holds the modifiers, taps the key `times` times, releases the modifiers.
    pub fn combo(vk: u16, modifiers: &[u16], times: u32) {
        let mut inputs = Vec::new();
        for &m in modifiers {
            inputs.push(key(m, false));
        }
        for _ in 0..times {
            inputs.push(key(vk, false));
            inputs.push(key(vk, true));
        }
        for &m in modifiers.iter().rev() {
            inputs.push(key(m, true));
        }
        send(&inputs);
    }

    pub fn tap(vk: u16, times: u32) {
        combo(vk, &[], times);
    }

    /// Presses or releases a single key and leaves it that way (Alt held through an Alt+Tab slide).
    pub fn hold(vk: u16, up: bool) {
        send(&[key(vk, up)]);
    }

    /// Ctrl + mouse wheel, which zooms in browsers, Office, Explorer and most viewers.
    pub fn ctrl_wheel(notches: i32) {
        if notches == 0 {
            return;
        }
        let wheel = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: (notches * 120) as u32,
                    dwFlags: MOUSEEVENTF_WHEEL,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        send(&[key(super::CTRL, false), wheel, key(super::CTRL, true)]);
    }

    /// Ctrl and Shift both held -> fine steps.
    pub fn fine_modifiers_held() -> bool {
        unsafe {
            (GetAsyncKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0
                && (GetAsyncKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0
        }
    }

    pub fn win_held() -> bool {
        unsafe {
            (GetAsyncKeyState(VK_LWIN.0 as i32) as u16 & 0x8000) != 0
                || (GetAsyncKeyState(VK_RWIN.0 as i32) as u16 & 0x8000) != 0
        }
    }
}

#[cfg(windows)]
pub use send::{combo, ctrl_wheel, fine_modifiers_held, hold, tap};

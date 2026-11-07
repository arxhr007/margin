//! Maps the Windows virtual-key codes used by the shared shortcut parser to Linux evdev key codes.

use crate::keys;

pub const KEY_ESC: u16 = 1;
pub const KEY_LEFTCTRL: u16 = 29;
pub const KEY_LEFTSHIFT: u16 = 42;
pub const KEY_LEFTALT: u16 = 56;
pub const KEY_LEFTMETA: u16 = 125;
pub const KEY_SYSRQ: u16 = 99; // the "Print Screen" key
pub const KEY_MUTE: u16 = 113;
pub const KEY_VOLUMEDOWN: u16 = 114;
pub const KEY_VOLUMEUP: u16 = 115;
pub const KEY_NEXTSONG: u16 = 163;
pub const KEY_PLAYPAUSE: u16 = 164;
pub const KEY_PREVIOUSSONG: u16 = 165;
pub const KEY_BRIGHTNESSDOWN: u16 = 224;
pub const KEY_BRIGHTNESSUP: u16 = 225;

/// Linux key codes of the letters A..Z, in alphabet order.
const LETTERS: [u16; 26] = [
    30, 48, 46, 32, 18, 33, 34, 35, 23, 36, 37, 38, 50, 49, 24, 25, 16, 19, 31, 20, 22, 47, 17, 45, 21, 44,
];
/// Linux key codes of the digits 0..9.
const DIGITS: [u16; 10] = [11, 2, 3, 4, 5, 6, 7, 8, 9, 10];

pub fn vk_to_key(vk: u16) -> Option<u16> {
    Some(match vk {
        0x41..=0x5A => LETTERS[(vk - 0x41) as usize],
        0x30..=0x39 => DIGITS[(vk - 0x30) as usize],
        0x70..=0x79 => 59 + (vk - 0x70), // F1-F10
        0x7A => 87,                      // F11
        0x7B => 88,                      // F12
        0x7C..=0x87 => 183 + (vk - 0x7C), // F13-F24
        0x08 => 14,                      // backspace
        0x09 => 15,                      // tab
        0x0D => 28,                      // enter
        0x1B => KEY_ESC,
        0x20 => 57, // space
        0x21 => 104, // page up
        0x22 => 109, // page down
        0x23 => 107, // end
        0x24 => 102, // home
        0x25 => 105, // left
        0x26 => 103, // up
        0x27 => 106, // right
        0x28 => 108, // down
        0x2C => KEY_SYSRQ,
        0x2D => 110, // insert
        0x2E => 111, // delete
        0xBA => 39,  // ;
        0xBB => 13,  // =
        0xBC => 51,  // ,
        0xBD => 12,  // -
        0xBE => 52,  // .
        0xBF => 53,  // /
        0xC0 => 41,  // `
        0xDB => 26,  // [
        0xDC => 43,  // backslash
        0xDD => 27,  // ]
        0xDE => 40,  // '
        // modifiers (the left-hand keys; the specific and generic VK codes both map here)
        0x10 | 0xA0 => KEY_LEFTSHIFT,
        0x11 | 0xA2 => KEY_LEFTCTRL,
        0x12 | 0xA4 => KEY_LEFTALT,
        0x5B => KEY_LEFTMETA,
        // media
        keys::VOLUME_MUTE => KEY_MUTE,
        keys::VOLUME_DOWN => KEY_VOLUMEDOWN,
        keys::VOLUME_UP => KEY_VOLUMEUP,
        keys::MEDIA_NEXT => KEY_NEXTSONG,
        keys::MEDIA_PREV => KEY_PREVIOUSSONG,
        keys::MEDIA_PLAY_PAUSE => KEY_PLAYPAUSE,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spot_checks() {
        assert_eq!(vk_to_key(b'A' as u16), Some(30));
        assert_eq!(vk_to_key(b'Q' as u16), Some(16));
        assert_eq!(vk_to_key(b'Z' as u16), Some(44));
        assert_eq!(vk_to_key(b'1' as u16), Some(2));
        assert_eq!(vk_to_key(b'0' as u16), Some(11));
        assert_eq!(vk_to_key(keys::TAB), Some(15));
        assert_eq!(vk_to_key(keys::CTRL), Some(KEY_LEFTCTRL));
        assert_eq!(vk_to_key(keys::SHIFT), Some(KEY_LEFTSHIFT));
        assert_eq!(vk_to_key(keys::ALT), Some(KEY_LEFTALT));
        assert_eq!(vk_to_key(keys::WIN), Some(KEY_LEFTMETA));
        assert_eq!(vk_to_key(keys::VOLUME_UP), Some(KEY_VOLUMEUP));
        assert_eq!(vk_to_key(0x70), Some(59)); // F1
        assert_eq!(vk_to_key(0x7A), Some(87)); // F11
        assert_eq!(vk_to_key(0x87), Some(194)); // F24
        assert_eq!(vk_to_key(0x07), None);
    }

    #[test]
    fn every_key_a_shortcut_can_name_has_a_linux_key() {
        let names = [
            "a", "z", "0", "9", "f1", "f12", "f24", "-", "=", "[", "]", ";", "'", ",", ".", "/", "\\", "`", "enter", "tab", "space",
            "backspace", "delete", "insert", "esc", "left", "up", "right", "down", "home", "end", "pageup", "pagedown", "printscreen",
            "volumeup", "volumedown", "mute", "playpause", "nexttrack", "prevtrack",
        ];
        for n in names {
            let vk = keys::key_code(n).unwrap_or_else(|| panic!("{n} is not a key name"));
            assert!(vk_to_key(vk).is_some(), "no Linux key for {n} (vk {vk:#x})");
        }
        for m in ["ctrl", "shift", "alt", "win"] {
            assert!(vk_to_key(keys::modifier_code(m).unwrap()).is_some(), "{m}");
        }
    }

    #[test]
    fn no_two_keys_share_a_linux_code() {
        let mut seen = std::collections::HashMap::new();
        for vk in 0u16..0x100 {
            // the generic and the left/right modifier codes intentionally map to the same key
            if matches!(vk, 0x10 | 0x11 | 0x12) {
                continue;
            }
            if let Some(k) = vk_to_key(vk) {
                if let Some(prev) = seen.insert(k, vk) {
                    panic!("vk {prev:#x} and {vk:#x} both map to Linux key {k}");
                }
            }
        }
    }
}

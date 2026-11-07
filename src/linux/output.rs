//! Sends key presses, the zoom wheel and brightness steps through virtual input devices.
//! Shortcuts are written with the shared Windows virtual-key codes and translated here.

use std::time::Duration;

use super::keymap::*;
use super::uinput::VirtualDevice;

pub struct Output {
    keyboard: Option<VirtualDevice>,
    mouse: Option<VirtualDevice>,
}

impl Output {
    /// Creates the virtual devices. If /dev/uinput isn't usable, Margin still runs and logs gestures.
    pub fn new() -> Output {
        let keyboard = match VirtualDevice::keyboard() {
            Ok(k) => Some(k),
            Err(e) => {
                crate::log::error(format!(
                    "cannot create the virtual keyboard ({e}). Margin needs write access to /dev/uinput: \
                     run the installer, or see \"Permissions\" in the README"
                ));
                None
            }
        };
        let mouse = VirtualDevice::mouse().ok();
        if keyboard.is_some() {
            // desktops notice a new input device a moment after it appears; keys sent earlier would be lost
            std::thread::sleep(Duration::from_millis(300));
        }
        Output { keyboard, mouse }
    }

    pub fn available(&self) -> bool {
        self.keyboard.is_some()
    }

    fn raw_key(&self, code: u16, down: bool) {
        if let Some(k) = &self.keyboard {
            if let Err(e) = k.key(code, down) {
                crate::log::error(format!("could not send key {code}: {e}"));
            }
        }
    }

    fn vk(&self, vk: u16, down: bool) {
        match vk_to_key(vk) {
            Some(code) => self.raw_key(code, down),
            None => crate::log::error(format!("no Linux key for virtual key {vk:#x}")),
        }
    }

    /// Holds the modifiers, taps the key `times` times, releases the modifiers.
    pub fn combo(&self, vk: u16, modifiers: &[u16], times: u32) {
        for &m in modifiers {
            self.vk(m, true);
        }
        for _ in 0..times {
            self.vk(vk, true);
            self.vk(vk, false);
        }
        for &m in modifiers.iter().rev() {
            self.vk(m, false);
        }
    }

    pub fn tap(&self, vk: u16, times: u32) {
        self.combo(vk, &[], times);
    }

    /// Presses or releases a key and leaves it that way (Alt held through an Alt+Tab slide).
    pub fn hold(&self, vk: u16, up: bool) {
        self.vk(vk, !up);
    }

    /// Ctrl + mouse wheel, which zooms in browsers, document viewers and most apps.
    pub fn ctrl_wheel(&self, notches: i32) {
        let Some(mouse) = &self.mouse else { return };
        if notches == 0 {
            return;
        }
        self.vk(crate::keys::CTRL, true);
        let _ = mouse.emit(super::sys::EV_REL, super::sys::REL_WHEEL, notches);
        let _ = mouse.emit(super::sys::EV_REL, super::sys::REL_WHEEL_HI_RES, notches * 120);
        let _ = mouse.sync();
        self.vk(crate::keys::CTRL, false);
    }

    /// Brightness has no portable "set to N %" on Linux, so slide steps become brightness key presses
    /// and the desktop applies its own step size and shows its own on-screen display.
    pub fn brightness(&self, delta_percent: i32) {
        let presses = (delta_percent.unsigned_abs() / 5).max(1);
        let code = if delta_percent >= 0 { KEY_BRIGHTNESSUP } else { KEY_BRIGHTNESSDOWN };
        for _ in 0..presses {
            self.raw_key(code, true);
            self.raw_key(code, false);
        }
    }

    pub fn tap_raw(&self, code: u16) {
        self.raw_key(code, true);
        self.raw_key(code, false);
    }
}

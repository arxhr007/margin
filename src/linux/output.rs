//! Sends key presses, the mouse wheel and brightness steps through virtual input devices.
//! Shortcuts are written with the shared Windows virtual-key codes and translated here.

use std::cell::Cell;
use std::time::Duration;

use super::keymap::*;
use super::uinput::VirtualDevice;

pub struct Output {
    keyboard: Option<VirtualDevice>,
    mouse: Option<VirtualDevice>,
    /// Scroll units not yet sent as a whole low-resolution notch: (vertical, horizontal).
    wheel_rest: Cell<(i32, i32)>,
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
        Output { keyboard, mouse, wheel_rest: Cell::new((0, 0)) }
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

    /// Scrolls by `units` (120 = one notch); positive is up / right. Every step goes out as a
    /// high-resolution event, and each whole notch also as a classic wheel event, as real mice do.
    pub fn wheel(&self, units: i32, horizontal: bool) {
        let Some(mouse) = &self.mouse else { return };
        if units == 0 {
            return;
        }
        let (mut v, mut h) = self.wheel_rest.get();
        let rest = if horizontal { &mut h } else { &mut v };
        if *rest != 0 && (*rest > 0) != (units > 0) {
            *rest = 0; // direction changed: start counting the notch again
        }
        *rest += units;
        let notches = *rest / 120;
        *rest -= notches * 120;
        self.wheel_rest.set((v, h));
        let (lo, hi) = if horizontal {
            (super::sys::REL_HWHEEL, super::sys::REL_HWHEEL_HI_RES)
        } else {
            (super::sys::REL_WHEEL, super::sys::REL_WHEEL_HI_RES)
        };
        if notches != 0 {
            let _ = mouse.emit(super::sys::EV_REL, lo, notches);
        }
        let _ = mouse.emit(super::sys::EV_REL, hi, units);
        let _ = mouse.sync();
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

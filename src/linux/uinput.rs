//! Virtual input devices through /dev/uinput. Margin uses one keyboard (for shortcuts and media keys)
//! and one mouse (only for the wheel that zooms), because desktops ignore wheel events from a device
//! that isn't a complete pointer.

use std::io;
use std::os::unix::io::RawFd;

use super::sys::*;

pub struct VirtualDevice {
    fd: Fd,
}

impl VirtualDevice {
    pub fn keyboard() -> io::Result<VirtualDevice> {
        // every key below BTN_MISC, so any shortcut can be sent
        let keys: Vec<u16> = (1u16..0x100).collect();
        Self::create("Margin virtual keyboard", &keys, &[])
    }

    pub fn mouse() -> io::Result<VirtualDevice> {
        Self::create(
            "Margin virtual mouse",
            &[BTN_LEFT, BTN_RIGHT, BTN_MIDDLE],
            &[REL_X, REL_Y, REL_WHEEL, REL_WHEEL_HI_RES],
        )
    }

    fn create(name: &str, keys: &[u16], rels: &[u16]) -> io::Result<VirtualDevice> {
        let fd = Fd::open("/dev/uinput", libc::O_WRONLY | libc::O_NONBLOCK)?;
        if !keys.is_empty() {
            fd.ioctl_int(UI_SET_EVBIT, EV_KEY as i32)?;
            for &k in keys {
                fd.ioctl_int(UI_SET_KEYBIT, k as i32)?;
            }
        }
        if !rels.is_empty() {
            fd.ioctl_int(UI_SET_EVBIT, EV_REL as i32)?;
            for &r in rels {
                fd.ioctl_int(UI_SET_RELBIT, r as i32)?;
            }
        }

        // SAFETY: uinput_setup is plain data.
        let mut setup: libc::uinput_setup = unsafe { std::mem::zeroed() };
        setup.id = libc::input_id { bustype: 0x06 /* BUS_VIRTUAL */, vendor: 0x1209, product: 0x4d41, version: 1 };
        for (slot, b) in setup.name.iter_mut().zip(name.bytes().take(79)) {
            *slot = b as libc::c_char;
        }
        fd.ioctl_ptr(UI_DEV_SETUP, &mut setup)?;
        fd.ioctl_none(UI_DEV_CREATE)?;
        Ok(VirtualDevice { fd })
    }

    pub fn raw_fd(&self) -> RawFd {
        self.fd.0
    }

    pub fn emit(&self, kind: u16, code: u16, value: i32) -> io::Result<()> {
        write_event(&self.fd, kind, code, value)
    }

    pub fn sync(&self) -> io::Result<()> {
        self.emit(EV_SYN, 0, 0)
    }

    /// Key down or up, followed by a sync so the desktop sees it immediately.
    pub fn key(&self, code: u16, down: bool) -> io::Result<()> {
        self.emit(EV_KEY, code, down as i32)?;
        self.sync()
    }
}

impl Drop for VirtualDevice {
    fn drop(&mut self) {
        let _ = self.fd.ioctl_none(UI_DEV_DESTROY);
    }
}

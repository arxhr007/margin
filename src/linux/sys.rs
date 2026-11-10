//! Thin, safe-ish wrappers over the Linux input ioctls: reading touchpads (evdev) and creating
//! virtual keyboards and mice (uinput).

use std::ffi::CString;
use std::io;
use std::os::raw::{c_int, c_ulong, c_void};
use std::os::unix::io::RawFd;

// ---- ioctl request numbers (asm-generic layout, used by x86 and arm) ----
const IOC_NONE: u32 = 0;
const IOC_WRITE: u32 = 1;
const IOC_READ: u32 = 2;

const fn ioc(dir: u32, ty: u8, nr: u32, size: u32) -> c_ulong {
    ((dir << 30) | (size << 16) | ((ty as u32) << 8) | nr) as c_ulong
}

pub const fn eviocgname(len: u32) -> c_ulong {
    ioc(IOC_READ, b'E', 0x06, len)
}
pub const fn eviocgprop(len: u32) -> c_ulong {
    ioc(IOC_READ, b'E', 0x09, len)
}
pub const fn eviocgbit(ev: u32, len: u32) -> c_ulong {
    ioc(IOC_READ, b'E', 0x20 + ev, len)
}
pub const fn eviocgabs(abs: u32) -> c_ulong {
    ioc(IOC_READ, b'E', 0x40 + abs, std::mem::size_of::<libc::input_absinfo>() as u32)
}

pub const UI_DEV_CREATE: c_ulong = ioc(IOC_NONE, b'U', 1, 0);
pub const UI_DEV_DESTROY: c_ulong = ioc(IOC_NONE, b'U', 2, 0);
pub const UI_DEV_SETUP: c_ulong = ioc(IOC_WRITE, b'U', 3, std::mem::size_of::<libc::uinput_setup>() as u32);
pub const UI_SET_EVBIT: c_ulong = ioc(IOC_WRITE, b'U', 100, 4);
pub const UI_SET_KEYBIT: c_ulong = ioc(IOC_WRITE, b'U', 101, 4);
pub const UI_SET_RELBIT: c_ulong = ioc(IOC_WRITE, b'U', 102, 4);

// event types / codes used here
pub const EV_SYN: u16 = 0x00;
pub const EV_KEY: u16 = 0x01;
pub const EV_REL: u16 = 0x02;
pub const EV_ABS: u16 = 0x03;
pub const REL_X: u16 = 0x00;
pub const REL_Y: u16 = 0x01;
pub const REL_HWHEEL: u16 = 0x06;
pub const REL_WHEEL: u16 = 0x08;
pub const REL_WHEEL_HI_RES: u16 = 0x0b;
pub const REL_HWHEEL_HI_RES: u16 = 0x0c;
pub const BTN_LEFT: u16 = 0x110;
pub const BTN_RIGHT: u16 = 0x111;
pub const BTN_MIDDLE: u16 = 0x112;
pub const INPUT_PROP_POINTER: usize = 0;
pub const INPUT_PROP_DIRECT: usize = 1;
pub const INPUT_PROP_SEMI_MT: usize = 3;

/// A file descriptor closed on drop.
pub struct Fd(pub RawFd);

impl Fd {
    pub fn open(path: &str, flags: c_int) -> io::Result<Fd> {
        let c = CString::new(path).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        // SAFETY: `c` is a valid NUL-terminated string.
        let fd = unsafe { libc::open(c.as_ptr(), flags | libc::O_CLOEXEC) };
        if fd < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(Fd(fd))
        }
    }

    pub fn ioctl_ptr<T>(&self, request: c_ulong, arg: *mut T) -> io::Result<()> {
        // SAFETY: callers pass a pointer to a value of the type the request expects.
        let r = unsafe { libc::ioctl(self.0, request as _, arg as *mut c_void) };
        if r < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    pub fn ioctl_int(&self, request: c_ulong, value: c_int) -> io::Result<()> {
        // SAFETY: the request takes a plain integer argument.
        let r = unsafe { libc::ioctl(self.0, request as _, value) };
        if r < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    pub fn ioctl_none(&self, request: c_ulong) -> io::Result<()> {
        // SAFETY: the request takes no argument.
        let r = unsafe { libc::ioctl(self.0, request as _) };
        if r < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

impl Drop for Fd {
    fn drop(&mut self) {
        // SAFETY: we own the descriptor.
        unsafe {
            libc::close(self.0);
        }
    }
}

pub fn test_bit(bits: &[u8], bit: usize) -> bool {
    bits.get(bit / 8).is_some_and(|b| b & (1 << (bit % 8)) != 0)
}

/// Capability bits of an event type (`EV_KEY`, `EV_ABS`...), or all zeros when the query fails.
pub fn event_bits(fd: &Fd, ev: u16, bytes: usize) -> Vec<u8> {
    let mut buf = vec![0u8; bytes];
    let _ = fd.ioctl_ptr(eviocgbit(ev as u32, bytes as u32), buf.as_mut_ptr());
    buf
}

pub fn device_name(fd: &Fd) -> String {
    let mut buf = [0u8; 256];
    if fd.ioctl_ptr(eviocgname(buf.len() as u32), buf.as_mut_ptr()).is_err() {
        return String::new();
    }
    let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..len]).into_owned()
}

pub fn device_props(fd: &Fd) -> Vec<u8> {
    let mut buf = vec![0u8; 8];
    let _ = fd.ioctl_ptr(eviocgprop(buf.len() as u32), buf.as_mut_ptr());
    buf
}

pub fn abs_info(fd: &Fd, axis: u16) -> Option<libc::input_absinfo> {
    // SAFETY: input_absinfo is plain data; the kernel fills it.
    let mut info: libc::input_absinfo = unsafe { std::mem::zeroed() };
    fd.ioctl_ptr(eviocgabs(axis as u32), &mut info).ok()?;
    Some(info)
}

/// Reads as many pending events as fit in `buf`. `Ok(0)` means nothing is waiting right now.
pub fn read_events(fd: &Fd, buf: &mut [libc::input_event]) -> io::Result<usize> {
    let size = std::mem::size_of::<libc::input_event>();
    // SAFETY: `buf` is valid for writes of `buf.len() * size` bytes.
    let n = unsafe { libc::read(fd.0, buf.as_mut_ptr() as *mut c_void, buf.len() * size) };
    if n < 0 {
        let e = io::Error::last_os_error();
        return if e.kind() == io::ErrorKind::WouldBlock { Ok(0) } else { Err(e) };
    }
    Ok(n as usize / size)
}

pub fn write_event(fd: &Fd, kind: u16, code: u16, value: i32) -> io::Result<()> {
    let ev = libc::input_event { time: libc::timeval { tv_sec: 0, tv_usec: 0 }, type_: kind, code, value };
    let size = std::mem::size_of::<libc::input_event>();
    // SAFETY: `ev` is a plain struct of exactly `size` bytes.
    let n = unsafe { libc::write(fd.0, &ev as *const _ as *const c_void, size) };
    if n as usize == size {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ioctl_numbers_match_the_kernel_headers() {
        // values taken from linux/input.h and linux/uinput.h on x86-64
        assert_eq!(UI_DEV_CREATE, 0x5501);
        assert_eq!(UI_DEV_DESTROY, 0x5502);
        assert_eq!(UI_SET_EVBIT, 0x4004_5564);
        assert_eq!(UI_SET_KEYBIT, 0x4004_5565);
        assert_eq!(UI_SET_RELBIT, 0x4004_5566);
        assert_eq!(UI_DEV_SETUP, 0x405c_5503);
        assert_eq!(eviocgbit(0, 4), 0x8004_4520);
        assert_eq!(eviocgabs(0x35), 0x8018_4575);
        assert_eq!(eviocgname(256), 0x8100_4506);
    }

    #[test]
    fn bit_test() {
        let bits = [0b0000_0101u8, 0b1000_0000];
        assert!(test_bit(&bits, 0));
        assert!(!test_bit(&bits, 1));
        assert!(test_bit(&bits, 2));
        assert!(test_bit(&bits, 15));
        assert!(!test_bit(&bits, 99));
    }
}

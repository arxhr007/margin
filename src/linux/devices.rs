//! Finding and reading touchpads under /dev/input.

use std::io;

use super::sys::*;
use crate::touchpad::evdev::{self, AxisRange, Frame, Parser};

pub struct Touchpad {
    pub id: u32,
    pub path: String,
    pub name: String,
    pub has_button: bool,
    pub fd: Fd,
    parser: Parser,
}

/// Why a device was skipped, for the log.
pub enum Probe {
    Touchpad(Touchpad),
    NotATouchpad,
    Denied(io::Error),
}

pub fn probe(id: u32, path: &str) -> Probe {
    let fd = match Fd::open(path, libc::O_RDONLY | libc::O_NONBLOCK) {
        Ok(fd) => fd,
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => return Probe::Denied(e),
        Err(_) => return Probe::NotATouchpad,
    };
    let name = device_name(&fd);
    if name.starts_with("Margin virtual") {
        return Probe::NotATouchpad; // our own output devices
    }

    let types = event_bits(&fd, 0, 4);
    if !test_bit(&types, EV_ABS as usize) {
        return Probe::NotATouchpad;
    }
    let abs = event_bits(&fd, EV_ABS, 8);
    let multitouch = [evdev::ABS_MT_SLOT, evdev::ABS_MT_POSITION_X, evdev::ABS_MT_POSITION_Y]
        .iter()
        .all(|&a| test_bit(&abs, a as usize));
    if !multitouch {
        return Probe::NotATouchpad;
    }
    let props = device_props(&fd);
    // touchscreens report positions in screen space; "semi-MT" pads only report a bounding box
    if test_bit(&props, INPUT_PROP_DIRECT) || test_bit(&props, INPUT_PROP_SEMI_MT) {
        return Probe::NotATouchpad;
    }
    if !test_bit(&props, INPUT_PROP_POINTER) && !name.to_lowercase().contains("touchpad") {
        return Probe::NotATouchpad;
    }

    let (Some(x), Some(y), Some(slot)) = (
        abs_info(&fd, evdev::ABS_MT_POSITION_X),
        abs_info(&fd, evdev::ABS_MT_POSITION_Y),
        abs_info(&fd, evdev::ABS_MT_SLOT),
    ) else {
        return Probe::NotATouchpad;
    };
    let keys = event_bits(&fd, EV_KEY, 96);
    let parser = Parser::new(
        AxisRange { min: x.minimum, max: x.maximum },
        AxisRange { min: y.minimum, max: y.maximum },
        (slot.maximum.max(0) as usize) + 1,
    );
    Probe::Touchpad(Touchpad {
        id,
        path: path.to_string(),
        name,
        has_button: test_bit(&keys, BTN_LEFT as usize),
        fd,
        parser,
    })
}

/// All `/dev/input/eventN` paths.
pub fn event_paths() -> Vec<String> {
    let mut paths: Vec<String> = std::fs::read_dir("/dev/input")
        .map(|d| {
            d.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.starts_with("event"))
                .map(|n| format!("/dev/input/{n}"))
                .collect()
        })
        .unwrap_or_default();
    paths.sort();
    paths
}

impl Touchpad {
    /// Reads everything waiting on the device and hands each complete frame to `on_frame`.
    /// Returns an error when the device is gone (unplugged, or the machine resumed with a new node).
    pub fn pump(&mut self, mut on_frame: impl FnMut(Frame)) -> io::Result<()> {
        // SAFETY: input_event is plain data.
        let mut buf: [libc::input_event; 64] = unsafe { std::mem::zeroed() };
        loop {
            let n = read_events(&self.fd, &mut buf)?;
            if n == 0 {
                return Ok(());
            }
            for ev in &buf[..n] {
                if let Some(frame) = self.parser.feed(ev.type_, ev.code, ev.value) {
                    on_frame(frame);
                }
            }
        }
    }
}

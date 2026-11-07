//! Reads finger positions and the physical button from Windows Precision Touchpads via Raw Input.
//!
//! A precision touchpad is a HID digitizer (usage page 0x0D, usage 0x05). Each finger is its own link
//! collection holding Contact ID, Tip Switch, X and Y. Reports can carry all fingers at once or be split
//! across several reports ("hybrid" mode), where only the first report has a non-zero Contact Count.

use std::collections::HashMap;
use std::ffi::c_void;

use windows::Win32::Devices::HumanInterfaceDevice::*;
use windows::Win32::Foundation::{HANDLE, HWND, LPARAM};
use windows::Win32::UI::Input::*;

use super::tracker::TouchTracker;

const PAGE_GENERIC: u16 = 0x01;
const USAGE_X: u16 = 0x30;
const USAGE_Y: u16 = 0x31;
const PAGE_DIGITIZER: u16 = 0x0D;
const USAGE_TOUCHPAD: u16 = 0x05;
const USAGE_TIP: u16 = 0x42;
const USAGE_CONTACT_ID: u16 = 0x51;
const USAGE_CONTACT_COUNT: u16 = 0x54;
const PAGE_BUTTON: u16 = 0x09;

#[derive(Clone, Copy, Debug)]
struct Axis {
    min: i32,
    max: i32,
}

impl Axis {
    fn normalize(self, raw: u32) -> f32 {
        if self.max > self.min {
            (((raw as i64 - self.min as i64) as f32) / ((self.max - self.min) as f32)).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Finger {
    link: u16,
    x: Axis,
    y: Axis,
}

struct Device {
    /// HID preparsed data; kept as u64s so the buffer is 8-byte aligned.
    preparsed: Vec<u64>,
    fingers: Vec<Finger>,
    has_contact_count: bool,
    has_button: bool,
    name: String,

    // hybrid-mode frame assembly
    expected: usize,
    down: Vec<(i32, f32, f32)>,
    collected: usize,
    button: bool,
}

impl Device {
    fn preparsed(&self) -> PHIDP_PREPARSED_DATA {
        PHIDP_PREPARSED_DATA(self.preparsed.as_ptr() as isize)
    }
}

#[derive(Default)]
pub struct RawTouchpad {
    devices: HashMap<isize, Option<Device>>,
    active_device: isize,
    buffer: Vec<u64>,
    last_diag: u64,
}

impl RawTouchpad {
    pub fn new() -> Self {
        Self::default()
    }

    /// Asks Windows to deliver touchpad reports (also while another window has focus) to `hwnd`.
    pub fn register(hwnd: HWND) {
        let rid = [RAWINPUTDEVICE {
            usUsagePage: PAGE_DIGITIZER,
            usUsage: USAGE_TOUCHPAD,
            dwFlags: RIDEV_INPUTSINK | RIDEV_DEVNOTIFY,
            hwndTarget: hwnd,
        }];
        match unsafe { RegisterRawInputDevices(&rid, std::mem::size_of::<RAWINPUTDEVICE>() as u32) } {
            Ok(()) => crate::log::info("listening for precision touchpad input"),
            Err(e) => crate::log::error(format!("RegisterRawInputDevices failed: {e}")),
        }
    }

    pub fn device_name(&self) -> Option<&str> {
        self.devices.values().flatten().next().map(|d| d.name.as_str())
    }

    /// A touchpad was added or removed: forget what we knew about it.
    pub fn device_changed(&mut self, device: isize, tracker: &mut TouchTracker) {
        self.devices.remove(&device);
        if self.active_device == device {
            self.active_device = 0;
            tracker.clear();
        }
    }

    /// Handles one WM_INPUT message. `after_frame` runs after each complete touch frame is committed.
    pub fn process(&mut self, lparam: LPARAM, tracker: &mut TouchTracker, mut after_frame: impl FnMut(&TouchTracker)) {
        let handle = HRAWINPUT(lparam.0 as *mut c_void);
        let header_size = std::mem::size_of::<RAWINPUTHEADER>() as u32;
        let mut size = 0u32;
        unsafe { GetRawInputData(handle, RID_INPUT, None, &mut size, header_size) };
        if size == 0 {
            return;
        }
        let words = (size as usize).div_ceil(8);
        if self.buffer.len() < words {
            self.buffer.resize(words, 0);
        }
        let got = unsafe {
            GetRawInputData(handle, RID_INPUT, Some(self.buffer.as_mut_ptr() as *mut c_void), &mut size, header_size)
        };
        if got == u32::MAX {
            return;
        }

        // SAFETY: the buffer holds a RAWINPUT written by GetRawInputData and is 8-byte aligned.
        let raw = unsafe { &*(self.buffer.as_ptr() as *const RAWINPUT) };
        if raw.header.dwType != RIM_TYPEHID.0 {
            return;
        }
        let h_device = raw.header.hDevice;
        let key = h_device.0 as isize;

        if !self.devices.contains_key(&key) {
            let described = describe(h_device);
            if let Some(d) = &described {
                tracker.device_has_button = d.has_button;
                crate::log::info(format!(
                    "touchpad: {} \u{2014} {} contact slots, button={}, contactCount={}",
                    d.name,
                    d.fingers.len(),
                    d.has_button,
                    d.has_contact_count
                ));
                for f in &d.fingers {
                    crate::debug!("  slot link={} x={}..{} y={}..{}", f.link, f.x.min, f.x.max, f.y.min, f.y.max);
                }
            }
            self.devices.insert(key, described);
        }

        // With two touchpads, the one touched first is in charge until all its fingers lift.
        if self.active_device != 0 && self.active_device != key && tracker.count() > 0 {
            return;
        }

        // SAFETY: for HID input the `hid` union member is the active one.
        let (report_size, report_count, data) = unsafe {
            let hid = &raw.data.hid;
            (hid.dwSizeHid as usize, hid.dwCount as usize, hid.bRawData.as_ptr())
        };
        if report_size == 0 {
            return;
        }
        let mut report = vec![0u8; report_size];
        for r in 0..report_count {
            // SAFETY: Windows lays the `dwCount` reports of `dwSizeHid` bytes back to back.
            unsafe { std::ptr::copy_nonoverlapping(data.add(r * report_size), report.as_mut_ptr(), report_size) };
            let Some(Some(device)) = self.devices.get_mut(&key) else { return };
            if let Some(committed) = parse_report(device, &mut report) {
                self.active_device = if committed.0 { key } else { 0 };
                tracker.commit(&committed.1, committed.2);
                self.log_frame(tracker);
                after_frame(tracker);
            }
        }
    }

    fn log_frame(&mut self, tracker: &TouchTracker) {
        if !crate::log::verbose() {
            return;
        }
        let now = crate::now_ms();
        if now.saturating_sub(self.last_diag) < 60 && !tracker.button_down {
            return;
        }
        self.last_diag = now;
        let parts: Vec<String> = tracker
            .snapshot()
            .iter()
            .map(|t| {
                format!(
                    "#{} ({:.2},{:.2}) from ({:.2},{:.2})",
                    t.id, t.current.x, t.current.y, t.start.x, t.start.y
                )
            })
            .collect();
        crate::log::debug(format!(
            "frame btn={} n={} {}",
            tracker.button_down as i32,
            tracker.count(),
            parts.join(" ")
        ));
    }
}

/// Reads one HID report. Returns `(any_finger_down, fingers, button)` when a complete frame is ready.
fn parse_report(d: &mut Device, report: &mut [u8]) -> Option<(bool, Vec<(i32, f32, f32)>, bool)> {
    let pre = d.preparsed();
    let mut usages = [0u16; 64];

    let mut button = false;
    if d.has_button {
        let mut n = usages.len() as u32;
        let status = unsafe { HidP_GetUsages(HidP_Input, PAGE_BUTTON, 0, usages.as_mut_ptr(), &mut n, pre, report) };
        if status == HIDP_STATUS_SUCCESS {
            button = n > 0;
        }
    }

    let mut count: i32 = -1;
    if d.has_contact_count {
        let mut c = 0u32;
        let status = unsafe { HidP_GetUsageValue(HidP_Input, PAGE_DIGITIZER, 0, USAGE_CONTACT_COUNT, &mut c, pre, report) };
        if status == HIDP_STATUS_SUCCESS {
            count = c as i32;
        }
    }

    if count > 0 || count == -1 || d.expected == 0 {
        // first report of a new frame
        d.expected = if count < 0 { d.fingers.len() } else { count as usize };
        d.down.clear();
        d.collected = 0;
        d.button = false;
    }
    d.button |= button;

    let fingers = d.fingers.clone();
    for f in &fingers {
        if d.collected >= d.expected {
            break;
        }
        let (mut id, mut rx, mut ry) = (0u32, 0u32, 0u32);
        let ok = unsafe {
            HidP_GetUsageValue(HidP_Input, PAGE_DIGITIZER, f.link, USAGE_CONTACT_ID, &mut id, pre, report) == HIDP_STATUS_SUCCESS
                && HidP_GetUsageValue(HidP_Input, PAGE_GENERIC, f.link, USAGE_X, &mut rx, pre, report) == HIDP_STATUS_SUCCESS
                && HidP_GetUsageValue(HidP_Input, PAGE_GENERIC, f.link, USAGE_Y, &mut ry, pre, report) == HIDP_STATUS_SUCCESS
        };
        if !ok {
            continue;
        }

        let mut n = usages.len() as u32;
        let tip = unsafe {
            HidP_GetUsages(HidP_Input, PAGE_DIGITIZER, f.link, usages.as_mut_ptr(), &mut n, pre, report) == HIDP_STATUS_SUCCESS
        } && usages[..n as usize].contains(&USAGE_TIP);

        d.collected += 1;
        // Windows reports Y growing downward; flip so y = 1 is the top edge.
        if tip {
            d.down.push((id as i32, f.x.normalize(rx), 1.0 - f.y.normalize(ry)));
        }
    }

    if d.collected >= d.expected {
        d.expected = 0;
        return Some((!d.down.is_empty(), std::mem::take(&mut d.down), d.button));
    }
    None
}

/// Reads the touchpad's HID description: finger slots with their X/Y ranges, contact count, button.
fn describe(h_device: HANDLE) -> Option<Device> {
    let mut size = 0u32;
    unsafe { GetRawInputDeviceInfoW(h_device, RIDI_PREPARSEDDATA, None, &mut size) };
    if size == 0 {
        return None;
    }
    let mut preparsed = vec![0u64; (size as usize).div_ceil(8)];
    let got = unsafe {
        GetRawInputDeviceInfoW(h_device, RIDI_PREPARSEDDATA, Some(preparsed.as_mut_ptr() as *mut c_void), &mut size)
    };
    if got == u32::MAX {
        return None;
    }

    let mut name_chars = 0u32;
    unsafe { GetRawInputDeviceInfoW(h_device, RIDI_DEVICENAME, None, &mut name_chars) };
    let mut name = String::new();
    if name_chars > 0 {
        let mut buf = vec![0u16; name_chars as usize];
        unsafe { GetRawInputDeviceInfoW(h_device, RIDI_DEVICENAME, Some(buf.as_mut_ptr() as *mut c_void), &mut name_chars) };
        name = String::from_utf16_lossy(&buf).trim_end_matches('\0').to_string();
    }

    let mut d = Device {
        preparsed,
        fingers: Vec::new(),
        has_contact_count: false,
        has_button: false,
        name,
        expected: 0,
        down: Vec::new(),
        collected: 0,
        button: false,
    };
    let pre = d.preparsed();

    let mut caps = HIDP_CAPS::default();
    if unsafe { HidP_GetCaps(pre, &mut caps) } != HIDP_STATUS_SUCCESS {
        return None;
    }

    // Value caps: find X/Y per link collection, and the top-level contact count.
    let mut xs: HashMap<u16, Axis> = HashMap::new();
    let mut ys: HashMap<u16, Axis> = HashMap::new();
    let mut ids: Vec<u16> = Vec::new();
    if caps.NumberInputValueCaps > 0 {
        let mut vc = vec![HIDP_VALUE_CAPS::default(); caps.NumberInputValueCaps as usize];
        let mut n = caps.NumberInputValueCaps;
        if unsafe { HidP_GetValueCaps(HidP_Input, vc.as_mut_ptr(), &mut n, pre) } == HIDP_STATUS_SUCCESS {
            for c in &vc[..n as usize] {
                let is_range = c.IsRange.as_bool();
                // SAFETY: both union members start with the usage (range min / single usage); max only when IsRange.
                let (u_min, u_max) = unsafe {
                    let min = c.Anonymous.Range.UsageMin;
                    (min, if is_range { c.Anonymous.Range.UsageMax } else { min })
                };
                let (min, mut max) = (c.LogicalMin, c.LogicalMax);
                if max <= min && c.BitSize > 0 && c.BitSize < 32 {
                    max = (1i32 << c.BitSize) - 1; // unsigned field declared oddly
                }
                let has = |usage: u16| usage >= u_min && usage <= u_max;
                let link = c.LinkCollection;
                if c.UsagePage == PAGE_GENERIC && has(USAGE_X) {
                    xs.insert(link, Axis { min, max });
                }
                if c.UsagePage == PAGE_GENERIC && has(USAGE_Y) {
                    ys.insert(link, Axis { min, max });
                }
                if c.UsagePage == PAGE_DIGITIZER && has(USAGE_CONTACT_ID) {
                    ids.push(link);
                }
                if c.UsagePage == PAGE_DIGITIZER && has(USAGE_CONTACT_COUNT) {
                    d.has_contact_count = true;
                }
            }
        }
    }
    let mut links: Vec<u16> = xs.keys().copied().collect();
    links.sort_unstable();
    for link in links {
        if let (Some(&y), true) = (ys.get(&link), ids.contains(&link)) {
            d.fingers.push(Finger { link, x: xs[&link], y });
        }
    }

    if caps.NumberInputButtonCaps > 0 {
        let mut bc = vec![HIDP_BUTTON_CAPS::default(); caps.NumberInputButtonCaps as usize];
        let mut n = caps.NumberInputButtonCaps;
        if unsafe { HidP_GetButtonCaps(HidP_Input, bc.as_mut_ptr(), &mut n, pre) } == HIDP_STATUS_SUCCESS {
            d.has_button = bc[..n as usize].iter().any(|c| c.UsagePage == PAGE_BUTTON);
        }
    }

    if d.fingers.is_empty() {
        return None;
    }
    Some(d)
}

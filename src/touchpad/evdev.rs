//! Turns raw Linux evdev events from a multitouch touchpad (kernel "protocol B") into touch frames.
//! Pure: no OS calls, so it is unit-tested on every platform even though only Linux uses it.
//!
//! A touchpad reports a stream of `(type, code, value)` events. `ABS_MT_SLOT` selects which finger
//! the following axis events belong to, `ABS_MT_TRACKING_ID` starts (>= 0) or ends (-1) a touch, and
//! `SYN_REPORT` marks the end of one consistent snapshot.

pub const EV_SYN: u16 = 0x00;
pub const EV_KEY: u16 = 0x01;
pub const EV_ABS: u16 = 0x03;

pub const SYN_REPORT: u16 = 0;
pub const SYN_DROPPED: u16 = 3;

pub const ABS_MT_SLOT: u16 = 0x2f;
pub const ABS_MT_POSITION_X: u16 = 0x35;
pub const ABS_MT_POSITION_Y: u16 = 0x36;
pub const ABS_MT_TRACKING_ID: u16 = 0x39;

pub const BTN_LEFT: u16 = 0x110;

/// The reported range of one axis (`input_absinfo` minimum and maximum).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxisRange {
    pub min: i32,
    pub max: i32,
}

impl AxisRange {
    fn normalize(self, raw: i32) -> f32 {
        if self.max > self.min {
            (((raw as i64 - self.min as i64) as f32) / ((self.max - self.min) as f32)).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

/// One snapshot of the touchpad: fingers that are down (id, x, y with y = 1 at the top) and the button.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Frame {
    pub fingers: Vec<(i32, f32, f32)>,
    pub button: bool,
}

#[derive(Clone, Copy, Debug)]
struct Slot {
    id: i32, // -1 = no finger
    x: i32,
    y: i32,
    has_x: bool,
    has_y: bool,
}

const EMPTY: Slot = Slot { id: -1, x: 0, y: 0, has_x: false, has_y: false };
/// Real touchpads track 2 to 10 fingers; this just guards against nonsense slot numbers.
const MAX_SLOTS: usize = 64;

pub struct Parser {
    x: AxisRange,
    y: AxisRange,
    slots: Vec<Slot>,
    current: usize,
    button: bool,
    dirty: bool,
}

impl Parser {
    pub fn new(x: AxisRange, y: AxisRange, slot_count: usize) -> Self {
        Parser { x, y, slots: vec![EMPTY; slot_count.clamp(1, MAX_SLOTS)], current: 0, button: false, dirty: false }
    }

    /// Feeds one event. Returns a frame at the end of each report that changed something.
    pub fn feed(&mut self, kind: u16, code: u16, value: i32) -> Option<Frame> {
        match kind {
            EV_ABS => self.abs(code, value),
            EV_KEY if code == BTN_LEFT => {
                let down = value != 0;
                if down != self.button {
                    self.button = down;
                    self.dirty = true;
                }
            }
            EV_SYN if code == SYN_DROPPED => {
                // the kernel's buffer overflowed: what we know is stale, start over
                self.slots.iter_mut().for_each(|s| *s = EMPTY);
                self.dirty = true;
            }
            EV_SYN if code == SYN_REPORT => {
                if std::mem::take(&mut self.dirty) {
                    return Some(self.frame());
                }
            }
            _ => {}
        }
        None
    }

    fn abs(&mut self, code: u16, value: i32) {
        match code {
            ABS_MT_SLOT => {
                let wanted = value.max(0) as usize;
                if wanted < MAX_SLOTS {
                    if wanted >= self.slots.len() {
                        self.slots.resize(wanted + 1, EMPTY);
                    }
                    self.current = wanted;
                }
            }
            ABS_MT_TRACKING_ID => {
                // Only the id changes. The kernel reports an axis only when it changed and remembers each
                // slot's last value, so a new touch that starts at the same X (or Y) as the slot's previous
                // touch sends no event for that axis: the old coordinate is still the right one.
                self.slots[self.current].id = if value < 0 { -1 } else { value };
                self.dirty = true;
            }
            ABS_MT_POSITION_X => {
                let slot = &mut self.slots[self.current];
                slot.x = value;
                slot.has_x = true;
                self.dirty = true;
            }
            ABS_MT_POSITION_Y => {
                let slot = &mut self.slots[self.current];
                slot.y = value;
                slot.has_y = true;
                self.dirty = true;
            }
            _ => {}
        }
    }

    fn frame(&self) -> Frame {
        let fingers = self
            .slots
            .iter()
            .filter(|s| s.id >= 0 && s.has_x && s.has_y)
            // the kernel reports y growing downward; flip so y = 1 is the top edge, like on Windows
            .map(|s| (s.id, self.x.normalize(s.x), 1.0 - self.y.normalize(s.y)))
            .collect();
        Frame { fingers, button: self.button }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parser() -> Parser {
        Parser::new(AxisRange { min: 0, max: 1000 }, AxisRange { min: 0, max: 500 }, 5)
    }

    /// Feeds a whole report and returns the frame at its SYN_REPORT.
    fn report(p: &mut Parser, events: &[(u16, u16, i32)]) -> Option<Frame> {
        for &(t, c, v) in events {
            assert!(p.feed(t, c, v).is_none(), "frame before SYN_REPORT");
        }
        p.feed(EV_SYN, SYN_REPORT, 0)
    }

    #[test]
    fn a_finger_down_move_and_up() {
        let mut p = parser();
        let f = report(
            &mut p,
            &[(EV_ABS, ABS_MT_SLOT, 0), (EV_ABS, ABS_MT_TRACKING_ID, 41), (EV_ABS, ABS_MT_POSITION_X, 100), (EV_ABS, ABS_MT_POSITION_Y, 100)],
        )
        .unwrap();
        assert_eq!(f.fingers.len(), 1);
        assert_eq!(f.fingers[0].0, 41);
        assert!((f.fingers[0].1 - 0.1).abs() < 1e-6);
        assert!((f.fingers[0].2 - 0.8).abs() < 1e-6, "y is flipped: 100 of 500 down = 0.8 up");

        let f = report(&mut p, &[(EV_ABS, ABS_MT_POSITION_X, 500)]).unwrap();
        assert!((f.fingers[0].1 - 0.5).abs() < 1e-6);
        assert!((f.fingers[0].2 - 0.8).abs() < 1e-6, "unchanged axes keep their value");

        let f = report(&mut p, &[(EV_ABS, ABS_MT_TRACKING_ID, -1)]).unwrap();
        assert!(f.fingers.is_empty());
    }

    #[test]
    fn nothing_changed_means_no_frame() {
        let mut p = parser();
        assert!(p.feed(EV_SYN, SYN_REPORT, 0).is_none());
        report(&mut p, &[(EV_ABS, ABS_MT_TRACKING_ID, 1), (EV_ABS, ABS_MT_POSITION_X, 1), (EV_ABS, ABS_MT_POSITION_Y, 1)]).unwrap();
        assert!(p.feed(EV_SYN, SYN_REPORT, 0).is_none());
    }

    #[test]
    fn two_fingers_in_different_slots() {
        let mut p = parser();
        let f = report(
            &mut p,
            &[
                (EV_ABS, ABS_MT_SLOT, 0),
                (EV_ABS, ABS_MT_TRACKING_ID, 10),
                (EV_ABS, ABS_MT_POSITION_X, 0),
                (EV_ABS, ABS_MT_POSITION_Y, 0),
                (EV_ABS, ABS_MT_SLOT, 1),
                (EV_ABS, ABS_MT_TRACKING_ID, 11),
                (EV_ABS, ABS_MT_POSITION_X, 1000),
                (EV_ABS, ABS_MT_POSITION_Y, 500),
            ],
        )
        .unwrap();
        assert_eq!(f.fingers.len(), 2);
        assert_eq!(f.fingers[0], (10, 0.0, 1.0));
        assert_eq!(f.fingers[1], (11, 1.0, 0.0));

        // lift the first finger only
        let f = report(&mut p, &[(EV_ABS, ABS_MT_SLOT, 0), (EV_ABS, ABS_MT_TRACKING_ID, -1)]).unwrap();
        assert_eq!(f.fingers.len(), 1);
        assert_eq!(f.fingers[0].0, 11);
    }

    #[test]
    fn a_slot_reused_by_a_new_touch_gets_the_new_id() {
        let mut p = parser();
        report(&mut p, &[(EV_ABS, ABS_MT_TRACKING_ID, 5), (EV_ABS, ABS_MT_POSITION_X, 900), (EV_ABS, ABS_MT_POSITION_Y, 400)]).unwrap();
        let f = report(&mut p, &[(EV_ABS, ABS_MT_TRACKING_ID, -1)]).unwrap();
        assert!(f.fingers.is_empty());
        // the new touch reports new coordinates
        let f = report(&mut p, &[(EV_ABS, ABS_MT_TRACKING_ID, 6), (EV_ABS, ABS_MT_POSITION_X, 100), (EV_ABS, ABS_MT_POSITION_Y, 50)]).unwrap();
        assert_eq!(f.fingers.len(), 1);
        assert_eq!(f.fingers[0].0, 6);
        assert!((f.fingers[0].1 - 0.1).abs() < 1e-6);
        assert!((f.fingers[0].2 - 0.9).abs() < 1e-6);
    }

    #[test]
    fn unchanged_axes_are_not_reported_for_a_new_touch() {
        // The kernel omits an axis whose value equals the slot's previous one, even across touches.
        let mut p = parser();
        report(&mut p, &[(EV_ABS, ABS_MT_TRACKING_ID, 5), (EV_ABS, ABS_MT_POSITION_X, 10), (EV_ABS, ABS_MT_POSITION_Y, 250)]).unwrap();
        report(&mut p, &[(EV_ABS, ABS_MT_TRACKING_ID, -1)]).unwrap();
        // new touch at the same X, a different Y: only Y is sent
        let f = report(&mut p, &[(EV_ABS, ABS_MT_TRACKING_ID, 6), (EV_ABS, ABS_MT_POSITION_Y, 100)]).unwrap();
        assert_eq!(f.fingers.len(), 1, "the finger must not vanish because X was not repeated");
        assert!((f.fingers[0].1 - 0.01).abs() < 1e-6, "X is the slot's remembered value");
        assert!((f.fingers[0].2 - 0.8).abs() < 1e-6);
        // and at the same X and the same Y: no coordinate events at all
        report(&mut p, &[(EV_ABS, ABS_MT_TRACKING_ID, -1)]).unwrap();
        let f = report(&mut p, &[(EV_ABS, ABS_MT_TRACKING_ID, 7)]).unwrap();
        assert_eq!(f.fingers.len(), 1);
        assert!((f.fingers[0].1 - 0.01).abs() < 1e-6 && (f.fingers[0].2 - 0.8).abs() < 1e-6);
    }

    #[test]
    fn a_slot_that_never_reported_a_position_has_no_finger_yet() {
        let mut p = parser();
        let f = report(&mut p, &[(EV_ABS, ABS_MT_TRACKING_ID, 1)]).unwrap();
        assert!(f.fingers.is_empty());
    }

    #[test]
    fn the_button_is_reported_with_the_frame() {
        let mut p = parser();
        let f = report(&mut p, &[(EV_KEY, BTN_LEFT, 1)]).unwrap();
        assert!(f.button);
        assert!(f.fingers.is_empty());
        let f = report(&mut p, &[(EV_KEY, BTN_LEFT, 0)]).unwrap();
        assert!(!f.button);
        // other keys (BTN_TOUCH, BTN_TOOL_*) are not the click
        assert!(report(&mut p, &[(EV_KEY, 0x14a, 1)]).is_none());
    }

    #[test]
    fn values_outside_the_range_are_clamped() {
        let mut p = parser();
        let f = report(&mut p, &[(EV_ABS, ABS_MT_TRACKING_ID, 1), (EV_ABS, ABS_MT_POSITION_X, -50), (EV_ABS, ABS_MT_POSITION_Y, 9999)]).unwrap();
        assert_eq!(f.fingers[0], (1, 0.0, 0.0));
    }

    #[test]
    fn syn_dropped_forgets_everything() {
        let mut p = parser();
        report(&mut p, &[(EV_ABS, ABS_MT_TRACKING_ID, 3), (EV_ABS, ABS_MT_POSITION_X, 10), (EV_ABS, ABS_MT_POSITION_Y, 10), (EV_KEY, BTN_LEFT, 1)]).unwrap();
        assert!(p.feed(EV_SYN, SYN_DROPPED, 0).is_none());
        let f = p.feed(EV_SYN, SYN_REPORT, 0).unwrap();
        assert!(f.fingers.is_empty());
        assert!(f.button, "the button state is not part of what the overflow lost track of");
    }

    #[test]
    fn nonsense_slot_numbers_do_not_panic() {
        let mut p = parser();
        report(&mut p, &[(EV_ABS, ABS_MT_SLOT, 9999), (EV_ABS, ABS_MT_TRACKING_ID, 1)]).unwrap();
        report(&mut p, &[(EV_ABS, ABS_MT_SLOT, -3), (EV_ABS, ABS_MT_POSITION_X, 1)]).unwrap();
    }

    #[test]
    fn slots_grow_on_demand() {
        let mut p = Parser::new(AxisRange { min: 0, max: 100 }, AxisRange { min: 0, max: 100 }, 2);
        let f = report(&mut p, &[(EV_ABS, ABS_MT_SLOT, 7), (EV_ABS, ABS_MT_TRACKING_ID, 9), (EV_ABS, ABS_MT_POSITION_X, 50), (EV_ABS, ABS_MT_POSITION_Y, 50)]).unwrap();
        assert_eq!(f.fingers.len(), 1);
    }
}

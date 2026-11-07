//! Fingers currently on the touchpad plus the state of its physical button.

use crate::geometry::Pt;

/// One finger, in normalized 0..1 coordinates (origin bottom-left: y = 1 is the top edge).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TouchPoint {
    pub id: i32,
    pub start: Pt,
    pub current: Pt,
}

#[derive(Default, Debug)]
pub struct TouchTracker {
    touches: Vec<TouchPoint>,
    /// True while the touchpad reports its button pressed (a real click, not tap-to-click).
    pub button_down: bool,
    /// False for touchpads whose HID report has no button usage; then "finger is down" is the click test.
    pub device_has_button: bool,
}

impl TouchTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> &[TouchPoint] {
        &self.touches
    }

    pub fn count(&self) -> usize {
        self.touches.len()
    }

    /// Applies one complete frame: the fingers that are down right now (tip switch on).
    pub fn commit(&mut self, down: &[(i32, f32, f32)], button: bool) {
        self.button_down = button;
        for &(id, x, y) in down {
            let p = Pt::new(x, y);
            match self.touches.iter_mut().find(|t| t.id == id) {
                Some(existing) => existing.current = p,
                None => self.touches.push(TouchPoint { id, start: p, current: p }),
            }
        }
        self.touches.retain(|t| down.iter().any(|d| d.0 == t.id));
    }

    pub fn clear(&mut self) {
        self.touches.clear();
        self.button_down = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_start_and_updates_current() {
        let mut t = TouchTracker::new();
        t.commit(&[(1, 0.1, 0.2)], false);
        t.commit(&[(1, 0.3, 0.4)], true);
        let f = t.snapshot()[0];
        assert_eq!(f.start, Pt::new(0.1, 0.2));
        assert_eq!(f.current, Pt::new(0.3, 0.4));
        assert!(t.button_down);
    }

    #[test]
    fn lifted_fingers_disappear_and_new_ones_get_a_fresh_start() {
        let mut t = TouchTracker::new();
        t.commit(&[(1, 0.1, 0.1), (2, 0.9, 0.9)], false);
        t.commit(&[(2, 0.8, 0.8)], false);
        assert_eq!(t.count(), 1);
        t.commit(&[(2, 0.8, 0.8), (3, 0.5, 0.5)], false);
        assert_eq!(t.snapshot()[1].start, Pt::new(0.5, 0.5));
        t.commit(&[], false);
        assert_eq!(t.count(), 0);
    }
}

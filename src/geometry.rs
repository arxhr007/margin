//! Where each section sits on the touchpad. Coordinates are normalized 0..1 with y = 1 at the top.
//! Corners own their squares; edges run between the corners. The settings window draws exactly
//! these rectangles, so what you see there is what the touchpad does.

use crate::settings::{Section, Settings, SlideAction};

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Pt {
    pub x: f32,
    pub y: f32,
}

impl Pt {
    pub const fn new(x: f32, y: f32) -> Self {
        Pt { x, y }
    }
}

/// Rectangle in touchpad coordinates (y grows upward): `y` is the bottom edge.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn contains(&self, p: Pt) -> bool {
        p.x >= self.x && p.x <= self.x + self.w && p.y >= self.y && p.y <= self.y + self.h
    }
}

/// Draw/check order: corners first, so they win over the edges they overlap.
pub const ALL: [Section; 8] = [
    Section::TopLeft,
    Section::TopRight,
    Section::BottomLeft,
    Section::BottomRight,
    Section::Left,
    Section::Right,
    Section::Top,
    Section::Bottom,
];

pub fn bounds(s: Section, st: &Settings) -> Rect {
    let c = st.corner_size as f32;
    let d = st.edge_depth as f32;
    match s {
        Section::TopLeft => Rect { x: 0.0, y: 1.0 - c, w: c, h: c },
        Section::TopRight => Rect { x: 1.0 - c, y: 1.0 - c, w: c, h: c },
        Section::BottomLeft => Rect { x: 0.0, y: 0.0, w: c, h: c },
        Section::BottomRight => Rect { x: 1.0 - c, y: 0.0, w: c, h: c },
        Section::Left => Rect { x: 0.0, y: c, w: d, h: 1.0 - 2.0 * c },
        Section::Right => Rect { x: 1.0 - d, y: c, w: d, h: 1.0 - 2.0 * c },
        Section::Top => Rect { x: c, y: 1.0 - d, w: 1.0 - 2.0 * c, h: d },
        Section::Bottom => Rect { x: c, y: 0.0, w: 1.0 - 2.0 * c, h: d },
    }
}

/// The section under a point, assigned or not.
pub fn hit(p: Pt, st: &Settings) -> Option<Section> {
    ALL.into_iter().find(|&s| bounds(s, st).contains(p))
}

pub fn is_assigned(s: Section, st: &Settings) -> bool {
    let v = st.section(s);
    if s.is_corner() {
        v.corner_actions().next().is_some()
    } else {
        v.slide != SlideAction::Off
    }
}

/// Slide-in mode: the assigned edge, working by touch, whose thin outer strip contains a newborn finger.
pub fn entry_edge(p: Pt, st: &Settings) -> Option<Section> {
    let e = st.entry_strip as f32;
    let c = st.corner_size as f32;
    let along_side = p.y >= c && p.y <= 1.0 - c;
    let along_top = p.x >= c && p.x <= 1.0 - c;
    let s = if p.x <= e && along_side {
        Section::Left
    } else if p.x >= 1.0 - e && along_side {
        Section::Right
    } else if p.y <= e && along_top {
        Section::Bottom
    } else if p.y >= 1.0 - e && along_top {
        Section::Top
    } else {
        return None;
    };
    (is_assigned(s, st) && !st.needs_click(s)).then_some(s)
}

/// How far from its edge a finger may drift before an active slide ends.
pub fn in_corridor(s: Section, p: Pt, st: &Settings) -> bool {
    let limit = (st.edge_depth as f32).max(0.08) * 1.75;
    match s {
        Section::Left => p.x <= limit,
        Section::Right => p.x >= 1.0 - limit,
        Section::Bottom => p.y <= limit,
        _ => p.y >= 1.0 - limit, // Top
    }
}

/// Position along the slide axis (up and right are positive).
pub fn along(s: Section, p: Pt) -> f32 {
    if s.is_vertical() {
        p.y
    } else {
        p.x
    }
}

/// Distance travelled away from the edge, toward the middle.
pub fn inward(s: Section, from: Pt, to: Pt) -> f32 {
    match s {
        Section::Left => to.x - from.x,
        Section::Right => from.x - to.x,
        Section::Bottom => to.y - from.y,
        _ => from.y - to.y,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corners_win_over_edges() {
        let st = Settings::default();
        assert_eq!(hit(Pt::new(0.02, 0.98), &st), Some(Section::TopLeft));
        assert_eq!(hit(Pt::new(0.98, 0.02), &st), Some(Section::BottomRight));
        assert_eq!(hit(Pt::new(0.02, 0.5), &st), Some(Section::Left));
        assert_eq!(hit(Pt::new(0.5, 0.98), &st), Some(Section::Top));
        assert_eq!(hit(Pt::new(0.5, 0.5), &st), None);
    }

    #[test]
    fn entry_edge_needs_assigned_thin_strip() {
        let st = Settings::default();
        assert_eq!(entry_edge(Pt::new(0.01, 0.5), &st), Some(Section::Left));
        assert_eq!(entry_edge(Pt::new(0.99, 0.5), &st), Some(Section::Right));
        assert_eq!(entry_edge(Pt::new(0.5, 0.99), &st), Some(Section::Top));
        assert_eq!(entry_edge(Pt::new(0.5, 0.01), &st), Some(Section::Bottom));
        assert_eq!(entry_edge(Pt::new(0.08, 0.5), &st), None, "inside the pad, not the strip");
        assert_eq!(entry_edge(Pt::new(0.01, 0.99), &st), None, "corner square");
        let mut off = Settings::default();
        off.section_mut(Section::Left).slide = SlideAction::Off;
        assert_eq!(entry_edge(Pt::new(0.01, 0.5), &off), None);
    }

    #[test]
    fn corridor_follows_edge_depth_with_a_minimum() {
        let st = Settings::default(); // depth 0.10 -> limit 0.175
        assert!(in_corridor(Section::Left, Pt::new(0.17, 0.5), &st));
        assert!(!in_corridor(Section::Left, Pt::new(0.18, 0.5), &st));
        assert!(in_corridor(Section::Top, Pt::new(0.5, 0.83), &st));
        assert!(!in_corridor(Section::Top, Pt::new(0.5, 0.82), &st));
    }

    #[test]
    fn along_and_inward() {
        assert_eq!(along(Section::Left, Pt::new(0.1, 0.7)), 0.7);
        assert_eq!(along(Section::Top, Pt::new(0.1, 0.7)), 0.1);
        assert!((inward(Section::Left, Pt::new(0.0, 0.5), Pt::new(0.1, 0.5)) - 0.1).abs() < 1e-6);
        assert!((inward(Section::Top, Pt::new(0.5, 1.0), Pt::new(0.5, 0.9)) - 0.1).abs() < 1e-6);
    }
}

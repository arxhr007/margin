//! Turns touchpad frames and clicks into section actions. No Win32 calls in here: the engine reads
//! fingers from the tracker and *returns effects* (key presses, brightness, HUD text, ...) that the
//! app carries out after the mouse hook has returned. That keeps it unit-testable without a touchpad.
//!
//! Edges, two ways to start (user's choice):
//!   Press   - one finger rests in an assigned edge, the touchpad is physically clicked, then the finger
//!             slides while held. The click and the drag are hidden from apps.
//!   SlideIn - one finger first appears in the thin outer strip of an assigned edge (it came in from the
//!             laptop body), then moves mostly along the edge within a short window. No click needed.
//!             Fingers born anywhere else, or any multi-finger contact, are ignored until all fingers lift.
//! Corners always need a physical click; 1-3 quick clicks pick the action.
//! While a slide runs the pointer is frozen and travel along the edge is turned into steps.

use std::collections::HashSet;

use crate::geometry;
use crate::keys;
use crate::settings::{Activation, ClickAction, ClickSpec, Section, SectionSettings, Settings, SlideAction, Speed};
use crate::touchpad::tracker::{TouchPoint, TouchTracker};

const INTENT_WINDOW_MS: u64 = 600;
const INTENT_TRAVEL: f32 = 0.03;
/// How long to wait for another click of a multi-click combination.
pub const MULTI_CLICK_MS: u32 = 300;

/// What the engine needs from the outside world.
pub trait Env {
    fn now_ms(&self) -> u64;
    /// Ctrl + Shift held: fine steps.
    fn fine_held(&self) -> bool;
}

/// Something for the app to do. Effects come out in the order they must run.
#[derive(Clone, PartialEq, Debug)]
pub enum Effect {
    Tap { vk: u16, times: u32 },
    Combo { vk: u16, mods: Vec<u16>, times: u32 },
    CtrlWheel(i32),
    /// Press (`up: false`) or release a key and leave it that way.
    Hold { vk: u16, up: bool },
    BrightnessRefresh,
    /// Move brightness by this many percent, then show it in the HUD.
    BrightnessNudge(i32),
    HudText(String),
    HideHud(u32),
    RunClick(ClickSpec),
    /// (Re)start the multi-click timer; the app calls `Engine::click_timer_fired` when it expires.
    StartClickTimer,
    StopClickTimer,
}

struct Run {
    section: Section,
    action: SlideAction,
    finger_id: i32,
    origin: f32,
    from_press: bool,
    steps: i32,
}

struct Candidate {
    section: Section,
    finger_id: i32,
    born_at: u64,
}

#[derive(Default)]
pub struct Engine {
    run: Option<Run>,
    candidate: Option<Candidate>,
    ignored: HashSet<i32>,
    multi_finger_seen: bool,
    holding_click: bool,
    /// Modifiers kept down for the current slide (Alt for an Alt+Tab switcher).
    held_keys: Vec<u16>,
    pressed_corner: Option<Section>,
    click_corner: Option<Section>,
    click_count: u32,
    effects: Vec<Effect>,
}

impl Engine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Effects produced since the last call, in order.
    pub fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }

    /// While true the mouse hook drops pointer movement.
    pub fn freeze_pointer(&self) -> bool {
        self.run.is_some()
    }

    pub fn is_sliding(&self) -> bool {
        self.run.is_some()
    }

    // ---------------- touch frames ----------------

    /// Call after every complete touch frame.
    pub fn on_frame(&mut self, tracker: &TouchTracker, st: &Settings, env: &dyn Env) {
        let fingers = tracker.snapshot();
        if fingers.is_empty() {
            self.multi_finger_seen = false;
            self.ignored.clear();
            self.candidate = None;
        } else if fingers.len() > 1 {
            self.multi_finger_seen = true;
            self.candidate = None;
        }

        if self.run.is_some() {
            self.follow(fingers, st, env);
        } else if st.enabled && st.activation == Activation::SlideIn {
            self.watch(fingers, st, env);
        }
    }

    /// Slide-in mode: look for a finger that entered from outside and then moves along the edge.
    fn watch(&mut self, fingers: &[TouchPoint], st: &Settings, env: &dyn Env) {
        if fingers.len() != 1 || self.multi_finger_seen {
            return;
        }
        let f = fingers[0];
        if self.ignored.contains(&f.id) {
            return;
        }
        let now = env.now_ms();

        let same_finger = matches!(&self.candidate, Some(c) if c.finger_id == f.id);
        if !same_finger {
            // Judge each finger once, by where it first touched down.
            match geometry::entry_edge(f.start, st) {
                Some(edge) => self.candidate = Some(Candidate { section: edge, finger_id: f.id, born_at: now }),
                None => {
                    self.ignored.insert(f.id);
                }
            }
            return;
        }

        let (s, born_at) = {
            let c = self.candidate.as_ref().expect("candidate checked above");
            (c.section, c.born_at)
        };
        let along = (geometry::along(s, f.current) - geometry::along(s, f.start)).abs();
        let inward = geometry::inward(s, f.start, f.current).max(0.0);
        let too_late = now.saturating_sub(born_at) > INTENT_WINDOW_MS;
        if too_late || !geometry::in_corridor(s, f.current, st) || (along >= INTENT_TRAVEL && along < inward * 0.6) {
            // wandered inward, went diagonal, or hesitated: it's normal pointer use
            self.ignored.insert(f.id);
            self.candidate = None;
            return;
        }
        if along >= INTENT_TRAVEL {
            self.candidate = None;
            self.start(s, f, false, st);
        }
    }

    fn start(&mut self, s: Section, f: TouchPoint, from_press: bool, st: &Settings) {
        let action = st.section(s).slide;
        self.run = Some(Run {
            section: s,
            action,
            finger_id: f.id,
            origin: geometry::along(s, f.current),
            from_press,
            steps: 0,
        });
        if action == SlideAction::Brightness {
            self.effects.push(Effect::BrightnessRefresh);
        }
        crate::debug!("slide start {s:?} -> {action:?} ({})", if from_press { "press" } else { "slide-in" });
    }

    fn follow(&mut self, fingers: &[TouchPoint], st: &Settings, env: &dyn Env) {
        let Some(run) = &self.run else { return };
        if fingers.len() > 1 {
            self.stop();
            return;
        }
        let Some(finger) = fingers.iter().find(|x| x.id == run.finger_id) else {
            self.stop();
            return;
        };
        let p = finger.current;
        if !geometry::in_corridor(run.section, p, st) {
            self.ignored.insert(run.finger_id);
            self.stop();
            return;
        }

        let travel = geometry::along(run.section, p) - run.origin;
        let target = (travel / step_size(run.action, st)) as i32; // truncates toward zero, like the C# cast
        let delta = target - run.steps;
        if delta == 0 {
            return;
        }
        let (section, action) = (run.section, run.action);
        if let Some(run) = &mut self.run {
            run.steps = target;
        }
        self.perform(section, action, delta, target, st, env);
    }

    fn perform(&mut self, section: Section, action: SlideAction, delta: i32, total: i32, st: &Settings, env: &dyn Env) {
        let fine = env.fine_held();
        let n = delta.unsigned_abs();
        let forward = delta > 0;
        match action {
            SlideAction::Off => {}
            SlideAction::Volume => self.effects.push(Effect::Tap {
                vk: if forward { keys::VOLUME_UP } else { keys::VOLUME_DOWN },
                times: n * if fine { 1 } else { 2 },
            }),
            SlideAction::Brightness => {
                self.effects.push(Effect::BrightnessNudge(delta * if fine { 1 } else { 5 }));
            }
            SlideAction::Seek => {
                self.effects.push(Effect::Tap { vk: if forward { keys::RIGHT } else { keys::LEFT }, times: n });
                let label = if total >= 0 {
                    format!("\u{23E9}  +{}s", total * 5)
                } else {
                    format!("\u{23EA}  \u{2212}{}s", -total * 5)
                };
                self.effects.push(Effect::HudText(label));
            }
            SlideAction::Zoom => {
                self.effects.push(Effect::CtrlWheel(if forward { n as i32 } else { -(n as i32) }));
            }
            SlideAction::Tracks => self.effects.push(Effect::Tap {
                vk: if forward { keys::MEDIA_NEXT } else { keys::MEDIA_PREV },
                times: n,
            }),
            SlideAction::Tabs => {
                // Hold Alt for the whole slide so the Alt+Tab switcher stays open; stop() releases it.
                self.hold_keys(&[keys::ALT]);
                self.effects.push(Effect::Combo {
                    vk: keys::TAB,
                    mods: if forward { vec![] } else { vec![keys::SHIFT] },
                    times: n,
                });
            }
            SlideAction::Custom => self.send_custom(st.section(section), forward, n),
            SlideAction::Desktops => self.effects.push(Effect::Combo {
                vk: if forward { keys::RIGHT } else { keys::LEFT },
                mods: vec![keys::CTRL, keys::WIN],
                times: n,
            }),
        }
    }

    /// Presses the modifiers that stay down until the slide ends.
    fn hold_keys(&mut self, keys: &[u16]) {
        for &k in keys {
            if self.held_keys.contains(&k) {
                continue;
            }
            self.effects.push(Effect::Hold { vk: k, up: false });
            self.held_keys.push(k);
        }
    }

    fn release_held_keys(&mut self) {
        for &k in self.held_keys.iter().rev() {
            self.effects.push(Effect::Hold { vk: k, up: true });
        }
        self.held_keys.clear();
    }

    /// Custom keys: sends the user's shortcut for this direction `n` times.
    fn send_custom(&mut self, cfg: &SectionSettings, forward: bool, n: u32) {
        let (this, other) = if forward { (&cfg.forward, &cfg.back) } else { (&cfg.back, &cfg.forward) };
        let Some((key, mut mods)) = this.as_deref().and_then(keys::parse_shortcut) else {
            self.effects.push(Effect::HudText("Set a shortcut for this direction in Margin".into()));
            return;
        };
        if cfg.hold_modifiers {
            // Hold the modifiers both directions share; only the rest are pressed per step.
            let other_mods = other.as_deref().and_then(keys::parse_shortcut).map(|(_, m)| m).unwrap_or_default();
            let shared: Vec<u16> = mods.iter().copied().filter(|m| other_mods.contains(m)).collect();
            self.hold_keys(&shared);
            mods.retain(|m| !shared.contains(m));
        }
        self.effects.push(Effect::Combo { vk: key, mods, times: n });
    }

    fn stop(&mut self) {
        let Some(run) = self.run.take() else { return };
        crate::debug!("slide end {:?} after {} steps", run.section, run.steps);
        if matches!(run.action, SlideAction::Tabs | SlideAction::Custom) {
            self.release_held_keys();
        }
        if matches!(run.action, SlideAction::Brightness | SlideAction::Seek) {
            self.effects.push(Effect::HideHud(700));
        }
    }

    // ---------------- clicks (from the mouse hook) ----------------

    /// Returns true when the click belongs to Margin and must be hidden from apps.
    pub fn mouse_down(&mut self, tracker: &TouchTracker, st: &Settings) -> bool {
        if self.holding_click {
            crate::log::info("button-up was lost, clearing stale click");
            self.holding_click = false;
            self.pressed_corner = None;
        }
        if !st.enabled {
            return false;
        }

        // A click during a slide-in run is part of the gesture.
        if self.run.is_some() {
            self.holding_click = true;
            return true;
        }

        // Tap-to-click has no button bit and comes after the finger lifts, so it never qualifies.
        if tracker.device_has_button && !tracker.button_down {
            return false;
        }
        let fingers = tracker.snapshot();
        if fingers.len() != 1 {
            return false;
        }
        let f = fingers[0];
        let Some(s) = geometry::hit(f.start, st) else { return false };
        if geometry::hit(f.current, st) != Some(s) || !geometry::is_assigned(s, st) {
            return false;
        }

        if s.is_corner() {
            self.pressed_corner = Some(s);
        } else {
            if st.activation != Activation::Press {
                return false;
            }
            self.start(s, f, true, st);
        }
        self.holding_click = true;
        true
    }

    pub fn mouse_up(&mut self, st: &Settings) {
        if !self.holding_click {
            return;
        }
        self.holding_click = false;
        if matches!(&self.run, Some(r) if r.from_press) {
            self.stop();
        }
        if let Some(corner) = self.pressed_corner.take() {
            self.count_click(corner, st);
        }
    }

    pub fn reset(&mut self) {
        self.stop();
        self.candidate = None;
        self.holding_click = false;
        self.pressed_corner = None;
    }

    fn count_click(&mut self, corner: Section, st: &Settings) {
        if self.click_corner.is_some_and(|c| c != corner) {
            // Another corner is still waiting for a possible next click: its clicks are final now.
            self.finish_clicks(st);
        }
        if self.click_corner != Some(corner) {
            self.click_count = 0;
        }
        self.click_corner = Some(corner);
        self.click_count += 1;
        self.effects.push(Effect::StopClickTimer);

        let clicks = &st.section(corner).clicks;
        let last = clicks.iter().rposition(|c| c.action != ClickAction::None).map_or(0, |i| i + 1);
        if self.click_count as usize >= last {
            self.finish_clicks(st); // no longer combination to wait for
        } else {
            self.effects.push(Effect::StartClickTimer);
        }
    }

    /// The multi-click timer expired: run whatever the clicks so far select.
    pub fn click_timer_fired(&mut self, st: &Settings) {
        self.finish_clicks(st);
    }

    fn finish_clicks(&mut self, st: &Settings) {
        self.effects.push(Effect::StopClickTimer);
        let Some(corner) = self.click_corner.take() else { return };
        let count = std::mem::take(&mut self.click_count) as usize;
        let clicks = &st.section(corner).clicks;
        if count < 1 || count > clicks.len() || clicks[count - 1].action == ClickAction::None {
            return;
        }
        crate::debug!("{corner:?} x{count} -> {:?}", clicks[count - 1].action);
        self.effects.push(Effect::RunClick(clicks[count - 1].clone()));
    }
}

fn step_size(a: SlideAction, st: &Settings) -> f32 {
    let base = match st.speed {
        Speed::Slow => 0.06,
        Speed::Fast => 0.03,
        Speed::Medium => 0.042,
    };
    // Discrete actions (tabs, tracks, desktops, custom keys) get bigger steps so one swipe doesn't overshoot.
    if matches!(a, SlideAction::Tabs | SlideAction::Tracks | SlideAction::Desktops | SlideAction::Custom) {
        base * 2.5
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct FakeEnv {
        now: Cell<u64>,
        fine: Cell<bool>,
    }

    impl FakeEnv {
        fn new() -> Self {
            FakeEnv { now: Cell::new(1_000), fine: Cell::new(false) }
        }
        fn advance(&self, ms: u64) {
            self.now.set(self.now.get() + ms);
        }
    }

    impl Env for FakeEnv {
        fn now_ms(&self) -> u64 {
            self.now.get()
        }
        fn fine_held(&self) -> bool {
            self.fine.get()
        }
    }

    struct Rig {
        engine: Engine,
        tracker: TouchTracker,
        st: Settings,
        env: FakeEnv,
    }

    impl Rig {
        fn new() -> Self {
            Rig { engine: Engine::new(), tracker: TouchTracker::new(), st: Settings::default(), env: FakeEnv::new() }
        }

        fn frame(&mut self, fingers: &[(i32, f32, f32)]) {
            self.tracker.commit(fingers, false);
            self.engine.on_frame(&self.tracker, &self.st, &self.env);
        }

        fn lift(&mut self) {
            self.frame(&[]);
        }

        fn fx(&mut self) -> Vec<Effect> {
            self.engine.take_effects()
        }
    }

    // --- slide-in ------------------------------------------------------------------------

    #[test]
    fn slide_in_on_left_edge_turns_travel_into_volume_steps() {
        let mut r = Rig::new(); // left = volume, medium speed -> 0.042 per step
        r.frame(&[(1, 0.01, 0.50)]); // born in the entry strip
        r.frame(&[(1, 0.01, 0.52)]); // 0.02 along: not yet
        assert!(!r.engine.freeze_pointer());
        r.frame(&[(1, 0.01, 0.54)]); // 0.04 along: slide starts, origin 0.54
        assert!(r.engine.freeze_pointer());
        assert!(r.fx().is_empty());
        r.frame(&[(1, 0.01, 0.63)]); // 0.09 travel = 2 steps
        assert_eq!(r.fx(), vec![Effect::Tap { vk: keys::VOLUME_UP, times: 4 }]); // 2 steps x 2 key presses
        r.frame(&[(1, 0.01, 0.55)]); // back down to about 0 steps
        assert_eq!(r.fx(), vec![Effect::Tap { vk: keys::VOLUME_DOWN, times: 4 }]);
    }

    #[test]
    fn fine_modifiers_halve_volume_presses() {
        let mut r = Rig::new();
        r.env.fine.set(true);
        r.frame(&[(1, 0.01, 0.50)]);
        r.frame(&[(1, 0.01, 0.54)]);
        r.frame(&[(1, 0.01, 0.60)]); // 0.06 travel = 1 step
        assert_eq!(r.fx(), vec![Effect::Tap { vk: keys::VOLUME_UP, times: 1 }]);
    }

    #[test]
    fn speed_changes_step_size() {
        let mut r = Rig::new();
        r.st.speed = Speed::Slow; // 0.06 per step
        r.frame(&[(1, 0.01, 0.50)]);
        r.frame(&[(1, 0.01, 0.54)]);
        r.frame(&[(1, 0.01, 0.59)]); // 0.05: under one slow step
        assert!(r.fx().is_empty());
        r.frame(&[(1, 0.01, 0.61)]); // 0.07: one step
        assert_eq!(r.fx().len(), 1);
    }

    #[test]
    fn finger_that_lands_inside_the_pad_never_triggers() {
        let mut r = Rig::new();
        r.frame(&[(1, 0.20, 0.50)]);
        r.frame(&[(1, 0.05, 0.50)]); // drifts onto the edge
        r.frame(&[(1, 0.05, 0.70)]);
        r.frame(&[(1, 0.05, 0.90)]);
        assert!(!r.engine.freeze_pointer());
        assert!(r.fx().is_empty());
    }

    #[test]
    fn two_finger_contact_is_ignored_until_everything_lifts() {
        let mut r = Rig::new();
        r.frame(&[(1, 0.01, 0.50), (2, 0.60, 0.50)]);
        r.frame(&[(1, 0.01, 0.56)]);
        r.frame(&[(1, 0.01, 0.62)]);
        assert!(!r.engine.freeze_pointer());
        r.lift();
        r.frame(&[(3, 0.01, 0.50)]);
        r.frame(&[(3, 0.01, 0.54)]);
        assert!(r.engine.freeze_pointer(), "a fresh single finger works again");
    }

    #[test]
    fn hesitating_longer_than_the_intent_window_is_normal_pointer_use() {
        let mut r = Rig::new();
        r.frame(&[(1, 0.01, 0.50)]);
        r.env.advance(700);
        r.frame(&[(1, 0.01, 0.60)]);
        assert!(!r.engine.freeze_pointer());
    }

    #[test]
    fn diagonal_movement_is_not_a_slide() {
        let mut r = Rig::new();
        r.frame(&[(1, 0.01, 0.50)]);
        r.frame(&[(1, 0.12, 0.54)]); // mostly inward
        assert!(!r.engine.freeze_pointer());
    }

    #[test]
    fn leaving_the_corridor_ends_the_slide_and_the_finger_is_ignored() {
        let mut r = Rig::new();
        r.frame(&[(1, 0.01, 0.50)]);
        r.frame(&[(1, 0.01, 0.54)]);
        assert!(r.engine.freeze_pointer());
        r.frame(&[(1, 0.40, 0.60)]);
        assert!(!r.engine.freeze_pointer());
        r.frame(&[(1, 0.01, 0.70)]); // same finger returning does not restart
        assert!(!r.engine.freeze_pointer());
    }

    #[test]
    fn lifting_the_finger_ends_the_slide() {
        let mut r = Rig::new();
        r.frame(&[(1, 0.01, 0.50)]);
        r.frame(&[(1, 0.01, 0.54)]);
        r.lift();
        assert!(!r.engine.freeze_pointer());
    }

    #[test]
    fn unassigned_edge_is_a_normal_touchpad() {
        let mut r = Rig::new();
        r.st.section_mut(Section::Left).slide = SlideAction::Off;
        r.frame(&[(1, 0.01, 0.50)]);
        r.frame(&[(1, 0.01, 0.60)]);
        assert!(!r.engine.freeze_pointer());
    }

    #[test]
    fn disabled_margin_does_nothing() {
        let mut r = Rig::new();
        r.st.enabled = false;
        r.frame(&[(1, 0.01, 0.50)]);
        r.frame(&[(1, 0.01, 0.60)]);
        assert!(!r.engine.freeze_pointer());
        assert!(!r.engine.mouse_down(&r.tracker, &r.st));
    }

    // --- actions -------------------------------------------------------------------------

    fn slide_right_edge(r: &mut Rig, to: f32) {
        r.frame(&[(1, 0.99, 0.50)]);
        r.frame(&[(1, 0.99, 0.54)]);
        r.frame(&[(1, 0.99, to)]);
    }

    #[test]
    fn brightness_refreshes_then_nudges_five_percent_per_step() {
        let mut r = Rig::new(); // right = brightness
        r.frame(&[(1, 0.99, 0.50)]);
        r.frame(&[(1, 0.99, 0.54)]);
        assert_eq!(r.fx(), vec![Effect::BrightnessRefresh]);
        r.frame(&[(1, 0.99, 0.63)]);
        assert_eq!(r.fx(), vec![Effect::BrightnessNudge(10)]);
        r.lift();
        assert_eq!(r.fx(), vec![Effect::HideHud(700)]);
    }

    #[test]
    fn fine_brightness_is_one_percent_per_step() {
        let mut r = Rig::new();
        r.env.fine.set(true);
        slide_right_edge(&mut r, 0.63);
        assert_eq!(r.fx(), vec![Effect::BrightnessRefresh, Effect::BrightnessNudge(2)]);
    }

    #[test]
    fn seek_sends_arrows_and_shows_seconds() {
        let mut r = Rig::new();
        r.st.section_mut(Section::Right).slide = SlideAction::Seek;
        slide_right_edge(&mut r, 0.63);
        assert_eq!(
            r.fx(),
            vec![
                Effect::Tap { vk: keys::RIGHT, times: 2 },
                Effect::HudText("\u{23E9}  +10s".into()),
            ]
        );
        r.frame(&[(1, 0.99, 0.40)]); // far back: negative total
        let fx = r.fx();
        assert!(matches!(fx[0], Effect::Tap { vk: keys::LEFT, .. }));
        assert!(matches!(&fx[1], Effect::HudText(t) if t.contains('\u{2212}')));
    }

    #[test]
    fn zoom_tracks_and_desktops() {
        let mut r = Rig::new();
        r.st.section_mut(Section::Right).slide = SlideAction::Zoom;
        slide_right_edge(&mut r, 0.63);
        assert_eq!(r.fx(), vec![Effect::CtrlWheel(2)]);

        let mut r = Rig::new();
        r.st.section_mut(Section::Right).slide = SlideAction::Tracks;
        slide_right_edge(&mut r, 0.80); // discrete: 0.105 per step, travel 0.26 = 2 steps
        assert_eq!(r.fx(), vec![Effect::Tap { vk: keys::MEDIA_NEXT, times: 2 }]);

        let mut r = Rig::new();
        r.st.section_mut(Section::Right).slide = SlideAction::Desktops;
        slide_right_edge(&mut r, 0.80);
        assert_eq!(
            r.fx(),
            vec![Effect::Combo { vk: keys::RIGHT, mods: vec![keys::CTRL, keys::WIN], times: 2 }]
        );
    }

    #[test]
    fn tabs_hold_alt_for_the_whole_slide_then_release() {
        let mut r = Rig::new();
        r.st.section_mut(Section::Right).slide = SlideAction::Tabs;
        slide_right_edge(&mut r, 0.70); // 0.16 travel / 0.105 = 1 step
        assert_eq!(
            r.fx(),
            vec![
                Effect::Hold { vk: keys::ALT, up: false },
                Effect::Combo { vk: keys::TAB, mods: vec![], times: 1 },
            ]
        );
        r.frame(&[(1, 0.99, 0.80)]); // another step: Alt is already down
        assert_eq!(r.fx(), vec![Effect::Combo { vk: keys::TAB, mods: vec![], times: 1 }]);
        r.frame(&[(1, 0.99, 0.50)]); // backwards uses Shift+Tab
        let fx = r.fx();
        assert!(matches!(&fx[0], Effect::Combo { vk: keys::TAB, mods, .. } if mods == &vec![keys::SHIFT]));
        r.lift();
        assert_eq!(r.fx(), vec![Effect::Hold { vk: keys::ALT, up: true }]);
    }

    #[test]
    fn custom_keys_hold_shared_modifiers() {
        let mut r = Rig::new();
        {
            let s = r.st.section_mut(Section::Right);
            s.slide = SlideAction::Custom;
            s.forward = Some("Alt+Tab".into());
            s.back = Some("Alt+Shift+Tab".into());
            s.hold_modifiers = true;
        }
        slide_right_edge(&mut r, 0.70);
        assert_eq!(
            r.fx(),
            vec![
                Effect::Hold { vk: keys::ALT, up: false },
                Effect::Combo { vk: keys::TAB, mods: vec![], times: 1 },
            ]
        );
        r.frame(&[(1, 0.99, 0.50)]);
        let fx = r.fx();
        assert_eq!(fx, vec![Effect::Combo { vk: keys::TAB, mods: vec![keys::SHIFT], times: fx_times(&fx) }]);
        r.lift();
        assert_eq!(r.fx(), vec![Effect::Hold { vk: keys::ALT, up: true }]);
    }

    fn fx_times(fx: &[Effect]) -> u32 {
        match &fx[0] {
            Effect::Combo { times, .. } => *times,
            _ => 0,
        }
    }

    #[test]
    fn custom_keys_without_hold_press_the_full_combo() {
        let mut r = Rig::new();
        {
            let s = r.st.section_mut(Section::Right);
            s.slide = SlideAction::Custom;
            s.forward = Some("Ctrl+PageDown".into());
            s.back = Some("Ctrl+PageUp".into());
        }
        slide_right_edge(&mut r, 0.70);
        assert_eq!(r.fx(), vec![Effect::Combo { vk: 0x22, mods: vec![keys::CTRL], times: 1 }]);
        r.lift();
        assert!(r.fx().iter().all(|e| !matches!(e, Effect::Hold { .. })), "nothing was held");
    }

    #[test]
    fn custom_keys_with_a_missing_shortcut_tell_the_user() {
        let mut r = Rig::new();
        {
            let s = r.st.section_mut(Section::Right);
            s.slide = SlideAction::Custom;
            s.forward = Some("Hyper+Q".into());
        }
        slide_right_edge(&mut r, 0.70);
        assert_eq!(r.fx(), vec![Effect::HudText("Set a shortcut for this direction in Margin".into())]);
    }

    // --- press mode and corners ------------------------------------------------------------

    fn press(r: &mut Rig, at: (f32, f32)) -> bool {
        r.tracker.device_has_button = true;
        r.tracker.commit(&[(1, at.0, at.1)], true);
        r.engine.on_frame(&r.tracker, &r.st, &r.env);
        r.engine.mouse_down(&r.tracker, &r.st)
    }

    fn release(r: &mut Rig) {
        r.tracker.commit(&[], false);
        r.engine.mouse_up(&r.st);
        r.engine.on_frame(&r.tracker, &r.st, &r.env);
    }

    #[test]
    fn press_mode_needs_a_real_click_on_an_assigned_edge() {
        let mut r = Rig::new();
        r.st.activation = Activation::Press;
        // no click: nothing starts even when sliding in from the edge
        r.frame(&[(1, 0.01, 0.50)]);
        r.frame(&[(1, 0.01, 0.60)]);
        assert!(!r.engine.freeze_pointer());
        r.lift();

        assert!(press(&mut r, (0.05, 0.50)), "click on the left edge is swallowed");
        assert!(r.engine.freeze_pointer());
        r.tracker.commit(&[(1, 0.05, 0.64)], true);
        r.engine.on_frame(&r.tracker, &r.st, &r.env);
        assert_eq!(r.fx(), vec![Effect::Tap { vk: keys::VOLUME_UP, times: 6 }]); // 0.14 = 3 steps
        release(&mut r);
        assert!(!r.engine.freeze_pointer());
    }

    #[test]
    fn click_in_the_middle_or_without_button_bit_is_not_ours() {
        let mut r = Rig::new();
        r.st.activation = Activation::Press;
        assert!(!press(&mut r, (0.5, 0.5)));
        release(&mut r);
        // tap-to-click: the pad has a button but it is not down
        r.tracker.device_has_button = true;
        r.tracker.commit(&[(1, 0.05, 0.5)], false);
        assert!(!r.engine.mouse_down(&r.tracker, &r.st));
    }

    #[test]
    fn slide_in_mode_leaves_edge_clicks_to_apps() {
        let mut r = Rig::new(); // slide-in
        assert!(!press(&mut r, (0.05, 0.50)));
    }

    #[test]
    fn single_click_corner_runs_immediately() {
        let mut r = Rig::new(); // top-left: show desktop
        assert!(press(&mut r, (0.05, 0.95)));
        release(&mut r);
        let fx = r.fx();
        assert!(fx.contains(&Effect::RunClick(ClickSpec { action: ClickAction::ShowDesktop, value: None })));
    }

    #[test]
    fn corner_waits_for_more_clicks_when_a_longer_combination_exists() {
        let mut r = Rig::new(); // top-right: 1 = task view, 2 = notifications
        assert!(press(&mut r, (0.95, 0.95)));
        release(&mut r);
        let fx = r.fx();
        assert!(fx.contains(&Effect::StartClickTimer));
        assert!(!fx.iter().any(|e| matches!(e, Effect::RunClick(_))), "must wait for a possible second click");

        // timer expires with one click: task view
        r.engine.click_timer_fired(&r.st);
        let fx = r.fx();
        assert!(fx.contains(&Effect::RunClick(ClickSpec { action: ClickAction::TaskView, value: None })));

        // two quick clicks: notifications, no timer needed once the last combination is reached
        assert!(press(&mut r, (0.95, 0.95)));
        release(&mut r);
        r.fx();
        assert!(press(&mut r, (0.95, 0.95)));
        release(&mut r);
        let fx = r.fx();
        assert!(fx.contains(&Effect::RunClick(ClickSpec { action: ClickAction::Notifications, value: None })));
        assert!(!fx.contains(&Effect::StartClickTimer));
    }

    #[test]
    fn triple_click_corner() {
        let mut r = Rig::new(); // bottom-right: play, next, prev
        for _ in 0..3 {
            assert!(press(&mut r, (0.95, 0.05)));
            release(&mut r);
        }
        let runs: Vec<_> = r.fx().into_iter().filter_map(|e| if let Effect::RunClick(c) = e { Some(c.action) } else { None }).collect();
        assert_eq!(runs, vec![ClickAction::PrevTrack]);
    }

    #[test]
    fn clicks_on_a_different_corner_restart_the_count() {
        let mut r = Rig::new();
        assert!(press(&mut r, (0.95, 0.05))); // bottom-right once: waits for a second or third click
        release(&mut r);
        r.fx();
        assert!(press(&mut r, (0.05, 0.05))); // bottom-left (lock): count restarts at 1
        release(&mut r);
        let runs: Vec<_> = r.fx().into_iter().filter_map(|e| if let Effect::RunClick(c) = e { Some(c.action) } else { None }).collect();
        // the bottom-right single click is not lost: it runs first, then the lock
        assert_eq!(runs, vec![ClickAction::PlayPause, ClickAction::Lock]);
    }

    #[test]
    fn switching_corners_twice_runs_each_pending_click_once() {
        let mut r = Rig::new();
        assert!(press(&mut r, (0.95, 0.95))); // top-right once (task view, waits for notifications)
        release(&mut r);
        assert!(press(&mut r, (0.95, 0.05))); // bottom-right once (play, waits)
        release(&mut r);
        r.engine.click_timer_fired(&r.st);
        r.engine.click_timer_fired(&r.st); // a stale second timer must not run anything again
        let runs: Vec<_> = r.fx().into_iter().filter_map(|e| if let Effect::RunClick(c) = e { Some(c.action) } else { None }).collect();
        assert_eq!(runs, vec![ClickAction::TaskView, ClickAction::PlayPause]);
    }

    #[test]
    fn unassigned_corner_and_click_slots_do_nothing() {
        let mut r = Rig::new();
        r.st.section_mut(Section::TopLeft).clicks.clear();
        assert!(!press(&mut r, (0.05, 0.95)));

        let mut r = Rig::new();
        r.st.section_mut(Section::TopLeft).clicks = vec![
            ClickSpec { action: ClickAction::None, value: None },
            ClickSpec { action: ClickAction::Mute, value: None },
        ];
        assert!(press(&mut r, (0.05, 0.95)));
        release(&mut r);
        let fx = r.fx();
        assert!(!fx.iter().any(|e| matches!(e, Effect::RunClick(_))), "single click slot is empty");
    }

    #[test]
    fn a_lost_button_up_is_recovered_on_the_next_click() {
        let mut r = Rig::new();
        assert!(press(&mut r, (0.05, 0.95))); // corner down, never released
        assert!(press(&mut r, (0.05, 0.95)), "next click still works");
    }

    #[test]
    fn reset_stops_a_running_slide_and_releases_held_keys() {
        let mut r = Rig::new();
        r.st.section_mut(Section::Right).slide = SlideAction::Tabs;
        slide_right_edge(&mut r, 0.70);
        r.fx();
        r.engine.reset();
        assert_eq!(r.fx(), vec![Effect::Hold { vk: keys::ALT, up: true }]);
        assert!(!r.engine.freeze_pointer());
    }
}

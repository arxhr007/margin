//! The Linux daemon: reads touchpads, runs the gesture engine, and sends the resulting key presses.
//! Runs as a systemd user service, or in a terminal with `margin run` / `margin --diag`.

use std::os::raw::c_int;
use std::sync::atomic::{AtomicBool, Ordering};

use super::actions;
use super::devices::{self, Probe, Touchpad};
use super::output::Output;
use super::sys::Fd;
use crate::engine::{Effect, Engine, Env, HOLD_MS, MOMENTUM_TICK_MS, MULTI_CLICK_MS};
use crate::settings::{self, Activation, Section, Settings};
use crate::touchpad::tracker::TouchTracker;

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_: c_int) {
    STOP.store(true, Ordering::SeqCst);
}

struct LinuxEnv;

impl Env for LinuxEnv {
    fn now_ms(&self) -> u64 {
        crate::now_ms()
    }
    /// The Ctrl+Shift "fine steps" modifier would need a keyboard reader; not supported on Linux yet.
    fn fine_held(&self) -> bool {
        false
    }
}

/// Edge slides on Linux always start by sliding in from the edge. Press-and-slide needs the click to be
/// hidden from the desktop, which a passive reader can't do. Corners keep their own choice.
fn prepared(mut s: Settings) -> Settings {
    s.activation = Activation::SlideIn;
    for edge in [Section::Left, Section::Right, Section::Top, Section::Bottom] {
        s.section_mut(edge).needs_click = None;
    }
    s
}

/// Only one Margin per user: a lock file that disappears with the process.
fn lock_instance() -> Option<Fd> {
    let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    // SAFETY: getuid has no preconditions.
    let uid = unsafe { libc::getuid() };
    let path = format!("{dir}/margin-{uid}.lock");
    let c = std::ffi::CString::new(path).ok()?;
    // SAFETY: valid C string; flags and mode are plain integers.
    let fd = unsafe { libc::open(c.as_ptr(), libc::O_CREAT | libc::O_RDWR | libc::O_CLOEXEC, 0o600) };
    if fd < 0 {
        return None;
    }
    let fd = Fd(fd);
    // SAFETY: `fd` is a valid descriptor.
    if unsafe { libc::flock(fd.0, libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return None;
    }
    Some(fd)
}

struct Daemon {
    settings: Settings,
    tracker: TouchTracker,
    engine: Engine,
    out: Output,
    pads: Vec<Touchpad>,
    next_id: u32,
    /// The pad whose fingers are being tracked (two pads: the one touched first wins until all fingers lift).
    active: Option<u32>,
    prev_button: bool,
    click_deadline: Option<u64>,
    hold_deadline: Option<u64>,
    momentum_deadline: Option<u64>,
    denied_logged: bool,
}

impl Daemon {
    fn scan(&mut self) {
        let known: Vec<String> = self.pads.iter().map(|p| p.path.clone()).collect();
        for path in devices::event_paths() {
            if known.contains(&path) {
                continue;
            }
            self.next_id += 1;
            match devices::probe(self.next_id, &path) {
                Probe::Touchpad(pad) => {
                    crate::log::info(format!(
                        "touchpad: {} ({}), button={}",
                        pad.name, pad.path, pad.has_button
                    ));
                    self.pads.push(pad);
                }
                Probe::Denied(e) => {
                    if !self.denied_logged {
                        self.denied_logged = true;
                        crate::log::error(format!(
                            "cannot read {path} ({e}). Your user needs to be in the \"input\" group: \
                             run `sudo usermod -aG input $USER`, then log out and back in"
                        ));
                    }
                }
                Probe::NotATouchpad => {}
            }
        }
    }

    fn execute(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            crate::debug!("effect {effect:?}");
            match effect {
                Effect::Tap { vk, times } => self.out.tap(vk, times),
                Effect::Combo { vk, mods, times } => self.out.combo(vk, &mods, times),
                Effect::CtrlWheel(n) => self.out.ctrl_wheel(n),
                Effect::Wheel { units, horizontal } => self.out.wheel(units, horizontal),
                Effect::Hold { vk, up } => self.out.hold(vk, up),
                Effect::BrightnessRefresh | Effect::HideHud(_) => {}
                Effect::BrightnessNudge(delta) => self.out.brightness(delta),
                Effect::HudText(text) => crate::log::info(text),
                Effect::RunClick(spec) => actions::run(&spec, &self.out),
                Effect::StartClickTimer => self.click_deadline = Some(crate::now_ms() + MULTI_CLICK_MS as u64),
                Effect::StopClickTimer => self.click_deadline = None,
                Effect::StartHoldTimer => self.hold_deadline = Some(crate::now_ms() + HOLD_MS as u64),
                Effect::StopHoldTimer => self.hold_deadline = None,
                Effect::StartMomentumTimer => self.momentum_deadline = Some(crate::now_ms() + MOMENTUM_TICK_MS as u64),
                Effect::StopMomentumTimer => self.momentum_deadline = None,
            }
        }
    }

    fn after_engine(&mut self) {
        let fx = self.engine.take_effects();
        if !fx.is_empty() {
            self.execute(fx);
        }
    }

    fn on_frame(&mut self, pad: u32, button_capable: bool, frame: crate::touchpad::evdev::Frame) {
        // With two touchpads, the one touched first is in charge until all its fingers lift.
        if self.active.is_some_and(|a| a != pad) && self.tracker.count() > 0 {
            return;
        }
        self.active = if frame.fingers.is_empty() { None } else { Some(pad) };
        self.tracker.device_has_button = button_capable;
        self.tracker.commit(&frame.fingers, frame.button);
        log_frame(&self.tracker);
        self.engine.on_frame(&self.tracker, &self.settings, &LinuxEnv);

        if frame.button != self.prev_button {
            self.prev_button = frame.button;
            if frame.button {
                // The desktop also sees this click: a passive reader can't hide it.
                self.engine.mouse_down(&self.tracker, &self.settings, &LinuxEnv);
            } else {
                self.engine.mouse_up(&self.settings);
            }
        }
        self.after_engine();
    }

    fn reload_settings(&mut self) {
        match settings::read() {
            Ok(Some(new)) => self.settings = prepared(new),
            Ok(None) => return, // deleted: keep going with what we have
            Err(e) => {
                crate::log::error(format!("{e}; keeping the current settings"));
                return;
            }
        }
        self.engine.reset(); // also lets go of any key held for a slide
        self.after_engine();
        crate::log::info("settings reloaded");
    }
}

/// Logs the finger positions in `--diag` mode, at most about 15 times a second.
fn log_frame(tracker: &TouchTracker) {
    if !crate::log::verbose() {
        return;
    }
    thread_local! { static LAST: std::cell::Cell<u64> = const { std::cell::Cell::new(0) }; }
    let now = crate::now_ms();
    if LAST.with(|l| now.saturating_sub(l.get())) < 60 && !tracker.button_down {
        return;
    }
    LAST.with(|l| l.set(now));
    let parts: Vec<String> = tracker
        .snapshot()
        .iter()
        .map(|t| format!("#{} ({:.2},{:.2}) from ({:.2},{:.2})", t.id, t.current.x, t.current.y, t.start.x, t.start.y))
        .collect();
    crate::log::debug(format!("frame btn={} n={} {}", tracker.button_down as i32, tracker.count(), parts.join(" ")));
}

pub fn run(diag: bool) {
    crate::log::set_console(true); // under systemd this goes to the journal
    if diag {
        crate::log::set_verbose(true);
        crate::log::info("diagnostic mode: every touch frame and gesture is printed here and in the log");
    }
    let Some(_lock) = lock_instance() else {
        eprintln!("margin is already running for this user (see `systemctl --user status margin`).");
        std::process::exit(1);
    };

    if !settings::exists() {
        settings::save(&Settings::default());
    }
    let mut seen = settings::modified_ms();

    // SAFETY: the handler only stores to an atomic.
    unsafe {
        libc::signal(libc::SIGINT, on_signal as libc::sighandler_t);
        libc::signal(libc::SIGTERM, on_signal as libc::sighandler_t);
    }

    crate::log::info(format!("Margin {} started", env!("CARGO_PKG_VERSION")));
    let out = Output::new();
    let mut d = Daemon {
        settings: prepared(settings::load()),
        tracker: TouchTracker::new(),
        engine: Engine::new(),
        out,
        pads: Vec::new(),
        next_id: 0,
        active: None,
        prev_button: false,
        click_deadline: None,
        hold_deadline: None,
        momentum_deadline: None,
        denied_logged: false,
    };

    let (mut last_scan, mut last_cfg) = (0u64, 0u64);
    let mut warned_no_pad = false;
    while !STOP.load(Ordering::SeqCst) {
        let now = crate::now_ms();
        if last_scan == 0 || now.saturating_sub(last_scan) >= 3000 {
            last_scan = now.max(1);
            d.scan();
            if d.pads.is_empty() && !warned_no_pad {
                warned_no_pad = true;
                crate::log::info("no precision touchpad found yet; still looking (is your user in the \"input\" group?)");
            }
        }
        if now.saturating_sub(last_cfg) >= 1000 {
            last_cfg = now;
            let m = settings::modified_ms();
            if m != seen {
                seen = m;
                d.reload_settings();
            }
        }

        // wait for touchpad events, but wake up for the click timers and the periodic checks
        let mut timeout = 1000i64;
        for deadline in [d.click_deadline, d.hold_deadline, d.momentum_deadline].into_iter().flatten() {
            timeout = timeout.min(deadline as i64 - now as i64);
        }
        let mut fds: Vec<libc::pollfd> = d.pads.iter().map(|p| libc::pollfd { fd: p.fd.0, events: libc::POLLIN, revents: 0 }).collect();
        // SAFETY: `fds` is a valid array of `fds.len()` pollfd structs.
        let ready = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, timeout.clamp(0, 1000) as c_int) };

        if ready > 0 {
            let mut gone: Vec<u32> = Vec::new();
            for i in 0..fds.len() {
                if fds[i].revents == 0 {
                    continue;
                }
                let mut frames = Vec::new();
                let pad = &mut d.pads[i];
                let result = pad.pump(|f| frames.push(f));
                let (id, capable) = (pad.id, pad.has_button);
                for f in frames {
                    d.on_frame(id, capable, f);
                }
                if result.is_err() || fds[i].revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                    gone.push(id);
                }
            }
            for id in gone {
                if let Some(pos) = d.pads.iter().position(|p| p.id == id) {
                    crate::log::info(format!("touchpad removed: {}", d.pads[pos].name));
                    d.pads.remove(pos);
                }
                if d.active == Some(id) {
                    d.active = None;
                    d.tracker.clear();
                }
                d.engine.reset();
                d.after_engine();
            }
        }

        if d.click_deadline.is_some_and(|dl| crate::now_ms() >= dl) {
            d.click_deadline = None;
            d.engine.click_timer_fired(&d.settings);
            d.after_engine();
        }
        if d.hold_deadline.is_some_and(|dl| crate::now_ms() >= dl) {
            d.hold_deadline = None;
            d.engine.hold_timer_fired(&d.settings);
            d.after_engine();
        }
        if d.momentum_deadline.is_some_and(|dl| crate::now_ms() >= dl) {
            d.momentum_deadline = Some(crate::now_ms() + MOMENTUM_TICK_MS as u64); // StopMomentumTimer clears it
            d.engine.momentum_tick(&LinuxEnv);
            d.after_engine();
        }
    }

    // let go of any key still held for a slide before leaving
    d.engine.reset();
    d.after_engine();
    crate::log::info("Margin quit");
}

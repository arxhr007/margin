//! Owns everything: touchpad input, mouse hook, engine, tray icon and the settings window.
//!
//! All of it lives on the UI thread. Raw input (`WM_INPUT`), the low-level mouse hook, the tray and the
//! settings window share one message loop, so state sits in a thread-local `RefCell` and every access goes
//! through [`with`], which gives up (instead of panicking) if a re-entrant message finds it already borrowed.

use std::cell::{Cell, RefCell};

use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::RemoteDesktop::{WTSRegisterSessionNotification, WTSUnRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION};
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::engine::{Effect, Engine, Env, HOLD_MS, MOMENTUM_TICK_MS, MULTI_CLICK_MS};
use crate::settings::{self, Settings};
use crate::touchpad::raw::RawTouchpad;
use crate::touchpad::tracker::TouchTracker;
use crate::{actions, brightness, hook, hud, keys, tray, ui};

pub const WM_APP_FLUSH: u32 = WM_APP + 1;
pub const WM_APP_TRAY: u32 = WM_APP + 2;
pub const WM_APP_SHOW_SETTINGS: u32 = WM_APP + 3;

const TIMER_CLICK: usize = 1;
const TIMER_SAVE: usize = 2;
const TIMER_WATCH: usize = 3;
const TIMER_HOLD: usize = 4;
const TIMER_MOMENTUM: usize = 5;
const SAVE_DELAY_MS: u32 = 400;
/// How often settings.json is checked for changes made outside Margin (by hand, or a synced copy).
const WATCH_MS: u32 = 1000;

pub const CMD_SETTINGS: usize = 1;
pub const CMD_TOGGLE: usize = 2;
pub const CMD_LOG: usize = 3;
pub const CMD_QUIT: usize = 4;

const WM_WTSSESSION_CHANGE: u32 = 0x02B1;
const WTS_CONSOLE_CONNECT: usize = 1;
const WTS_SESSION_UNLOCK: usize = 8;
const PBT_APMRESUMESUSPEND: usize = 0x0007;
const PBT_APMRESUMEAUTOMATIC: usize = 0x0012;
const WM_INPUT_DEVICE_CHANGE: u32 = 0x00FE;

struct RealEnv;

impl Env for RealEnv {
    fn now_ms(&self) -> u64 {
        crate::now_ms()
    }
    fn fine_held(&self) -> bool {
        keys::fine_modifiers_held()
    }
}

pub struct State {
    /// The settings as saved: for all apps, plus the per-app profiles. The settings window edits these.
    pub settings: Settings,
    /// What the engine uses: `settings` with the profile of the app in front applied.
    active: Settings,
    /// `active` needs rebuilding, as soon as no gesture is in progress.
    active_stale: bool,
    /// The front-app change `active` was built for (see FRONT_GEN).
    front_seen: u64,
    pub tracker: TouchTracker,
    pub engine: Engine,
    touchpad: RawTouchpad,
    pending: Vec<Effect>,
    flush_posted: bool,
    /// Bumped on every touch frame so the settings window knows when to repaint its finger dots.
    pub frames: u64,
}

impl State {
    fn new(settings: Settings) -> Self {
        State {
            active: settings.for_app(FRONT.with(|f| f.borrow().clone()).as_deref()),
            settings,
            active_stale: false,
            front_seen: FRONT_GEN.with(|g| g.get()),
            tracker: TouchTracker::new(),
            engine: Engine::new(),
            touchpad: RawTouchpad::new(),
            pending: Vec::new(),
            flush_posted: false,
            frames: 0,
        }
    }

    /// Collects what the engine produced and schedules it to run once the current callback has returned.
    fn engine_done(&mut self) {
        FREEZE.with(|f| f.set(self.engine.freeze_pointer()));
        let fx = self.engine.take_effects();
        if fx.is_empty() {
            return;
        }
        self.pending.extend(fx);
        if !self.flush_posted {
            self.flush_posted = true;
            let hwnd = main_hwnd();
            if unsafe { PostMessageW(hwnd, WM_APP_FLUSH, WPARAM(0), LPARAM(0)) }.is_err() {
                self.flush_posted = false;
            }
        }
    }

    fn process_raw(&mut self, lparam: LPARAM) {
        self.refresh_active();
        let State { touchpad, tracker, engine, active, frames, .. } = self;
        touchpad.process(lparam, tracker, |t| {
            engine.on_frame(t, active, &RealEnv);
            *frames += 1;
        });
        self.engine_done();
        self.refresh_active(); // a gesture may have just ended
    }

    /// The settings or the app in front changed: rebuild `active` once no gesture is in progress.
    /// A slide keeps the profile it started with, even when it switches apps (Alt+Tab).
    fn refresh_active(&mut self) {
        let front_gen = FRONT_GEN.with(|g| g.get());
        if front_gen != self.front_seen {
            self.active_stale = true;
        }
        if !self.active_stale || !self.engine.is_idle() {
            return;
        }
        let front = FRONT.with(|f| f.borrow().clone());
        if front_gen != self.front_seen {
            self.front_seen = front_gen;
            if let Some(app) = &front {
                let own = self.settings.profile(app).is_some();
                crate::debug!("front app: {app} ({})", if own { "its own settings" } else { "settings for all apps" });
            }
        }
        self.active = self.settings.for_app(front.as_deref());
        self.active_stale = false;
    }

    /// The settings were edited or reloaded: drop any gesture and use them right away.
    fn settings_replaced(&mut self) {
        self.engine.reset(); // also lets go of any key held for a slide
        self.engine_done();
        self.active_stale = true;
        self.refresh_active();
    }
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    static FREEZE: Cell<bool> = const { Cell::new(false) };
    static MAIN: Cell<isize> = const { Cell::new(0) };
    static BRIGHTNESS: RefCell<Option<brightness::Brightness>> = const { RefCell::new(None) };
    static TASKBAR_CREATED: Cell<u32> = const { Cell::new(0) };
    /// The app in front (see `settings::app_name`), kept up to date by a WinEvent hook.
    static FRONT: RefCell<Option<String>> = const { RefCell::new(None) };
    /// Bumped whenever FRONT changes.
    static FRONT_GEN: Cell<u64> = const { Cell::new(0) };
    static FOREGROUND_HOOK: Cell<isize> = const { Cell::new(0) };
    /// The settings were edited in Margin and not written yet.
    static UNSAVED: Cell<bool> = const { Cell::new(false) };
    /// settings.json's modification time when Margin last read or wrote it.
    static SEEN: Cell<u64> = const { Cell::new(0) };
}

/// Runs `f` on the app state. Returns None if the state is already borrowed (a re-entrant message).
pub fn with<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    STATE.with(|c| {
        let mut guard = c.try_borrow_mut().ok()?;
        guard.as_mut().map(f)
    })
}

pub fn main_hwnd() -> HWND {
    HWND(MAIN.with(|m| m.get()) as *mut _)
}

// ---------------------------------------------------------------- called by the mouse hook

pub fn freeze_pointer() -> bool {
    FREEZE.with(|f| f.get())
}

pub fn mouse_down() -> bool {
    with(|s| {
        s.refresh_active();
        let swallow = s.engine.mouse_down(&s.tracker, &s.active, &RealEnv);
        s.engine_done();
        swallow
    })
    .unwrap_or(false)
}

pub fn mouse_up() {
    with(|s| {
        s.engine.mouse_up(&s.active);
        s.engine_done();
        s.refresh_active();
    });
}

/// Processes touch reports that are already waiting, so a click handled by the mouse hook
/// sees the touch frame that produced it.
pub fn drain_raw_input() {
    let hwnd = main_hwnd();
    let mut msg = MSG::default();
    while unsafe { PeekMessageW(&mut msg, hwnd, WM_INPUT, WM_INPUT, PM_REMOVE) }.as_bool() {
        with(|s| s.process_raw(msg.lParam));
        unsafe { DefWindowProcW(msg.hwnd, msg.message, msg.wParam, msg.lParam) };
    }
}

// ---------------------------------------------------------------- settings plumbing (used by the UI)

/// Edits the live settings. Call [`settings_changed`] afterwards.
pub fn edit_settings<R>(f: impl FnOnce(&mut Settings) -> R) -> Option<R> {
    with(|s| f(&mut s.settings))
}

pub fn read_settings<R>(f: impl FnOnce(&Settings) -> R) -> Option<R> {
    with(|s| f(&s.settings))
}

/// The settings were edited: reset any running gesture, update the tray tip, save shortly.
pub fn settings_changed() {
    let enabled = with(|s| {
        s.settings_replaced();
        s.settings.enabled
    })
    .unwrap_or(true);
    tray::set_tip(main_hwnd(), if enabled { "Margin" } else { "Margin (off)" });
    UNSAVED.with(|u| u.set(true));
    unsafe { SetTimer(main_hwnd(), TIMER_SAVE, SAVE_DELAY_MS, None) };
}

/// Writes the settings if they were edited since the last save. Nothing else writes settings.json, so a
/// file Margin couldn't read (and replaced with the defaults in memory) is left alone until the user
/// changes something.
fn save_if_edited() {
    if !UNSAVED.with(|u| u.replace(false)) {
        return;
    }
    with(|s| settings::save(&s.settings));
    SEEN.with(|m| m.set(settings::modified_ms()));
}

/// settings.json changed outside Margin: apply it, like the Linux version does.
fn check_settings_file() {
    let modified = settings::modified_ms();
    if modified == 0 || modified == SEEN.with(|m| m.get()) || UNSAVED.with(|u| u.get()) {
        return; // no file, nothing new, or an edit of ours is about to be written anyway
    }
    SEEN.with(|m| m.set(modified));
    match settings::read() {
        Ok(Some(new)) => {
            let enabled = new.enabled;
            with(|s| {
                s.settings = new;
                s.settings_replaced();
            });
            tray::set_tip(main_hwnd(), if enabled { "Margin" } else { "Margin (off)" });
            ui::settings_window::reload();
            crate::log::info("settings.json changed, settings reloaded");
        }
        Ok(None) => {}
        Err(e) => crate::log::error(format!("{e}; keeping the current settings")),
    }
}

pub fn touchpad_name() -> Option<String> {
    with(|s| s.touchpad.device_name().map(str::to_owned)).flatten()
}

/// Runs the effects the engine queued. Executed from the message loop, never inside the hook.
fn flush() {
    let list = with(|s| {
        s.flush_posted = false;
        std::mem::take(&mut s.pending)
    })
    .unwrap_or_default();
    for effect in list {
        run_effect(effect);
    }
}

fn run_effect(effect: Effect) {
    match effect {
        Effect::Tap { vk, times } => keys::tap(vk, times),
        Effect::Combo { vk, mods, times } => keys::combo(vk, &mods, times),
        Effect::CtrlWheel(n) => keys::ctrl_wheel(n),
        Effect::Wheel { units, horizontal } => keys::wheel(units, horizontal),
        Effect::Hold { vk, up } => keys::hold(vk, up),
        Effect::BrightnessRefresh => BRIGHTNESS.with(|b| {
            if let Some(b) = b.borrow().as_ref() {
                b.refresh();
            }
        }),
        Effect::BrightnessNudge(delta) => {
            let value = BRIGHTNESS.with(|b| b.borrow().as_ref().map_or(-1, |b| b.nudge(delta)));
            if value >= 0 {
                hud::show_level(&format!("Brightness  {value}%"), value);
            } else {
                hud::show_text("Brightness not available");
            }
        }
        Effect::HudText(text) => hud::show_text(&text),
        Effect::HideHud(ms) => hud::hide_after(ms),
        Effect::RunClick(spec) => actions::run(&spec),
        Effect::StartClickTimer => unsafe {
            SetTimer(main_hwnd(), TIMER_CLICK, MULTI_CLICK_MS, None);
        },
        Effect::StopClickTimer => unsafe {
            let _ = KillTimer(main_hwnd(), TIMER_CLICK);
        },
        Effect::StartHoldTimer => unsafe {
            SetTimer(main_hwnd(), TIMER_HOLD, HOLD_MS, None);
        },
        Effect::StopHoldTimer => unsafe {
            let _ = KillTimer(main_hwnd(), TIMER_HOLD);
        },
        Effect::StartMomentumTimer => unsafe {
            SetTimer(main_hwnd(), TIMER_MOMENTUM, MOMENTUM_TICK_MS, None);
        },
        Effect::StopMomentumTimer => unsafe {
            let _ = KillTimer(main_hwnd(), TIMER_MOMENTUM);
        },
    }
}

// ---------------------------------------------------------------- window and message loop

/// Creates the hidden window that owns input, tray and timers, and runs the message loop.
pub fn run(first_run: bool) -> i32 {
    let settings = settings::load();
    SEEN.with(|m| m.set(settings::modified_ms()));
    set_front(unsafe { GetForegroundWindow() });
    STATE.with(|c| *c.borrow_mut() = Some(State::new(settings)));
    BRIGHTNESS.with(|b| *b.borrow_mut() = Some(brightness::Brightness::new()));

    let instance = unsafe { GetModuleHandleW(None) }.unwrap_or_default();
    let class = w!("MarginMainWindow");
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(wnd_proc),
        hInstance: instance.into(),
        lpszClassName: class,
        ..Default::default()
    };
    unsafe { RegisterClassExW(&wc) };
    // An invisible top-level window (not message-only) so it also receives broadcasts such as TaskbarCreated.
    let hwnd = match unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class,
            w!("Margin"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            instance,
            None,
        )
    } {
        Ok(h) => h,
        Err(e) => {
            crate::log::error(format!("could not create the main window: {e}"));
            return 1;
        }
    };
    MAIN.with(|m| m.set(hwnd.0 as isize));
    MAIN_RAW.store(hwnd.0 as isize, std::sync::atomic::Ordering::Relaxed);
    TASKBAR_CREATED.with(|t| t.set(unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) }));

    hud::create();
    if let Some(demo) = std::env::args().find(|a| a.starts_with("--hud-demo")) {
        // developer aid: show the HUD for a few seconds to check how it looks
        if demo == "--hud-demo-seek" {
            hud::show_text("\u{23E9}  +10s");
        } else {
            hud::show_level("Brightness  70%", 70);
        }
        hud::hide_after(8000);
    }
    RawTouchpad::register(hwnd);
    hook::install();
    watch_front_app();
    let _ = unsafe { WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION) };
    tray::add(hwnd);
    unsafe { SetTimer(hwnd, TIMER_WATCH, WATCH_MS, None) };

    crate::log::info(format!("Margin {} started", env!("CARGO_PKG_VERSION")));
    if first_run {
        settings_changed(); // writes settings.json
        ui::settings_window::show();
    }

    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if ui::settings_window::pre_translate(&msg) {
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    shutdown(hwnd);
    msg.wParam.0 as i32
}

fn shutdown(hwnd: HWND) {
    unsafe {
        let _ = KillTimer(hwnd, TIMER_SAVE);
        let _ = KillTimer(hwnd, TIMER_WATCH);
    }
    with(|s| {
        s.engine.reset(); // releases any held keys
        s.engine_done();
    });
    save_if_edited();
    flush();
    hook::uninstall();
    let front_hook = FOREGROUND_HOOK.with(|h| h.replace(0));
    if front_hook != 0 {
        let _ = unsafe { UnhookWinEvent(HWINEVENTHOOK(front_hook as *mut _)) };
    }
    tray::remove(hwnd);
    hud::destroy();
    let _ = unsafe { WTSUnRegisterSessionNotification(hwnd) };
    BRIGHTNESS.with(|b| *b.borrow_mut() = None);
    crate::log::info("Margin quit");
}

/// The hook and raw input registration can be dropped across sleep or the lock screen.
fn rearm(why: &str) {
    crate::log::info(format!("re-arming input after {why}"));
    with(|s| {
        s.engine.reset();
        s.tracker.clear();
        s.engine_done();
    });
    RawTouchpad::register(main_hwnd());
    hook::install();
}

pub fn quit() {
    unsafe {
        let _ = DestroyWindow(main_hwnd());
    }
}

fn toggle_enabled() {
    edit_settings(|s| s.enabled = !s.enabled);
    settings_changed();
    ui::settings_window::enabled_changed();
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_INPUT => {
            with(|s| s.process_raw(lparam));
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        WM_INPUT_DEVICE_CHANGE => {
            with(|s| s.touchpad.device_changed(lparam.0 as isize, &mut s.tracker));
        }
        WM_APP_FLUSH => flush(),
        WM_APP_SHOW_SETTINGS => ui::settings_window::show(),
        WM_TIMER => match wparam.0 {
            TIMER_CLICK => {
                with(|s| {
                    s.engine.click_timer_fired(&s.active);
                    s.engine_done();
                    s.refresh_active();
                });
            }
            TIMER_HOLD => {
                let _ = KillTimer(hwnd, TIMER_HOLD);
                with(|s| {
                    s.engine.hold_timer_fired(&s.active);
                    s.engine_done();
                });
            }
            TIMER_MOMENTUM => {
                with(|s| {
                    s.engine.momentum_tick(&RealEnv);
                    s.engine_done();
                });
            }
            TIMER_SAVE => {
                let _ = KillTimer(hwnd, TIMER_SAVE);
                save_if_edited();
            }
            TIMER_WATCH => check_settings_file(),
            _ => {}
        },
        WM_APP_TRAY => match (lparam.0 & 0xFFFF) as u32 {
            WM_LBUTTONUP => ui::settings_window::show(),
            WM_RBUTTONUP | WM_CONTEXTMENU => {
                let enabled = read_settings(|s| s.enabled).unwrap_or(true);
                tray::show_menu(hwnd, enabled);
            }
            _ => {}
        },
        WM_COMMAND => match wparam.0 & 0xFFFF {
            CMD_SETTINGS => ui::settings_window::show(),
            CMD_TOGGLE => toggle_enabled(),
            CMD_LOG => {
                let _ = std::process::Command::new("notepad.exe").arg(crate::log::file_path()).spawn();
            }
            CMD_QUIT => quit(),
            _ => {}
        },
        WM_POWERBROADCAST => {
            if wparam.0 == PBT_APMRESUMEAUTOMATIC || wparam.0 == PBT_APMRESUMESUSPEND {
                rearm("resume");
            }
            return LRESULT(1);
        }
        WM_WTSSESSION_CHANGE => {
            if wparam.0 == WTS_SESSION_UNLOCK || wparam.0 == WTS_CONSOLE_CONNECT {
                rearm("unlock");
            }
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            return LRESULT(0);
        }
        other => {
            if other != 0 && other == TASKBAR_CREATED.with(|t| t.get()) {
                tray::add(hwnd); // Explorer restarted: put the icon back
            }
        }
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

// ---------------------------------------------------------------- the app in front, for per-app settings

/// Records which app owns `hwnd`, the new window in front.
fn set_front(hwnd: HWND) {
    let name = actions::process_name(hwnd);
    let changed = FRONT.with(|f| {
        let mut f = f.borrow_mut();
        let changed = *f != name;
        *f = name;
        changed
    });
    if changed {
        FRONT_GEN.with(|g| g.set(g.get() + 1));
    }
}

/// Follows the window in front. The callback runs from the message loop, outside the mouse hook,
/// and only records the app; `State::refresh_active` applies it when no gesture is in progress.
fn watch_front_app() {
    let hook = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(on_foreground),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS, // Margin's own settings window doesn't count
        )
    };
    if hook.is_invalid() {
        crate::log::error("could not follow the window in front; per-app settings won't apply");
    }
    FOREGROUND_HOOK.with(|h| h.set(hook.0 as isize));
}

unsafe extern "system" fn on_foreground(_: HWINEVENTHOOK, _event: u32, hwnd: HWND, _: i32, _: i32, _: u32, _: u32) {
    set_front(hwnd);
    with(|s| s.refresh_active());
}

static MAIN_RAW: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

/// The main window handle as a plain number, readable from other threads.
pub fn main_hwnd_raw() -> isize {
    MAIN_RAW.load(std::sync::atomic::Ordering::Relaxed)
}

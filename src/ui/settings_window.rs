//! The settings window: pick a section on the touchpad map, then choose what it does.
//! Every change applies immediately and is saved. Native Win32 controls, laid out by hand in
//! 96-DPI units and scaled to the monitor.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::ffi::c_void;

use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::Dialogs::{GetOpenFileNameW, OFN_FILEMUSTEXIST, OFN_PATHMUSTEXIST, OPENFILENAMEW};
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow, SystemParametersInfoForDpi};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, GetKeyState, SetFocus, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT};
use windows::Win32::UI::WindowsAndMessaging::*;

use super::pad_view::{self, WM_PAD_CLICKED};
use crate::app;
use crate::keys;
use crate::labels;
use crate::settings::{self, AppProfile, ClickAction, Section, SectionSettings, Settings, SlideAction, Speed, CORNER_SLOTS, LONG_PRESS_SLOT};
use crate::startup;

// control ids
const ID_ENABLED: u16 = 100;
const ID_ACTIVATION: u16 = 101;
const ID_SPEED: u16 = 102;
const ID_STARTUP: u16 = 103;
const ID_PROFILE: u16 = 104;
const ID_ADD_APP: u16 = 105;
const ID_REMOVE_APP: u16 = 106;
const ID_SLIDE: u16 = 200;
const ID_PRESET: u16 = 201;
const ID_FWD_BOX: u16 = 202;
const ID_FWD_REC: u16 = 203;
const ID_BACK_BOX: u16 = 204;
const ID_BACK_REC: u16 = 205;
const ID_HOLD: u16 = 206;
const ID_CUSTOMIZE: u16 = 207;
const ID_OVERRIDE: u16 = 208;
const ID_NEEDS_CLICK: u16 = 209;

const TIP_ALL_APPS: &str =
    "Click a section on the touchpad to change it. Orange dots show your fingers. Hold Ctrl + Shift while sliding for fine steps.";
const ID_CORNER_BASE: u16 = 300; // + slot * 10: +0 combo, +1 value, +2 browse; slots 0-2 clicks, 3 hold

const WM_REBUILD: u32 = WM_APP + 20;

// raw control constants (kept numeric to stay independent of the generated wrapper types)
const BS_PUSHBUTTON: u32 = 0;
const BS_AUTOCHECKBOX: u32 = 3;
const BS_3STATE: u32 = 5;
const BS_GROUPBOX: u32 = 7;
const CBS_DROPDOWNLIST: u32 = 3;
const ES_AUTOHSCROLL: u32 = 0x80;
const SS_NOPREFIX: u32 = 0x80;
const CB_ADDSTRING: u32 = 0x143;
const CB_GETCURSEL: u32 = 0x147;
const CB_SETCURSEL: u32 = 0x14E;
const BM_GETCHECK: u32 = 0xF0;
const BM_SETCHECK: u32 = 0xF1;
const EM_SETCUEBANNER: u32 = 0x1501;
const CBN_SELCHANGE: u32 = 1;
const EN_CHANGE: u32 = 0x300;
const WM_DPICHANGED_: u32 = 0x02E0;
const TBM_GETPOS: u32 = 0x0400;
const DLGC_WANTALLKEYS: isize = 0x0004;

const SHORTCUT_SUBCLASS_ID: usize = 1;

const KEY_PRESETS: [(&str, &str, &str, bool); 6] = [
    ("Alt+Tab: switch windows", "Alt+Tab", "Alt+Shift+Tab", true),
    ("Ctrl+Tab: switch browser tabs", "Ctrl+Tab", "Ctrl+Shift+Tab", false),
    ("Ctrl+PageDown / PageUp: next / previous tab", "Ctrl+PageDown", "Ctrl+PageUp", false),
    ("Page Up / Page Down: scroll", "PageUp", "PageDown", false),
    ("Right / Left arrow", "Right", "Left", false),
    ("Ctrl+Y / Ctrl+Z: redo / undo", "Ctrl+Y", "Ctrl+Z", false),
];

struct Ui {
    hwnd: HWND,
    dpi: u32,
    font: HFONT,
    bold: HFONT,
    /// larger bold font of the on/off switch
    big: HFONT,
    icon_big: HICON,
    icon_small: HICON,
    /// static parts of the window
    fixed: Vec<HWND>,
    enabled: HWND,
    activation: HWND,
    speed: HWND,
    startup: HWND,
    /// "Settings for:" list of All apps and the per-app profiles, with its Add / Remove buttons
    profile: HWND,
    add_app: HWND,
    remove_app: HWND,
    pad: HWND,
    group: HWND,
    edge_trk: HWND,
    corner_trk: HWND,
    tip: HWND,
    /// controls of the section editor, rebuilt whenever the selection or an action changes
    detail: Vec<HWND>,
    gray: HashSet<isize>,
    bad_boxes: HashSet<isize>,
    /// (record button id, its shortcut box)
    record_pairs: Vec<(u16, HWND)>,
    bad_brush: HBRUSH,
}

thread_local! {
    static UI: RefCell<Option<Ui>> = const { RefCell::new(None) };
    /// True while the program itself fills controls, so their change notifications are ignored.
    static LOADING: Cell<bool> = const { Cell::new(false) };
    /// The shortcut box currently listening for a key combination (0 = none).
    static RECORDING: Cell<isize> = const { Cell::new(0) };
    static RECORD_BUTTON: Cell<isize> = const { Cell::new(0) };
    /// The app whose settings are shown (`None` = all apps).
    static PROFILE: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// The app whose settings the window shows, or `None` for all apps.
pub fn selected_profile() -> Option<String> {
    PROFILE.with(|p| p.borrow().clone())
}

/// The settings the window shows: those for all apps, or the chosen app's.
pub fn view_settings() -> Settings {
    let profile = selected_profile();
    app::read_settings(|s| s.for_app(profile.as_deref())).unwrap_or_default()
}

/// Whether the chosen app has its own settings for this section.
pub fn overrides(section: Section) -> bool {
    match selected_profile() {
        Some(app) => app::read_settings(|s| s.overrides(&app, section)).unwrap_or(false),
        None => false,
    }
}

/// Edits a section: the chosen app's own copy, or the one for all apps.
fn edit_section<R>(section: Section, f: impl FnOnce(&mut SectionSettings) -> R) -> Option<R> {
    let profile = selected_profile();
    app::edit_settings(|s| f(s.section_for_edit(section, profile.as_deref())))
}

fn ui<R>(f: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    UI.with(|c| c.try_borrow_mut().ok().and_then(|mut g| g.as_mut().map(f)))
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn px(v: f32, scale: f32) -> i32 {
    (v * scale).round() as i32
}

// ---------------------------------------------------------------- public entry points

/// Opens the settings window, or brings the open one to the front.
pub fn show() {
    if let Some(hwnd) = ui(|u| u.hwnd) {
        unsafe {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            let _ = SetForegroundWindow(hwnd);
        }
        return;
    }
    create();
}

/// The settings were replaced from settings.json: show the new values if the window is open.
pub fn reload() {
    let Some((hwnd, enabled, activation, speed, edge_trk, corner_trk, pad)) =
        ui(|u| (u.hwnd, u.enabled, u.activation, u.speed, u.edge_trk, u.corner_trk, u.pad))
    else {
        return;
    };
    let st = app::read_settings(|s| s.clone()).unwrap_or_default();
    let pct = |f: f64, lo: i32, hi: i32| ((f * 100.0).round() as i32).clamp(lo, hi);
    end_recording(); // the box being recorded into is about to be rebuilt
    LOADING.with(|l| l.set(true));
    unsafe {
        set_all_click_box(activation, &st);
        SendMessageW(speed, CB_SETCURSEL, WPARAM(st.speed as usize), LPARAM(0));
        refresh_switch_control(enabled, st.enabled);
        SendMessageW(edge_trk, TBM_SETPOS, WPARAM(1), LPARAM(pct(st.edge_depth, 5, 25) as isize));
        SendMessageW(corner_trk, TBM_SETPOS, WPARAM(1), LPARAM(pct(st.corner_size, 8, 30) as isize));
        let _ = InvalidateRect(pad, None, false);
    }
    LOADING.with(|l| l.set(false));
    fill_profiles();
    post_rebuild(hwnd); // the section editor reads the new settings when it is rebuilt
}

/// Lets the dialog manager handle Tab / arrow keys for the window's controls.
pub fn pre_translate(msg: &MSG) -> bool {
    if RECORDING.with(|r| r.get()) != 0 {
        return false; // a key combination is being recorded: every key belongs to the box
    }
    let Some(hwnd) = ui(|u| u.hwnd) else { return false };
    unsafe { (msg.hwnd == hwnd || IsChild(hwnd, msg.hwnd).as_bool()) && IsDialogMessageW(hwnd, msg).as_bool() }
}

// ---------------------------------------------------------------- creation

fn create() {
    pad_view::register();
    unsafe {
        let instance = GetModuleHandleW(None).unwrap_or_default();
        let class = w!("MarginSettingsWindow");
        let icon_big = crate::tray::make_icon(32);
        let icon_small = crate::tray::make_icon(16);
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wnd_proc),
            hInstance: instance.into(),
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: HBRUSH((COLOR_3DFACE.0 + 1) as usize as *mut c_void),
            hIcon: icon_big,
            hIconSm: icon_small,
            ..Default::default()
        };
        RegisterClassExW(&wc);

        let hwnd = match CreateWindowExW(
            WS_EX_CONTROLPARENT,
            class,
            w!("Margin \u{2014} touchpad edges and corners"),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            900,
            640,
            None,
            None,
            instance,
            None,
        ) {
            Ok(h) => h,
            Err(e) => {
                crate::log::error(format!("could not create the settings window: {e}"));
                return;
            }
        };

        let dpi = GetDpiForWindow(hwnd).max(96);
        let scale = dpi as f32 / 96.0;
        // size the window for a 900x580 client area at this DPI, centred on the screen
        let mut want = RECT { left: 0, top: 0, right: px(900.0, scale), bottom: px(600.0, scale) };
        let _ = AdjustWindowRectExForDpi(&mut want, WS_OVERLAPPEDWINDOW, false, WS_EX_CONTROLPARENT, dpi);
        let (ww, wh) = (want.right - want.left, want.bottom - want.top);
        let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
        let _ = SetWindowPos(hwnd, None, (sw - ww) / 2, (sh - wh) / 2, ww, wh, SWP_NOZORDER | SWP_NOACTIVATE);

        let (font, bold, big) = make_fonts(dpi);
        UI.with(|c| {
            *c.borrow_mut() = Some(Ui {
                hwnd,
                dpi,
                font,
                bold,
                big,
                icon_big,
                icon_small,
                fixed: Vec::new(),
                enabled: HWND::default(),
                activation: HWND::default(),
                speed: HWND::default(),
                startup: HWND::default(),
                profile: HWND::default(),
                add_app: HWND::default(),
                remove_app: HWND::default(),
                pad: HWND::default(),
                group: HWND::default(),
                edge_trk: HWND::default(),
                corner_trk: HWND::default(),
                tip: HWND::default(),
                detail: Vec::new(),
                gray: HashSet::new(),
                bad_boxes: HashSet::new(),
                record_pairs: Vec::new(),
                bad_brush: CreateSolidBrush(COLORREF(255 | 228 << 8 | 228 << 16)),
            });
        });
        create_controls(hwnd);
        layout(hwnd);
        build_detail(pad_view::selected());
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }
}

/// Segoe UI at the system message-box size for this DPI, plus a bold variant.
fn make_fonts(dpi: u32) -> (HFONT, HFONT, HFONT) {
    unsafe {
        let mut ncm = NONCLIENTMETRICSW { cbSize: std::mem::size_of::<NONCLIENTMETRICSW>() as u32, ..Default::default() };
        let ok = SystemParametersInfoForDpi(
            SPI_GETNONCLIENTMETRICS.0,
            ncm.cbSize,
            Some(&mut ncm as *mut _ as *mut c_void),
            0,
            dpi,
        )
        .is_ok();
        let mut lf = if ok { ncm.lfMessageFont } else { LOGFONTW::default() };
        if !ok {
            lf.lfHeight = -((9.0 * dpi as f32 / 72.0).round() as i32);
            for (i, c) in "Segoe UI".encode_utf16().enumerate() {
                lf.lfFaceName[i] = c;
            }
        }
        let normal = CreateFontIndirectW(&lf);
        lf.lfWeight = FW_BOLD.0 as i32;
        let bold = CreateFontIndirectW(&lf);
        lf.lfHeight = (lf.lfHeight as f32 * 1.35).round() as i32; // the on/off switch
        let big = CreateFontIndirectW(&lf);
        (normal, bold, big)
    }
}

#[allow(clippy::too_many_arguments)]
fn make_child(parent: HWND, class: PCWSTR, text: &str, style: u32, ex: u32, id: u16, font: HFONT) -> HWND {
    unsafe {
        let instance = GetModuleHandleW(None).unwrap_or_default();
        let t = wide(text);
        let h = CreateWindowExW(
            WINDOW_EX_STYLE(ex),
            class,
            PCWSTR(t.as_ptr()),
            WINDOW_STYLE(style | WS_CHILD.0 | WS_VISIBLE.0),
            0,
            0,
            10,
            10,
            parent,
            HMENU(id as usize as *mut c_void),
            instance,
            None,
        )
        .unwrap_or_default();
        SendMessageW(h, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
        h
    }
}

fn create_controls(hwnd: HWND) {
    let Some((font, _bold)) = ui(|u| (u.font, u.bold)) else { return };
    let st = app::read_settings(|s| s.clone()).unwrap_or_default();
    let tab = WS_TABSTOP.0;

    // drawn as a large on/off switch (see draw_switch); the text is what screen readers announce
    let enabled = make_child(hwnd, w!("BUTTON"), switch_text(st.enabled), BS_OWNERDRAW as u32 | tab, 0, ID_ENABLED, font);
    // one box for every edge and corner at once; it shows a dash when they differ
    let activation = make_child(hwnd, w!("BUTTON"), "Every edge and corner needs a click", BS_3STATE | tab, 0, ID_ACTIVATION, font);
    let speed = make_child(hwnd, w!("COMBOBOX"), "", CBS_DROPDOWNLIST | WS_VSCROLL.0 | tab, 0, ID_SPEED, font);
    let startup = make_child(hwnd, w!("BUTTON"), "Start with Windows", BS_AUTOCHECKBOX | tab, 0, ID_STARTUP, font);
    let cap_speed = make_child(hwnd, w!("STATIC"), "Speed:", 0, 0, 0, font);
    let cap_profile = make_child(hwnd, w!("STATIC"), "Settings for:", 0, 0, 0, font);
    let profile = make_child(hwnd, w!("COMBOBOX"), "", CBS_DROPDOWNLIST | WS_VSCROLL.0 | tab, 0, ID_PROFILE, font);
    let add_app = make_child(hwnd, w!("BUTTON"), "Add an app\u{2026}", BS_PUSHBUTTON | tab, 0, ID_ADD_APP, font);
    let remove_app = make_child(hwnd, w!("BUTTON"), "Remove", BS_PUSHBUTTON | tab, 0, ID_REMOVE_APP, font);

    LOADING.with(|l| l.set(true));
    unsafe {
        set_all_click_box(activation, &st);
        for text in ["Slow", "Medium", "Fast"] {
            SendMessageW(speed, CB_ADDSTRING, WPARAM(0), LPARAM(wide(text).as_ptr() as isize));
        }
        SendMessageW(speed, CB_SETCURSEL, WPARAM(st.speed as usize), LPARAM(0));
        SendMessageW(startup, BM_SETCHECK, WPARAM(startup::enabled() as usize), LPARAM(0));
    }
    LOADING.with(|l| l.set(false));

    let pad = make_child(hwnd, pad_view::CLASS, "", WS_TABSTOP.0, 0, 0, font);
    let group = make_child(hwnd, w!("BUTTON"), "", BS_GROUPBOX, 0, 0, font);

    let edge_cap = make_child(hwnd, w!("STATIC"), "Edge size", 0, 0, 0, font);
    let corner_cap = make_child(hwnd, w!("STATIC"), "Corner size", 0, 0, 0, font);
    let edge_trk = make_child(hwnd, w!("msctls_trackbar32"), "", TBS_AUTOTICKS | tab, 0, 0, font);
    let corner_trk = make_child(hwnd, w!("msctls_trackbar32"), "", TBS_AUTOTICKS | tab, 0, 0, font);
    let pct = |f: f64, lo: i32, hi: i32| ((f * 100.0).round() as i32).clamp(lo, hi);
    unsafe {
        SendMessageW(edge_trk, TBM_SETRANGE, WPARAM(1), LPARAM(5 | (25 << 16)));
        SendMessageW(edge_trk, TBM_SETTICFREQ, WPARAM(5), LPARAM(0));
        SendMessageW(edge_trk, TBM_SETPOS, WPARAM(1), LPARAM(pct(st.edge_depth, 5, 25) as isize));
        SendMessageW(corner_trk, TBM_SETRANGE, WPARAM(1), LPARAM(8 | (30 << 16)));
        SendMessageW(corner_trk, TBM_SETTICFREQ, WPARAM(2), LPARAM(0));
        SendMessageW(corner_trk, TBM_SETPOS, WPARAM(1), LPARAM(pct(st.corner_size, 8, 30) as isize));
    }
    let tip = make_child(
        hwnd,
        w!("STATIC"),
        TIP_ALL_APPS,
        SS_NOPREFIX,
        0,
        0,
        font,
    );

    ui(|u| {
        u.fixed = vec![cap_speed, edge_cap, corner_cap, cap_profile];
        u.profile = profile;
        u.add_app = add_app;
        u.remove_app = remove_app;
        u.enabled = enabled;
        u.activation = activation;
        u.speed = speed;
        u.startup = startup;
        u.pad = pad;
        u.group = group;
        u.edge_trk = edge_trk;
        u.corner_trk = corner_trk;
        u.tip = tip;
        u.gray.insert(tip.0 as isize);
    });
    fill_profiles();
}

/// Fills the "Settings for" list and selects the chosen app, falling back to all apps when that app's
/// profile is gone.
fn fill_profiles() {
    let Some((profile, remove, tip)) = ui(|u| (u.profile, u.remove_app, u.tip)) else { return };
    let apps: Vec<String> = app::read_settings(|s| s.apps.iter().map(|p| settings::app_name(&p.app)).collect()).unwrap_or_default();
    let chosen = selected_profile().map(|a| settings::app_name(&a)).filter(|a| apps.contains(a));
    PROFILE.with(|p| *p.borrow_mut() = chosen.clone());
    LOADING.with(|l| l.set(true));
    unsafe {
        SendMessageW(profile, CB_RESETCONTENT, WPARAM(0), LPARAM(0));
        SendMessageW(profile, CB_ADDSTRING, WPARAM(0), LPARAM(wide("All apps").as_ptr() as isize));
        for a in &apps {
            SendMessageW(profile, CB_ADDSTRING, WPARAM(0), LPARAM(wide(a).as_ptr() as isize));
        }
        let index = chosen.and_then(|c| apps.iter().position(|a| *a == c)).map_or(0, |i| i + 1);
        SendMessageW(profile, CB_SETCURSEL, WPARAM(index), LPARAM(0));
        let _ = EnableWindow(remove, index > 0);
        // the hint line explains what the chosen view edits
        let text = match selected_profile() {
            Some(app) => format!("Orange: different in {app}. The rest uses the settings for all apps until you tick \"Different in {app}\"."),
            None => TIP_ALL_APPS.to_string(),
        };
        let t = wide(&text);
        let _ = SetWindowTextW(tip, PCWSTR(t.as_ptr()));
    }
    LOADING.with(|l| l.set(false));
}

fn move_to(h: HWND, x: i32, y: i32, w: i32, hgt: i32) {
    unsafe {
        let _ = MoveWindow(h, x, y, w, hgt, true);
    }
}

/// Positions every fixed control for the current client size and DPI.
fn layout(hwnd: HWND) {
    let Some((u_dpi, fixed, enabled, activation, speed, startup, pad, group, edge_trk, corner_trk, tip, profile, add_app, remove_app)) = ui(|u| {
        (u.dpi, u.fixed.clone(), u.enabled, u.activation, u.speed, u.startup, u.pad, u.group, u.edge_trk, u.corner_trk, u.tip, u.profile, u.add_app, u.remove_app)
    }) else {
        return;
    };
    let s = u_dpi as f32 / 96.0;
    let mut client = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut client);
    }
    let (cw, ch) = (client.right, client.bottom);
    let m = px(12.0, s);

    // header, three rows; the first holds the larger on/off switch
    let row_h = px(24.0, s);
    let switch_h = px(34.0, s);
    let r1 = m;
    let r2 = r1 + switch_h + px(8.0, s);
    let r3 = r2 + row_h + px(8.0, s);
    let mid = (switch_h - row_h) / 2; // centres the first row's other controls on the switch
    move_to(enabled, m - px(2.0, s), r1, px(205.0, s), switch_h);
    move_to(activation, m + px(220.0, s), r1 + mid, px(330.0, s), row_h);
    move_to(fixed[0], m, r2 + px(4.0, s), px(50.0, s), px(20.0, s));
    move_to(speed, m + px(55.0, s), r2, px(100.0, s), px(160.0, s));
    move_to(startup, m + px(180.0, s), r2, px(180.0, s), row_h);
    move_to(fixed[3], m, r3 + px(4.0, s), px(90.0, s), px(20.0, s));
    move_to(profile, m + px(95.0, s), r3, px(200.0, s), px(220.0, s));
    move_to(add_app, m + px(305.0, s), r3 - px(1.0, s), px(110.0, s), px(26.0, s));
    move_to(remove_app, m + px(423.0, s), r3 - px(1.0, s), px(80.0, s), px(26.0, s));

    // footer
    let tip_h = px(20.0, s);
    let trk_h = px(32.0, s);
    let tip_y = ch - m - tip_h;
    let trk_y = tip_y - trk_h - px(4.0, s);
    move_to(tip, m, tip_y, cw - 2 * m, tip_h);
    move_to(fixed[1], m, trk_y + px(8.0, s), px(70.0, s), px(20.0, s));
    move_to(edge_trk, m + px(72.0, s), trk_y, px(170.0, s), trk_h);
    move_to(fixed[2], m + px(270.0, s), trk_y + px(8.0, s), px(80.0, s), px(20.0, s));
    move_to(corner_trk, m + px(352.0, s), trk_y, px(170.0, s), trk_h);

    // middle: pad on the left, editor group on the right
    let top = r3 + row_h + px(12.0, s);
    let bottom = trk_y - px(8.0, s);
    let group_w = px(330.0, s);
    let gap = px(10.0, s);
    move_to(pad, m, top, (cw - 2 * m - group_w - gap).max(px(200.0, s)), (bottom - top).max(px(100.0, s)));
    move_to(group, cw - m - group_w, top, group_w, (bottom - top).max(px(100.0, s)));
}

// ---------------------------------------------------------------- section editor

fn destroy_detail() {
    let list = ui(|u| {
        u.gray.clear();
        u.bad_boxes.clear();
        u.record_pairs.clear();
        std::mem::take(&mut u.detail)
    })
    .unwrap_or_default();
    unsafe {
        for h in list {
            let _ = DestroyWindow(h);
        }
        // the tip stays gray
    }
    ui(|u| {
        let tip = u.tip.0 as isize;
        u.gray.insert(tip);
    });
}

/// A small layout cursor for stacking controls inside the editor group.
struct Cursor {
    x: i32,
    y: i32,
    width: i32,
    scale: f32,
    parent: HWND,
    font: HFONT,
    bold: HFONT,
}

impl Cursor {
    fn gap(&mut self, v: f32) {
        self.y += px(v, self.scale);
    }

    fn register(&self, h: HWND, gray: bool) {
        ui(|u| {
            u.detail.push(h);
            if gray {
                u.gray.insert(h.0 as isize);
            }
        });
    }

    /// A wrapped paragraph of text.
    fn para(&mut self, text: &str, bold: bool, gray: bool, below: f32) -> HWND {
        let font = if bold { self.bold } else { self.font };
        let h = make_child(self.parent, w!("STATIC"), text, SS_NOPREFIX, 0, 0, font);
        let height = measure(self.parent, font, text, self.width);
        move_to(h, self.x, self.y, self.width, height);
        self.y += height + px(below, self.scale);
        self.register(h, gray);
        h
    }

    fn combo(&mut self, id: u16, items: &[&str], selected: usize, below: f32) -> HWND {
        let h = make_child(self.parent, w!("COMBOBOX"), "", CBS_DROPDOWNLIST | WS_VSCROLL.0 | WS_TABSTOP.0, 0, id, self.font);
        LOADING.with(|l| l.set(true));
        unsafe {
            for it in items {
                SendMessageW(h, CB_ADDSTRING, WPARAM(0), LPARAM(wide(it).as_ptr() as isize));
            }
            SendMessageW(h, CB_SETCURSEL, WPARAM(selected), LPARAM(0));
        }
        LOADING.with(|l| l.set(false));
        let row = px(24.0, self.scale);
        move_to(h, self.x, self.y, self.width, px(260.0, self.scale));
        self.y += row + px(below, self.scale);
        self.register(h, false);
        h
    }

    /// A checkbox on its own line.
    fn check(&mut self, id: u16, text: &str, checked: bool, below: f32) -> HWND {
        let h = make_child(self.parent, w!("BUTTON"), text, BS_AUTOCHECKBOX | WS_TABSTOP.0, 0, id, self.font);
        unsafe {
            SendMessageW(h, BM_SETCHECK, WPARAM(checked as usize), LPARAM(0));
        }
        move_to(h, self.x, self.y, self.width, px(22.0, self.scale));
        self.y += px(22.0 + below, self.scale);
        self.register(h, false);
        h
    }

    /// An edit box with an optional button to its right.
    fn edit_row(&mut self, id: u16, text: &str, button: Option<(u16, &str)>, shortcut: bool, below: f32) -> (HWND, Option<HWND>) {
        let row = px(24.0, self.scale);
        let btn_w = px(78.0, self.scale);
        let edit_w = if button.is_some() { self.width - btn_w - px(6.0, self.scale) } else { self.width };
        let edit = make_child(self.parent, w!("EDIT"), "", ES_AUTOHSCROLL | WS_TABSTOP.0, WS_EX_CLIENTEDGE.0, id, self.font);
        LOADING.with(|l| l.set(true));
        unsafe {
            let t = wide(text);
            let _ = SetWindowTextW(edit, PCWSTR(t.as_ptr()));
        }
        LOADING.with(|l| l.set(false));
        move_to(edit, self.x, self.y, edit_w, row);
        self.register(edit, false);
        if shortcut {
            unsafe {
                let _ = SetWindowSubclass(edit, Some(shortcut_proc), SHORTCUT_SUBCLASS_ID, 0);
            }
        }
        let btn = button.map(|(bid, label)| {
            let b = make_child(self.parent, w!("BUTTON"), label, BS_PUSHBUTTON | WS_TABSTOP.0, 0, bid, self.font);
            move_to(b, self.x + edit_w + px(6.0, self.scale), self.y, btn_w, row);
            self.register(b, false);
            b
        });
        self.y += row + px(below, self.scale);
        (edit, btn)
    }
}

/// Height needed to show `text` word-wrapped to `width` pixels in `font`.
fn measure(hwnd: HWND, font: HFONT, text: &str, width: i32) -> i32 {
    unsafe {
        let hdc = GetDC(hwnd);
        let old = SelectObject(hdc, font);
        let mut rc = RECT { left: 0, top: 0, right: width, bottom: 0 };
        let mut wt: Vec<u16> = text.encode_utf16().collect();
        DrawTextW(hdc, &mut wt, &mut rc, DT_CALCRECT | DT_WORDBREAK | DT_NOPREFIX);
        SelectObject(hdc, old);
        ReleaseDC(hwnd, hdc);
        rc.bottom + 2
    }
}

fn build_detail(section: Section) {
    destroy_detail();
    let Some((parent, group, dpi, font, bold)) = ui(|u| (u.hwnd, u.group, u.dpi, u.font, u.bold)) else { return };
    let scale = dpi as f32 / 96.0;
    unsafe {
        let t = wide(section.name());
        let _ = SetWindowTextW(group, PCWSTR(t.as_ptr()));
    }
    let mut r = RECT::default();
    unsafe {
        let _ = GetWindowRect(group, &mut r);
        let mut tl = windows::Win32::Foundation::POINT { x: r.left, y: r.top };
        let _ = ScreenToClient(parent, &mut tl);
        r.right -= r.left;
        r.left = tl.x;
        r.top = tl.y;
    }
    let pad = px(12.0, scale);
    let mut c = Cursor { x: r.left + pad, y: r.top + px(24.0, scale), width: r.right - 2 * pad, scale, parent, font, bold };
    let st = view_settings();
    let profile = selected_profile();
    let own = overrides(section);
    if let Some(app) = &profile {
        let check = make_child(parent, w!("BUTTON"), &format!("Different in {app}"), BS_AUTOCHECKBOX | WS_TABSTOP.0, 0, ID_OVERRIDE, bold);
        unsafe {
            SendMessageW(check, BM_SETCHECK, WPARAM(own as usize), LPARAM(0));
        }
        move_to(check, c.x, c.y, c.width, px(22.0, scale));
        c.y += px(30.0, scale);
        c.register(check, false);
    }
    let first_editor_control = ui(|u| u.detail.len()).unwrap_or(0);
    if section.is_corner() {
        build_corner(&mut c, section, &st);
    } else {
        build_edge(&mut c, section, &st);
    }
    if profile.is_some() && !own {
        // shown for reference: these are the all-apps settings, edited when "All apps" is chosen
        let list = ui(|u| u.detail[first_editor_control..].to_vec()).unwrap_or_default();
        for h in list {
            unsafe {
                let _ = EnableWindow(h, false);
            }
        }
    }
    unsafe {
        let _ = InvalidateRect(parent, None, true);
    }
}

fn build_edge(c: &mut Cursor, section: Section, st: &Settings) {
    let v = st.section(section).clone();
    c.para("Sliding along this edge controls:", true, false, 4.0);
    let labels_list: Vec<&str> = SlideAction::ALL.iter().map(|&a| labels::slide_long(a)).collect();
    c.combo(ID_SLIDE, &labels_list, v.slide as usize, 12.0);

    if v.slide == SlideAction::Custom {
        build_custom_keys(c, section, &v);
    } else if v.slide == SlideAction::Tabs {
        // "Switch tabs" is fixed to Alt+Tab; this is the way to other keys and the presets.
        c.para(
            "Sliding sends Alt+Tab and keeps Alt held, so the window picker stays open. To use other keys, or a preset \
             such as Ctrl+Tab for browser tabs, change the keys:",
            false,
            false,
            6.0,
        );
        let button = make_child(c.parent, w!("BUTTON"), "Change the keys\u{2026}", BS_PUSHBUTTON | WS_TABSTOP.0, 0, ID_CUSTOMIZE, c.font);
        move_to(button, c.x, c.y, px(150.0, c.scale), px(28.0, c.scale));
        c.y += px(38.0, c.scale);
        c.register(button, false);
    }

    let direction = if v.slide == SlideAction::Scroll {
        if section.is_vertical() {
            "Slide up to scroll up, down to scroll down, like dragging a scroll bar. The window under the pointer scrolls."
        } else {
            "Slide right to scroll right, left to scroll left. The window under the pointer scrolls."
        }
    } else if section.is_vertical() {
        "Slide up for more / next, down for less / previous."
    } else {
        "Slide right for forward / next, left for back / previous."
    };
    let click = st.needs_click(section);
    let how = if click {
        "Rest a finger on the edge, press until the touchpad clicks, and slide without letting go."
    } else {
        "Bring your finger in from the laptop body onto this edge, then slide along it. No click needed."
    };
    c.para(direction, false, false, 6.0);
    c.check(ID_NEEDS_CLICK, "Needs a click", click, 4.0);
    c.para(how, false, true, 6.0);
}

fn build_custom_keys(c: &mut Cursor, section: Section, v: &crate::settings::SectionSettings) {
    let (fwd, back) = if section.is_vertical() { ("Slide up", "Slide down") } else { ("Slide right", "Slide left") };

    c.para("Preset:", false, false, 2.0);
    let mut items: Vec<&str> = vec!["Choose a preset\u{2026}"];
    items.extend(KEY_PRESETS.iter().map(|p| p.0));
    c.combo(ID_PRESET, &items, 0, 8.0);

    c.para(&format!("{fwd} presses:"), false, false, 2.0);
    let (fwd_box, _) = c.edit_row(ID_FWD_BOX, v.forward.as_deref().unwrap_or(""), Some((ID_FWD_REC, "Record")), true, 6.0);
    c.para(&format!("{back} presses:"), false, false, 2.0);
    let (back_box, _) = c.edit_row(ID_BACK_BOX, v.back.as_deref().unwrap_or(""), Some((ID_BACK_REC, "Record")), true, 6.0);
    ui(|u| {
        u.record_pairs.push((ID_FWD_REC, fwd_box));
        u.record_pairs.push((ID_BACK_REC, back_box));
    });
    mark_validity(fwd_box);
    mark_validity(back_box);

    let hold = make_child(c.parent, w!("BUTTON"), "Hold shared modifiers while sliding", BS_AUTOCHECKBOX | WS_TABSTOP.0, 0, ID_HOLD, c.font);
    unsafe {
        SendMessageW(hold, BM_SETCHECK, WPARAM(v.hold_modifiers as usize), LPARAM(0));
    }
    move_to(hold, c.x, c.y, c.width, px(22.0, c.scale));
    c.y += px(28.0, c.scale);
    c.register(hold, false);

    c.para(
        "Click Record, then press the keys. Windows keeps a few combos (Alt+Tab, Win+\u{2026}) to itself, so type those \
         instead, like Alt+Tab or Win+Ctrl+Right. Each step of the slide presses the shortcut once. \"Hold shared \
         modifiers\" keeps Alt down until you lift your finger, like holding Alt while pressing Tab.",
        false,
        true,
        6.0,
    );
}

fn build_corner(c: &mut Cursor, section: Section, st: &Settings) {
    let v = st.section(section).clone();
    let click = st.needs_click(section);
    c.check(ID_NEEDS_CLICK, "Needs a click (untick: a tap is enough)", click, 8.0);
    c.para(if click { "Press this corner until the touchpad clicks:" } else { "Tap this corner:" }, true, false, 2.0);
    let names: Vec<&str> = ClickAction::ALL.iter().map(|&a| labels::click_long(a)).collect();
    for i in 0..CORNER_SLOTS {
        let spec = v.corner_slot(i);
        c.gap(6.0);
        let title = match (i, click) {
            (0, true) => "1 click".to_string(),
            (0, false) => "1 tap".to_string(),
            (LONG_PRESS_SLOT, true) => "Press and hold".to_string(),
            (LONG_PRESS_SLOT, false) => "Touch and hold".to_string(),
            (n, true) => format!("{} clicks", n + 1),
            (n, false) => format!("{} taps", n + 1),
        };
        c.para(&title, false, false, 2.0);
        let base = ID_CORNER_BASE + (i as u16) * 10;
        c.combo(base, &names, spec.action as usize, 4.0);
        if labels::needs_value(spec.action) {
            let browse = (spec.action == ClickAction::App).then_some((base + 2, "Browse\u{2026}"));
            let (edit, _) = c.edit_row(base + 1, spec.value.as_deref().unwrap_or(""), browse, spec.action == ClickAction::Keys, 2.0);
            let hint = wide(labels::placeholder(spec.action));
            unsafe {
                SendMessageW(edit, EM_SETCUEBANNER, WPARAM(1), LPARAM(hint.as_ptr() as isize));
            }
            if spec.action == ClickAction::Keys {
                mark_validity(edit); // only shortcuts are checked; apps, websites and commands are free text
            }
        }
    }
    c.gap(10.0);
    let hint = if click {
        "This corner reacts only to a real press, never to a tap. Press and hold means keeping the touchpad pressed for half a second. Untick \"Needs a click\" if your touchpad doesn't click here (many only click near the bottom)."
    } else {
        "A tap is a quick touch that doesn't move away; the tap's own click is hidden from apps. Touch and hold means resting the finger still for half a second. Unused corners stay normal touchpad."
    };
    c.para(hint, false, true, 0.0);
}

// ---------------------------------------------------------------- change handling

fn changed() {
    app::settings_changed();
    refresh_pad();
}

fn switch_text(on: bool) -> &'static str {
    if on { "Margin is on" } else { "Margin is off" }
}

/// The on/off switch shows the setting: repaint it, and keep its text (read by screen readers) in step.
fn refresh_switch_control(control: HWND, on: bool) {
    unsafe {
        let t = wide(switch_text(on));
        let _ = SetWindowTextW(control, PCWSTR(t.as_ptr()));
        let _ = InvalidateRect(control, None, true);
    }
}

/// The "every edge and corner needs a click" box: ticked, empty, or a dash when the sections differ.
fn set_all_click_box(control: HWND, st: &Settings) {
    const BST_INDETERMINATE: usize = 2;
    let state = match st.all_need_click() {
        Some(true) => 1,
        Some(false) => 0,
        None => BST_INDETERMINATE,
    };
    unsafe {
        SendMessageW(control, BM_SETCHECK, WPARAM(state), LPARAM(0));
    }
}

/// Margin was turned on or off elsewhere (the tray menu): update the switch if the window is open.
pub fn enabled_changed() {
    if let Some(control) = ui(|u| u.enabled) {
        refresh_switch_control(control, app::read_settings(|s| s.enabled).unwrap_or(true));
    }
}

/// Paints the on/off control as a large switch: a green track with the knob on the right when on,
/// a gray one with the knob on the left when off, and bold text saying which.
fn draw_switch(dis: &DRAWITEMSTRUCT) {
    let on = app::read_settings(|s| s.enabled).unwrap_or(true);
    let Some((big, dpi)) = ui(|u| (u.big, u.dpi)) else { return };
    let s = dpi as f32 / 96.0;
    let hdc = dis.hDC;
    let r = dis.rcItem;
    unsafe {
        FillRect(hdc, &r, GetSysColorBrush(COLOR_3DFACE));

        let track_h = px(22.0, s);
        let track_w = px(42.0, s);
        let x0 = r.left + px(4.0, s);
        let y0 = r.top + (r.bottom - r.top - track_h) / 2;
        let (fill, edge) = if on { (rgb(15, 123, 15), rgb(15, 123, 15)) } else { (rgb(255, 255, 255), rgb(110, 110, 115)) };
        let brush = CreateSolidBrush(fill);
        let pen = CreatePen(PS_SOLID, px(1.5, s).max(1), edge);
        let (old_b, old_p) = (SelectObject(hdc, brush), SelectObject(hdc, pen));
        let _ = RoundRect(hdc, x0, y0, x0 + track_w, y0 + track_h, track_h, track_h);
        SelectObject(hdc, old_b);
        SelectObject(hdc, old_p);
        let _ = DeleteObject(brush);
        let _ = DeleteObject(pen);

        // the knob
        let k = track_h - px(8.0, s);
        let kx = if on { x0 + track_w - px(4.0, s) - k } else { x0 + px(4.0, s) };
        let ky = y0 + px(4.0, s);
        let knob = CreateSolidBrush(if on { rgb(255, 255, 255) } else { rgb(110, 110, 115) });
        let no_pen = GetStockObject(NULL_PEN);
        let (old_b, old_p) = (SelectObject(hdc, knob), SelectObject(hdc, no_pen));
        let _ = Ellipse(hdc, kx, ky, kx + k + 1, ky + k + 1);
        SelectObject(hdc, old_b);
        SelectObject(hdc, old_p);
        let _ = DeleteObject(knob);

        // the words
        let old_f = SelectObject(hdc, big);
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, if on { rgb(15, 110, 15) } else { rgb(90, 90, 95) });
        let mut text: Vec<u16> = switch_text(on).encode_utf16().collect();
        let mut tr = RECT { left: x0 + track_w + px(10.0, s), top: r.top, right: r.right, bottom: r.bottom };
        DrawTextW(hdc, &mut text, &mut tr, DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);
        if dis.itemState.0 & ODS_FOCUS.0 != 0 && dis.itemState.0 & ODS_NOFOCUSRECT.0 == 0 {
            let mut measured = tr;
            DrawTextW(hdc, &mut text, &mut measured, DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | DT_CALCRECT);
            let h = measured.bottom - measured.top;
            let fy = r.top + (r.bottom - r.top - h) / 2;
            let focus = RECT { left: measured.left - px(3.0, s), top: fy - px(1.0, s), right: measured.right + px(3.0, s), bottom: fy + h + px(1.0, s) };
            let _ = DrawFocusRect(hdc, &focus);
        }
        SelectObject(hdc, old_f);
    }
}

fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16)
}

fn refresh_pad() {
    ui(|u| unsafe {
        let _ = InvalidateRect(u.pad, None, false);
    });
}

fn read_text(h: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(h) as usize;
        let mut buf = vec![0u16; len + 1];
        let n = GetWindowTextW(h, &mut buf) as usize;
        String::from_utf16_lossy(&buf[..n])
    }
}

fn set_text(h: HWND, text: &str) {
    LOADING.with(|l| l.set(true));
    unsafe {
        let t = wide(text);
        let _ = SetWindowTextW(h, PCWSTR(t.as_ptr()));
    }
    LOADING.with(|l| l.set(false));
}

fn combo_index(h: HWND) -> usize {
    let i = unsafe { SendMessageW(h, CB_GETCURSEL, WPARAM(0), LPARAM(0)) }.0;
    i.max(0) as usize
}

fn is_checked(h: HWND) -> bool {
    unsafe { SendMessageW(h, BM_GETCHECK, WPARAM(0), LPARAM(0)) }.0 == 1
}

/// Shortcut boxes turn red while their text isn't a shortcut Margin understands.
fn mark_validity(edit: HWND) {
    let text = read_text(edit);
    let bad = !text.is_empty() && keys::parse_shortcut(&text).is_none();
    let key = edit.0 as isize;
    ui(|u| {
        // only shortcut fields are validated: forward/back boxes and corner "Keys" boxes
        if bad {
            u.bad_boxes.insert(key);
        } else {
            u.bad_boxes.remove(&key);
        }
    });
    unsafe {
        let _ = InvalidateRect(edit, None, true);
    }
}

fn on_command(hwnd: HWND, wparam: WPARAM, lparam: LPARAM) {
    if LOADING.with(|l| l.get()) {
        return;
    }
    let id = (wparam.0 & 0xFFFF) as u16;
    let code = ((wparam.0 >> 16) & 0xFFFF) as u32;
    let control = HWND(lparam.0 as *mut c_void);
    let section = pad_view::selected();

    match id {
        ID_ENABLED if code == BN_CLICKED => {
            let on = app::edit_settings(|s| {
                s.enabled = !s.enabled;
                s.enabled
            })
            .unwrap_or(true);
            changed();
            refresh_switch_control(control, on);
        }
        ID_ACTIVATION if code == BN_CLICKED => {
            // ticked -> nothing needs a click; empty or mixed -> everything does
            let click = app::read_settings(|s| s.all_need_click() != Some(true)).unwrap_or(true);
            app::edit_settings(|s| s.set_all_need_click(click));
            changed();
            if let Some(st) = app::read_settings(|s| s.clone()) {
                set_all_click_box(control, &st);
            }
            build_detail(section); // the section's own box and how-to text
        }
        ID_NEEDS_CLICK if code == 0 => {
            let click = is_checked(control);
            edit_section(section, |v| v.needs_click = Some(click));
            changed();
            if let (Some(header), Some(st)) = (ui(|u| u.activation), app::read_settings(|s| s.clone())) {
                set_all_click_box(header, &st);
            }
            post_rebuild(hwnd); // titles say click or tap
        }
        ID_SPEED if code == CBN_SELCHANGE => {
            let speed = [Speed::Slow, Speed::Medium, Speed::Fast][combo_index(control).min(2)];
            app::edit_settings(|s| s.speed = speed);
            changed();
        }
        ID_STARTUP if code == 0 => startup::set_enabled(is_checked(control)),
        ID_PROFILE if code == CBN_SELCHANGE => {
            let index = combo_index(control);
            let apps: Vec<String> = app::read_settings(|s| s.apps.iter().map(|p| settings::app_name(&p.app)).collect()).unwrap_or_default();
            PROFILE.with(|p| *p.borrow_mut() = index.checked_sub(1).and_then(|i| apps.get(i).cloned()));
            fill_profiles();
            refresh_pad();
            post_rebuild(hwnd);
        }
        ID_ADD_APP if code == 0 => {
            if let Some(path) = browse_for_app(hwnd) {
                let name = settings::app_name(&path);
                if !name.is_empty() {
                    app::edit_settings(|s| {
                        if s.profile(&name).is_none() {
                            s.apps.push(AppProfile { app: name.clone(), sections: Default::default() });
                        }
                    });
                    changed();
                    PROFILE.with(|p| *p.borrow_mut() = Some(name));
                    fill_profiles();
                    refresh_pad();
                    post_rebuild(hwnd);
                }
            }
        }
        ID_REMOVE_APP if code == 0 => {
            if let Some(name) = selected_profile() {
                let question = wide(&format!("Remove the settings for {name}? It will use the settings for all apps again."));
                let answer = unsafe { MessageBoxW(hwnd, PCWSTR(question.as_ptr()), w!("Margin"), MB_YESNO | MB_ICONQUESTION) };
                if answer == IDYES {
                    app::edit_settings(|s| s.apps.retain(|p| settings::app_name(&p.app) != name));
                    changed();
                    PROFILE.with(|p| *p.borrow_mut() = None);
                    fill_profiles();
                    refresh_pad();
                    post_rebuild(hwnd);
                }
            }
        }
        ID_OVERRIDE if code == 0 => {
            if let Some(name) = selected_profile() {
                let own = is_checked(control);
                app::edit_settings(|s| s.set_override(&name, section, own));
                changed();
                post_rebuild(hwnd);
            }
        }
        ID_SLIDE if code == CBN_SELCHANGE => {
            let action = SlideAction::ALL[combo_index(control).min(SlideAction::ALL.len() - 1)];
            edit_section(section, |v| {
                v.slide = action;
                if action == SlideAction::Custom && v.forward.as_deref().unwrap_or("").trim().is_empty() && v.back.as_deref().unwrap_or("").trim().is_empty() {
                    v.forward = Some("Ctrl+Tab".into());
                    v.back = Some("Ctrl+Shift+Tab".into());
                    v.hold_modifiers = false;
                }
            });
            changed();
            post_rebuild(hwnd); // rebuild so the key fields appear or disappear
        }
        ID_PRESET if code == CBN_SELCHANGE => {
            let index = combo_index(control);
            if index > 0 {
                let (_, fwd, back, hold) = KEY_PRESETS[index - 1];
                edit_section(section, |v| {
                    v.forward = Some(fwd.into());
                    v.back = Some(back.into());
                    v.hold_modifiers = hold;
                });
                changed();
                post_rebuild(hwnd);
            }
        }
        ID_FWD_BOX | ID_BACK_BOX if code == EN_CHANGE => {
            let text = read_text(control);
            let value = (!text.is_empty()).then_some(text);
            edit_section(section, |v| {
                if id == ID_FWD_BOX {
                    v.forward = value;
                } else {
                    v.back = value;
                }
            });
            mark_validity(control);
            changed();
        }
        ID_FWD_REC | ID_BACK_REC if code == 0 => begin_recording(id),
        ID_CUSTOMIZE if code == 0 => {
            // start from what "Switch tabs" does today, so nothing changes until the user edits it
            edit_section(section, |v| {
                v.slide = SlideAction::Custom;
                v.forward = Some("Alt+Tab".into());
                v.back = Some("Alt+Shift+Tab".into());
                v.hold_modifiers = true;
            });
            changed();
            post_rebuild(hwnd); // shows the preset list and the key fields
        }
        ID_HOLD if code == 0 => {
            let on = is_checked(control);
            edit_section(section, |v| v.hold_modifiers = on);
            changed();
        }
        _ if (ID_CORNER_BASE..ID_CORNER_BASE + 10 * CORNER_SLOTS as u16).contains(&id) => {
            on_corner_command(hwnd, id, code, control, section)
        }
        _ => {}
    }
}

fn on_corner_command(hwnd: HWND, id: u16, code: u32, control: HWND, section: Section) {
    let slot = ((id - ID_CORNER_BASE) / 10) as usize;
    let part = (id - ID_CORNER_BASE) % 10;
    match part {
        0 if code == CBN_SELCHANGE => {
            let action = ClickAction::ALL[combo_index(control).min(ClickAction::ALL.len() - 1)];
            edit_section(section, |v| {
                let spec = v.corner_slot_mut(slot);
                spec.action = action;
                if !labels::needs_value(action) {
                    spec.value = None;
                }
            });
            changed();
            post_rebuild(hwnd);
        }
        1 if code == EN_CHANGE => {
            let text = read_text(control);
            let value = (!text.is_empty()).then_some(text);
            edit_section(section, |v| v.corner_slot_mut(slot).value = value);
            let is_keys = view_settings().section(section).corner_slot(slot).action == ClickAction::Keys;
            if is_keys {
                mark_validity(control);
            }
            changed();
        }
        2 if code == 0 => {
            if let Some(path) = browse_for_app(hwnd) {
                // the value edit sits right before the Browse button in id order
                let edit = unsafe { GetDlgItem(hwnd, (ID_CORNER_BASE + slot as u16 * 10 + 1) as i32) }.unwrap_or_default();
                if edit != HWND::default() {
                    unsafe {
                        let t = wide(&path);
                        let _ = SetWindowTextW(edit, PCWSTR(t.as_ptr())); // fires EN_CHANGE, which saves it
                    }
                }
            }
        }
        _ => {}
    }
}

fn post_rebuild(hwnd: HWND) {
    unsafe {
        let _ = PostMessageW(hwnd, WM_REBUILD, WPARAM(0), LPARAM(0));
    }
}

fn browse_for_app(owner: HWND) -> Option<String> {
    let mut file = [0u16; 1024];
    let filter: Vec<u16> = "Programs\0*.exe;*.lnk\0All files\0*.*\0\0".encode_utf16().collect();
    let title = wide("Choose an app");
    let mut ofn = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: PWSTR(file.as_mut_ptr()),
        nMaxFile: file.len() as u32,
        lpstrTitle: PCWSTR(title.as_ptr()),
        Flags: OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST,
        ..Default::default()
    };
    if unsafe { GetOpenFileNameW(&mut ofn) }.as_bool() {
        let len = file.iter().position(|&c| c == 0).unwrap_or(file.len());
        Some(String::from_utf16_lossy(&file[..len]))
    } else {
        None
    }
}

// ---------------------------------------------------------------- shortcut recording

fn begin_recording(button_id: u16) {
    let Some((hwnd, box_hwnd)) = ui(|u| (u.hwnd, u.record_pairs.iter().find(|p| p.0 == button_id).map(|p| p.1))) else { return };
    let Some(box_hwnd) = box_hwnd else { return };
    let button = unsafe { GetDlgItem(hwnd, button_id as i32) }.unwrap_or_default();
    RECORDING.with(|r| r.set(box_hwnd.0 as isize));
    RECORD_BUTTON.with(|r| r.set(button.0 as isize));
    set_button_text(button, "Press keys\u{2026}");
    unsafe {
        let _ = SetFocus(box_hwnd);
    }
}

fn set_button_text(button: HWND, text: &str) {
    if button != HWND::default() {
        unsafe {
            let t = wide(text);
            let _ = SetWindowTextW(button, PCWSTR(t.as_ptr()));
        }
    }
}

fn end_recording() {
    if RECORDING.with(|r| r.replace(0)) != 0 {
        let button = HWND(RECORD_BUTTON.with(|r| r.replace(0)) as *mut c_void);
        set_button_text(button, "Record");
    }
}

/// Subclass of the shortcut edit boxes: while recording, the next key combination becomes the text.
unsafe extern "system" fn shortcut_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _data: usize) -> LRESULT {
    let recording = RECORDING.with(|r| r.get()) == hwnd.0 as isize;
    if recording {
        match msg {
            WM_GETDLGCODE => return LRESULT(DLGC_WANTALLKEYS),
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                let vk = wparam.0 as u32;
                let modifier = [VK_CONTROL.0, VK_SHIFT.0, VK_MENU.0, VK_LWIN.0, VK_RWIN.0, 0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5]
                    .contains(&(vk as u16));
                if !modifier {
                    if let Some(name) = keys::key_name(vk) {
                        let down = |k: u16| (GetKeyState(k as i32) as u16 & 0x8000) != 0;
                        let mut parts: Vec<String> = Vec::new();
                        if down(VK_CONTROL.0) {
                            parts.push("Ctrl".into());
                        }
                        if down(VK_MENU.0) {
                            parts.push("Alt".into());
                        }
                        if down(VK_SHIFT.0) {
                            parts.push("Shift".into());
                        }
                        if down(VK_LWIN.0) || down(VK_RWIN.0) {
                            parts.push("Win".into());
                        }
                        parts.push(name);
                        end_recording();
                        let t = wide(&parts.join("+"));
                        let _ = SetWindowTextW(hwnd, PCWSTR(t.as_ptr())); // EN_CHANGE saves it
                    }
                }
                return LRESULT(0);
            }
            WM_CHAR | WM_SYSCHAR | WM_KEYUP | WM_SYSKEYUP | WM_DEADCHAR | WM_SYSDEADCHAR => return LRESULT(0),
            _ => {}
        }
    }
    if msg == WM_KILLFOCUS && recording {
        end_recording();
    }
    if msg == WM_NCDESTROY {
        let _ = RemoveWindowSubclass(hwnd, Some(shortcut_proc), SHORTCUT_SUBCLASS_ID);
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
}

// ---------------------------------------------------------------- window procedure

fn trackbar_pos(h: HWND) -> i32 {
    unsafe { SendMessageW(h, TBM_GETPOS, WPARAM(0), LPARAM(0)) }.0 as i32
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_SIZE => {
            if wparam.0 != SIZE_MINIMIZED as usize {
                layout(hwnd);
                build_detail(pad_view::selected());
            }
            return LRESULT(0);
        }
        WM_GETMINMAXINFO => {
            let dpi = GetDpiForWindow(hwnd).max(96);
            let info = &mut *(lparam.0 as *mut MINMAXINFO);
            info.ptMinTrackSize.x = (800.0 * dpi as f32 / 96.0) as i32;
            info.ptMinTrackSize.y = (670.0 * dpi as f32 / 96.0) as i32;
            return LRESULT(0);
        }
        WM_DPICHANGED_ => {
            let new_dpi = (wparam.0 & 0xFFFF) as u32;
            let suggested = &*(lparam.0 as *const RECT);
            let _ = SetWindowPos(
                hwnd,
                None,
                suggested.left,
                suggested.top,
                suggested.right - suggested.left,
                suggested.bottom - suggested.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            rebuild_for_dpi(hwnd, new_dpi);
            return LRESULT(0);
        }
        WM_DRAWITEM => {
            let dis = &*(lparam.0 as *const DRAWITEMSTRUCT);
            if dis.CtlID == ID_ENABLED as u32 {
                draw_switch(dis);
                return LRESULT(1);
            }
        }
        WM_PAD_CLICKED => {
            let section = Section::ALL[wparam.0.min(Section::ALL.len() - 1)];
            pad_view::select(section); // already done for a click on the map; keeps both in step otherwise
            refresh_pad();
            build_detail(section);
            return LRESULT(0);
        }
        WM_REBUILD => {
            build_detail(pad_view::selected());
            return LRESULT(0);
        }
        WM_COMMAND => {
            on_command(hwnd, wparam, lparam);
            return LRESULT(0);
        }
        WM_HSCROLL => {
            let from = HWND(lparam.0 as *mut c_void);
            let (edge, corner) = ui(|u| (u.edge_trk, u.corner_trk)).unwrap_or_default();
            if from == edge {
                let v = trackbar_pos(edge) as f64 / 100.0;
                app::edit_settings(|s| s.edge_depth = v);
                changed();
            } else if from == corner {
                let v = trackbar_pos(corner) as f64 / 100.0;
                app::edit_settings(|s| s.corner_size = v);
                changed();
                build_detail(pad_view::selected());
            }
            return LRESULT(0);
        }
        WM_CTLCOLORSTATIC => {
            let target = HWND(lparam.0 as *mut c_void);
            let gray = ui(|u| u.gray.contains(&(target.0 as isize))).unwrap_or(false);
            if gray {
                let hdc = HDC(wparam.0 as *mut c_void);
                SetTextColor(hdc, COLORREF(GetSysColor(COLOR_GRAYTEXT)));
                SetBkMode(hdc, TRANSPARENT);
                return LRESULT(GetSysColorBrush(COLOR_3DFACE).0 as isize);
            }
        }
        WM_CTLCOLOREDIT => {
            let target = HWND(lparam.0 as *mut c_void);
            if let Some((true, brush)) = ui(|u| (u.bad_boxes.contains(&(target.0 as isize)), u.bad_brush)) {
                let hdc = HDC(wparam.0 as *mut c_void);
                SetBkColor(hdc, COLORREF(255 | 228 << 8 | 228 << 16));
                return LRESULT(brush.0 as isize);
            }
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            return LRESULT(0);
        }
        WM_DESTROY => {
            end_recording();
            if let Some(Some(u)) = UI.with(|c| c.try_borrow_mut().ok().map(|mut g| g.take())) {
                let _ = DeleteObject(u.font);
                let _ = DeleteObject(u.bold);
                let _ = DeleteObject(u.big);
                let _ = DeleteObject(u.bad_brush);
                let _ = DestroyIcon(u.icon_big);
                let _ = DestroyIcon(u.icon_small);
            }
            return LRESULT(0);
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

/// The window moved to a monitor with a different DPI: new fonts, then every control is rebuilt.
fn rebuild_for_dpi(hwnd: HWND, dpi: u32) {
    let (font, bold, big) = make_fonts(dpi);
    let old = ui(|u| {
        let old = (u.font, u.bold, u.big);
        u.font = font;
        u.bold = bold;
        u.big = big;
        u.dpi = dpi;
        old
    });
    destroy_detail();
    let fixed: Vec<HWND> = ui(|u| {
        let mut all = std::mem::take(&mut u.fixed);
        all.extend([u.enabled, u.activation, u.speed, u.startup, u.pad, u.group, u.edge_trk, u.corner_trk, u.tip]);
        all.extend([u.profile, u.add_app, u.remove_app]);
        all
    })
    .unwrap_or_default();
    unsafe {
        for h in fixed {
            let _ = DestroyWindow(h);
        }
    }
    create_controls(hwnd);
    layout(hwnd);
    build_detail(pad_view::selected());
    if let Some((a, b, c)) = old {
        unsafe {
            let _ = DeleteObject(a);
            let _ = DeleteObject(b);
            let _ = DeleteObject(c);
        }
    }
}

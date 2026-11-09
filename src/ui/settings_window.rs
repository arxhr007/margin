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
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, SetFocus, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT};
use windows::Win32::UI::WindowsAndMessaging::*;

use super::pad_view::{self, WM_PAD_CLICKED};
use crate::app;
use crate::keys;
use crate::labels;
use crate::settings::{Activation, ClickAction, ClickSpec, Section, Settings, SlideAction, Speed};
use crate::startup;

// control ids
const ID_ENABLED: u16 = 100;
const ID_ACTIVATION: u16 = 101;
const ID_SPEED: u16 = 102;
const ID_STARTUP: u16 = 103;
const ID_SLIDE: u16 = 200;
const ID_PRESET: u16 = 201;
const ID_FWD_BOX: u16 = 202;
const ID_FWD_REC: u16 = 203;
const ID_BACK_BOX: u16 = 204;
const ID_BACK_REC: u16 = 205;
const ID_HOLD: u16 = 206;
const ID_CUSTOMIZE: u16 = 207;
const ID_CORNER_BASE: u16 = 300; // + slot * 10: +0 combo, +1 value, +2 browse

const WM_REBUILD: u32 = WM_APP + 20;

// raw control constants (kept numeric to stay independent of the generated wrapper types)
const BS_PUSHBUTTON: u32 = 0;
const BS_AUTOCHECKBOX: u32 = 3;
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
    icon_big: HICON,
    icon_small: HICON,
    /// static parts of the window
    fixed: Vec<HWND>,
    enabled: HWND,
    activation: HWND,
    speed: HWND,
    startup: HWND,
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
        SendMessageW(activation, CB_SETCURSEL, WPARAM(if st.activation == Activation::SlideIn { 1 } else { 0 }), LPARAM(0));
        SendMessageW(speed, CB_SETCURSEL, WPARAM(st.speed as usize), LPARAM(0));
        SendMessageW(enabled, BM_SETCHECK, WPARAM(st.enabled as usize), LPARAM(0));
        SendMessageW(edge_trk, TBM_SETPOS, WPARAM(1), LPARAM(pct(st.edge_depth, 5, 25) as isize));
        SendMessageW(corner_trk, TBM_SETPOS, WPARAM(1), LPARAM(pct(st.corner_size, 8, 30) as isize));
        let _ = InvalidateRect(pad, None, false);
    }
    LOADING.with(|l| l.set(false));
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

        let (font, bold) = make_fonts(dpi);
        UI.with(|c| {
            *c.borrow_mut() = Some(Ui {
                hwnd,
                dpi,
                font,
                bold,
                icon_big,
                icon_small,
                fixed: Vec::new(),
                enabled: HWND::default(),
                activation: HWND::default(),
                speed: HWND::default(),
                startup: HWND::default(),
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
fn make_fonts(dpi: u32) -> (HFONT, HFONT) {
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
        (normal, bold)
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

    let enabled = make_child(hwnd, w!("BUTTON"), "Margin is on", BS_AUTOCHECKBOX | tab, 0, ID_ENABLED, font);
    let activation = make_child(hwnd, w!("COMBOBOX"), "", CBS_DROPDOWNLIST | WS_VSCROLL.0 | tab, 0, ID_ACTIVATION, font);
    let speed = make_child(hwnd, w!("COMBOBOX"), "", CBS_DROPDOWNLIST | WS_VSCROLL.0 | tab, 0, ID_SPEED, font);
    let startup = make_child(hwnd, w!("BUTTON"), "Start with Windows", BS_AUTOCHECKBOX | tab, 0, ID_STARTUP, font);
    let cap_activation = make_child(hwnd, w!("STATIC"), "Start a slide by:", 0, 0, 0, font);
    let cap_speed = make_child(hwnd, w!("STATIC"), "Speed:", 0, 0, 0, font);

    LOADING.with(|l| l.set(true));
    unsafe {
        for text in ["Press the edge until it clicks, then slide", "Slide in from the edge (no click)"] {
            SendMessageW(activation, CB_ADDSTRING, WPARAM(0), LPARAM(wide(text).as_ptr() as isize));
        }
        SendMessageW(activation, CB_SETCURSEL, WPARAM(if st.activation == Activation::SlideIn { 1 } else { 0 }), LPARAM(0));
        for text in ["Slow", "Medium", "Fast"] {
            SendMessageW(speed, CB_ADDSTRING, WPARAM(0), LPARAM(wide(text).as_ptr() as isize));
        }
        SendMessageW(speed, CB_SETCURSEL, WPARAM(st.speed as usize), LPARAM(0));
        SendMessageW(enabled, BM_SETCHECK, WPARAM(st.enabled as usize), LPARAM(0));
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
        "Click a section on the touchpad to change it. Orange dots show your fingers. Hold Ctrl + Shift while sliding for fine steps.",
        SS_NOPREFIX,
        0,
        0,
        font,
    );

    ui(|u| {
        u.fixed = vec![cap_activation, cap_speed, edge_cap, corner_cap];
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
}

fn move_to(h: HWND, x: i32, y: i32, w: i32, hgt: i32) {
    unsafe {
        let _ = MoveWindow(h, x, y, w, hgt, true);
    }
}

/// Positions every fixed control for the current client size and DPI.
fn layout(hwnd: HWND) {
    let Some((u_dpi, fixed, enabled, activation, speed, startup, pad, group, edge_trk, corner_trk, tip)) = ui(|u| {
        (u.dpi, u.fixed.clone(), u.enabled, u.activation, u.speed, u.startup, u.pad, u.group, u.edge_trk, u.corner_trk, u.tip)
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

    // header, two rows
    let row_h = px(24.0, s);
    let (r1, r2) = (m, m + row_h + px(8.0, s));
    move_to(enabled, m, r1, px(130.0, s), row_h);
    move_to(fixed[0], m + px(150.0, s), r1 + px(4.0, s), px(100.0, s), px(20.0, s));
    move_to(activation, m + px(255.0, s), r1, px(300.0, s), px(220.0, s));
    move_to(fixed[1], m, r2 + px(4.0, s), px(50.0, s), px(20.0, s));
    move_to(speed, m + px(55.0, s), r2, px(100.0, s), px(160.0, s));
    move_to(startup, m + px(180.0, s), r2, px(180.0, s), row_h);

    // footer
    let tip_h = px(20.0, s);
    let trk_h = px(32.0, s);
    let tip_y = ch - m - tip_h;
    let trk_y = tip_y - trk_h - px(4.0, s);
    move_to(tip, m, tip_y, cw - 2 * m, tip_h);
    move_to(fixed[2], m, trk_y + px(8.0, s), px(70.0, s), px(20.0, s));
    move_to(edge_trk, m + px(72.0, s), trk_y, px(170.0, s), trk_h);
    move_to(fixed[3], m + px(270.0, s), trk_y + px(8.0, s), px(80.0, s), px(20.0, s));
    move_to(corner_trk, m + px(352.0, s), trk_y, px(170.0, s), trk_h);

    // middle: pad on the left, editor group on the right
    let top = r2 + row_h + px(12.0, s);
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
    let st = app::read_settings(|s| s.clone()).unwrap_or_default();
    if section.is_corner() {
        build_corner(&mut c, section, &st);
    } else {
        build_edge(&mut c, section, &st);
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

    let direction = if section.is_vertical() {
        "Slide up for more / next, down for less / previous."
    } else {
        "Slide right for forward / next, left for back / previous."
    };
    let how = if st.activation == Activation::Press {
        "Rest a finger on the edge, press until the touchpad clicks, and slide without letting go."
    } else {
        "Bring your finger in from the laptop body onto this edge, then slide along it. No click needed."
    };
    c.para(direction, false, false, 6.0);
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
    let clicks = st.section(section).clicks.clone();
    c.para("Press this corner until the touchpad clicks:", true, false, 2.0);
    let names: Vec<&str> = ClickAction::ALL.iter().map(|&a| labels::click_long(a)).collect();
    for i in 0..3usize {
        let spec = clicks.get(i).cloned().unwrap_or_default();
        c.gap(6.0);
        c.para(&if i == 0 { "1 click".to_string() } else { format!("{} clicks", i + 1) }, false, false, 2.0);
        let base = ID_CORNER_BASE + (i as u16) * 10;
        c.combo(base, &names, spec.action as usize, 4.0);
        if labels::needs_value(spec.action) {
            let browse = (spec.action == ClickAction::App).then_some((base + 2, "Browse\u{2026}"));
            let (edit, _) = c.edit_row(base + 1, spec.value.as_deref().unwrap_or(""), browse, spec.action == ClickAction::Keys, 2.0);
            let hint = wide(labels::placeholder(spec.action));
            unsafe {
                SendMessageW(edit, EM_SETCUEBANNER, WPARAM(1), LPARAM(hint.as_ptr() as isize));
            }
            mark_validity(edit);
        }
    }
    c.gap(10.0);
    c.para("Corners react only to a real press, never to a tap. Unused corners stay normal touchpad.", false, true, 0.0);
}

// ---------------------------------------------------------------- change handling

fn changed() {
    app::settings_changed();
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
        ID_ENABLED if code == 0 => {
            let on = is_checked(control);
            app::edit_settings(|s| s.enabled = on);
            changed();
        }
        ID_ACTIVATION if code == CBN_SELCHANGE => {
            let slide_in = combo_index(control) == 1;
            app::edit_settings(|s| s.activation = if slide_in { Activation::SlideIn } else { Activation::Press });
            changed();
            build_detail(section); // refresh the how-to text
        }
        ID_SPEED if code == CBN_SELCHANGE => {
            let speed = [Speed::Slow, Speed::Medium, Speed::Fast][combo_index(control).min(2)];
            app::edit_settings(|s| s.speed = speed);
            changed();
        }
        ID_STARTUP if code == 0 => startup::set_enabled(is_checked(control)),
        ID_SLIDE if code == CBN_SELCHANGE => {
            let action = SlideAction::ALL[combo_index(control).min(SlideAction::ALL.len() - 1)];
            app::edit_settings(|s| {
                let v = s.section_mut(section);
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
                app::edit_settings(|s| {
                    let v = s.section_mut(section);
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
            app::edit_settings(|s| {
                let v = s.section_mut(section);
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
            app::edit_settings(|s| {
                let v = s.section_mut(section);
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
            app::edit_settings(|s| s.section_mut(section).hold_modifiers = on);
            changed();
        }
        _ if (ID_CORNER_BASE..ID_CORNER_BASE + 30).contains(&id) => on_corner_command(hwnd, id, code, control, section),
        _ => {}
    }
}

fn on_corner_command(hwnd: HWND, id: u16, code: u32, control: HWND, section: Section) {
    let slot = ((id - ID_CORNER_BASE) / 10) as usize;
    let part = (id - ID_CORNER_BASE) % 10;
    match part {
        0 if code == CBN_SELCHANGE => {
            let action = ClickAction::ALL[combo_index(control).min(ClickAction::ALL.len() - 1)];
            app::edit_settings(|s| {
                let clicks = &mut s.section_mut(section).clicks;
                while clicks.len() <= slot {
                    clicks.push(ClickSpec::default());
                }
                clicks[slot].action = action;
                if !labels::needs_value(action) {
                    clicks[slot].value = None;
                }
            });
            changed();
            post_rebuild(hwnd);
        }
        1 if code == EN_CHANGE => {
            let text = read_text(control);
            let value = (!text.is_empty()).then_some(text);
            app::edit_settings(|s| {
                let clicks = &mut s.section_mut(section).clicks;
                while clicks.len() <= slot {
                    clicks.push(ClickSpec::default());
                }
                clicks[slot].value = value;
            });
            let is_keys = app::read_settings(|s| s.section(section).clicks.get(slot).map(|c| c.action) == Some(ClickAction::Keys)).unwrap_or(false);
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
            info.ptMinTrackSize.y = (640.0 * dpi as f32 / 96.0) as i32;
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
        WM_PAD_CLICKED => {
            build_detail(Section::ALL[wparam.0.min(Section::ALL.len() - 1)]);
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
    let (font, bold) = make_fonts(dpi);
    let old = ui(|u| {
        let old = (u.font, u.bold);
        u.font = font;
        u.bold = bold;
        u.dpi = dpi;
        old
    });
    destroy_detail();
    let fixed: Vec<HWND> = ui(|u| {
        let mut all = std::mem::take(&mut u.fixed);
        all.extend([u.enabled, u.activation, u.speed, u.startup, u.pad, u.group, u.edge_trk, u.corner_trk, u.tip]);
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
    if let Some((a, b)) = old {
        unsafe {
            let _ = DeleteObject(a);
            let _ = DeleteObject(b);
        }
    }
}

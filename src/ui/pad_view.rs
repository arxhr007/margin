//! A drawing of the touchpad with its eight sections. Click a section to select it.
//! Fingers currently on the real touchpad are shown as dots, so you can see where the zones are.

use std::cell::Cell;

use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::geometry::{self, Pt, Rect};
use crate::labels;
use crate::settings::{ClickAction, Section, Settings};
use crate::app;

pub const CLASS: windows::core::PCWSTR = w!("MarginPadView");
/// Sent to the parent when a section is clicked: wparam = index into `Section::ALL`.
pub const WM_PAD_CLICKED: u32 = WM_APP + 10;

const ACCENT: (u8, u8, u8) = (0, 103, 192);
const PAD_FILL: (u8, u8, u8) = (242, 243, 245);
const TIMER_REPAINT: usize = 1;

thread_local! {
    static SELECTED: Cell<usize> = const { Cell::new(1) }; // Section::Right
    static HOVER: Cell<Option<usize>> = const { Cell::new(None) };
    static LAST_FRAME: Cell<u64> = const { Cell::new(0) };
    static TRACKING: Cell<bool> = const { Cell::new(false) };
}

pub fn selected() -> Section {
    Section::ALL[SELECTED.with(|s| s.get())]
}

pub fn select(s: Section) {
    let index = Section::ALL.iter().position(|&x| x == s).unwrap_or(1);
    SELECTED.with(|c| c.set(index));
}

pub fn register() {
    unsafe {
        let instance = GetModuleHandleW(None).unwrap_or_default();
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            hInstance: instance.into(),
            lpszClassName: CLASS,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        };
        RegisterClassExW(&wc);
    }
}

fn blend(top: (u8, u8, u8), alpha: u32, base: (u8, u8, u8)) -> COLORREF {
    let mix = |t: u8, b: u8| ((t as u32 * alpha + b as u32 * (255 - alpha)) / 255) as u8;
    rgb(mix(top.0, base.0), mix(top.1, base.1), mix(top.2, base.2))
}

fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16)
}

/// The touchpad outline: a 1.6:1 rounded rectangle centered in the control.
fn pad_rect(width: i32, height: i32, scale: f32) -> (f32, f32, f32, f32) {
    let margin = 12.0 * scale;
    let mut w = width as f32 - 2.0 * margin;
    let mut h = height as f32 - 2.0 * margin;
    if w / h > 1.6 {
        w = h * 1.6;
    } else {
        h = w / 1.6;
    }
    ((width as f32 - w) / 2.0, (height as f32 - h) / 2.0, w, h)
}

fn to_screen(n: Rect, pad: (f32, f32, f32, f32)) -> RECT {
    let (px, py, pw, ph) = pad;
    RECT {
        left: (px + n.x * pw).round() as i32,
        top: (py + (1.0 - (n.y + n.h)) * ph).round() as i32,
        right: (px + (n.x + n.w) * pw).round() as i32,
        bottom: (py + (1.0 - n.y) * ph).round() as i32,
    }
}

fn hit_test(hwnd: HWND, x: i32, y: i32) -> Option<usize> {
    let mut client = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut client);
    }
    let scale = unsafe { GetDpiForWindow(hwnd) } as f32 / 96.0;
    let (px, py, pw, ph) = pad_rect(client.right, client.bottom, scale);
    let (fx, fy) = (x as f32, y as f32);
    if fx < px || fx > px + pw || fy < py || fy > py + ph {
        return None;
    }
    let n = Pt::new((fx - px) / pw, 1.0 - (fy - py) / ph);
    let section = app::read_settings(|st| geometry::hit(n, st)).flatten()?;
    Section::ALL.iter().position(|&s| s == section)
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            SetTimer(hwnd, TIMER_REPAINT, 33, None);
            LRESULT(0)
        }
        WM_DESTROY => {
            let _ = KillTimer(hwnd, TIMER_REPAINT);
            HOVER.with(|h| h.set(None));
            LRESULT(0)
        }
        WM_TIMER => {
            let frames = app::with(|s| s.frames).unwrap_or(0);
            if LAST_FRAME.with(|l| l.replace(frames)) != frames {
                let _ = InvalidateRect(hwnd, None, false);
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_MOUSEMOVE => {
            let (x, y) = ((lparam.0 & 0xFFFF) as i16 as i32, ((lparam.0 >> 16) & 0xFFFF) as i16 as i32);
            if !TRACKING.with(|t| t.get()) {
                let mut tme = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                let _ = TrackMouseEvent(&mut tme);
                TRACKING.with(|t| t.set(true));
            }
            let h = hit_test(hwnd, x, y);
            if HOVER.with(|c| c.replace(h)) != h {
                let _ = InvalidateRect(hwnd, None, false);
            }
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            TRACKING.with(|t| t.set(false));
            HOVER.with(|h| h.set(None));
            let _ = InvalidateRect(hwnd, None, false);
            LRESULT(0)
        }
        WM_SETCURSOR => {
            let mut at = POINT::default();
            let _ = GetCursorPos(&mut at);
            let _ = ScreenToClient(hwnd, &mut at);
            let id = if hit_test(hwnd, at.x, at.y).is_some() { IDC_HAND } else { IDC_ARROW };
            SetCursor(LoadCursorW(None, id).unwrap_or_default());
            LRESULT(1)
        }
        WM_LBUTTONUP => {
            let (x, y) = ((lparam.0 & 0xFFFF) as i16 as i32, ((lparam.0 >> 16) & 0xFFFF) as i16 as i32);
            if let Some(i) = hit_test(hwnd, x, y) {
                SELECTED.with(|s| s.set(i));
                let _ = InvalidateRect(hwnd, None, false);
                if let Ok(parent) = GetParent(hwnd) {
                    let _ = PostMessageW(parent, WM_PAD_CLICKED, WPARAM(i), LPARAM(0));
                }
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            paint_buffered(hwnd, hdc);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn paint_buffered(hwnd: HWND, target: HDC) {
    let mut client = RECT::default();
    let _ = GetClientRect(hwnd, &mut client);
    let (w, h) = (client.right.max(1), client.bottom.max(1));
    let mem = CreateCompatibleDC(target);
    let bitmap = CreateCompatibleBitmap(target, w, h);
    let old = SelectObject(mem, bitmap);

    paint(hwnd, mem, client);

    let _ = BitBlt(target, 0, 0, w, h, mem, 0, 0, SRCCOPY);
    SelectObject(mem, old);
    let _ = DeleteObject(bitmap);
    let _ = DeleteDC(mem);
}

fn summary(s: Section, st: &Settings) -> String {
    let v = st.section(s);
    if !s.is_corner() {
        return labels::slide_short(v.slide).to_string();
    }
    // Corners are small: show the single-click action, plus how many more are set.
    let used: Vec<ClickAction> = v.clicks.iter().map(|c| c.action).filter(|a| *a != ClickAction::None).collect();
    match used.len() {
        0 => String::new(),
        1 => labels::click_short(used[0]).to_string(),
        n => format!("{} +{}", labels::click_short(used[0]), n - 1),
    }
}

unsafe fn make_font(scale: f32, points: f32, bold: bool, escapement: i32) -> HFONT {
    CreateFontW(
        -((points * 96.0 / 72.0 * scale).round() as i32),
        0,
        escapement,
        escapement,
        if bold { FW_BOLD.0 as i32 } else { FW_NORMAL.0 as i32 },
        0,
        0,
        0,
        DEFAULT_CHARSET.0 as u32,
        OUT_DEFAULT_PRECIS.0 as u32,
        CLIP_DEFAULT_PRECIS.0 as u32,
        CLEARTYPE_QUALITY.0 as u32,
        DEFAULT_PITCH.0 as u32,
        w!("Segoe UI"),
    )
}

unsafe fn paint(hwnd: HWND, hdc: HDC, client: RECT) {
    let scale = GetDpiForWindow(hwnd) as f32 / 96.0;
    FillRect(hdc, &client, GetSysColorBrush(COLOR_3DFACE));
    let pad = pad_rect(client.right, client.bottom, scale);
    let selected = SELECTED.with(|s| s.get());
    let hover = HOVER.with(|h| h.get());

    // outline of the pad
    let pad_rc = RECT {
        left: pad.0.round() as i32,
        top: pad.1.round() as i32,
        right: (pad.0 + pad.2).round() as i32,
        bottom: (pad.1 + pad.3).round() as i32,
    };
    {
        let fill = CreateSolidBrush(rgb(PAD_FILL.0, PAD_FILL.1, PAD_FILL.2));
        let border = CreatePen(PS_SOLID, ((1.5 * scale).round() as i32).max(1), rgb(160, 160, 165));
        let (old_b, old_p) = (SelectObject(hdc, fill), SelectObject(hdc, border));
        let r = (28.0 * scale) as i32;
        let _ = RoundRect(hdc, pad_rc.left, pad_rc.top, pad_rc.right, pad_rc.bottom, r, r);
        SelectObject(hdc, old_b);
        SelectObject(hdc, old_p);
        let _ = DeleteObject(fill);
        let _ = DeleteObject(border);
    }

    let (normal, bold, small) = (make_font(scale, 8.5, false, 0), make_font(scale, 9.0, true, 0), make_font(scale, 7.0, false, 0));
    let font_left = make_font(scale, 8.5, false, 900);
    let font_right = make_font(scale, 8.5, false, 2700);
    let font_left_bold = make_font(scale, 9.0, true, 900);
    let font_right_bold = make_font(scale, 9.0, true, 2700);
    SetBkMode(hdc, TRANSPARENT);

    let st = app::read_settings(|s| s.clone()).unwrap_or_default();
    for &s in geometry::ALL.iter() {
        let index = Section::ALL.iter().position(|&x| x == s).unwrap_or(0);
        let mut r = to_screen(geometry::bounds(s, &st), pad);
        let inset = (2.0 * scale).round() as i32;
        r.left += inset;
        r.top += inset;
        r.right -= inset;
        r.bottom -= inset;
        if r.right <= r.left || r.bottom <= r.top {
            continue;
        }
        let assigned = geometry::is_assigned(s, &st);
        let is_selected = index == selected;
        let alpha = if is_selected { 120 } else if hover == Some(index) { 80 } else if assigned { 55 } else { 0 };

        let fill = if alpha > 0 { CreateSolidBrush(blend(ACCENT, alpha, PAD_FILL)) } else { HBRUSH(GetStockObject(NULL_BRUSH).0) };
        let pen = if is_selected {
            CreatePen(PS_SOLID, ((2.5 * scale).round() as i32).max(1), rgb(ACCENT.0, ACCENT.1, ACCENT.2))
        } else {
            let edge = blend((120, 120, 128), if assigned { 140 } else { 90 }, PAD_FILL);
            CreatePen(if assigned { PS_SOLID } else { PS_DASH }, 1, edge)
        };
        let (old_b, old_p) = (SelectObject(hdc, fill), SelectObject(hdc, pen));
        let radius = (12.0 * scale) as i32;
        let _ = RoundRect(hdc, r.left, r.top, r.right, r.bottom, radius, radius);
        SelectObject(hdc, old_b);
        SelectObject(hdc, old_p);
        if alpha > 0 {
            let _ = DeleteObject(fill);
        }
        let _ = DeleteObject(pen);

        let text = summary(s, &st);
        if text.is_empty() {
            continue;
        }
        SetTextColor(hdc, if is_selected { rgb(0, 50, 110) } else { rgb(40, 40, 45) });
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        if s.is_vertical() {
            // write sideways along tall, narrow edges
            let font = match (s == Section::Left, is_selected) {
                (true, true) => font_left_bold,
                (true, false) => font_left,
                (false, true) => font_right_bold,
                (false, false) => font_right,
            };
            let old = SelectObject(hdc, font);
            let mut size = windows::Win32::Foundation::SIZE::default();
            let _ = GetTextExtentPoint32W(hdc, &wide, &mut size);
            let (cx, cy) = ((r.left + r.right) / 2, (r.top + r.bottom) / 2);
            let (x0, y0) = if s == Section::Left {
                (cx - size.cy / 2, cy + size.cx / 2)
            } else {
                (cx + size.cy / 2, cy - size.cx / 2)
            };
            let _ = TextOutW(hdc, x0, y0, &wide);
            SelectObject(hdc, old);
        } else {
            let font = if s.is_corner() { small } else if is_selected { bold } else { normal };
            let old = SelectObject(hdc, font);
            let mut rc = r;
            DrawTextW(hdc, &mut wide, &mut rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX | DT_END_ELLIPSIS);
            SelectObject(hdc, old);
        }
    }

    // hint in the middle
    {
        let mut rc = to_screen(Rect { x: 0.3, y: 0.4, w: 0.4, h: 0.2 }, pad);
        SetTextColor(hdc, rgb(130, 130, 136));
        let old = SelectObject(hdc, normal);
        let mut wide: Vec<u16> = "Normal touchpad".encode_utf16().collect();
        DrawTextW(hdc, &mut wide, &mut rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
        SelectObject(hdc, old);
    }

    // fingers on the real touchpad
    let fingers: Vec<Pt> = app::with(|s| s.tracker.snapshot().iter().map(|t| t.current).collect()).unwrap_or_default();
    if !fingers.is_empty() {
        let dot = CreateSolidBrush(rgb(255, 140, 0));
        let none = GetStockObject(NULL_PEN);
        let (old_b, old_p) = (SelectObject(hdc, dot), SelectObject(hdc, none));
        let rad = (9.0 * scale).round() as i32;
        for p in fingers {
            let x = (pad.0 + p.x * pad.2).round() as i32;
            let y = (pad.1 + (1.0 - p.y) * pad.3).round() as i32;
            let _ = Ellipse(hdc, x - rad, y - rad, x + rad, y + rad);
        }
        SelectObject(hdc, old_b);
        SelectObject(hdc, old_p);
        let _ = DeleteObject(dot);
    }

    for f in [normal, bold, small, font_left, font_right, font_left_bold, font_right_bold] {
        let _ = DeleteObject(f);
    }
}

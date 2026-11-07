//! Small floating label for scrubbing and brightness. Volume uses the native Windows flyout.
//! Never takes focus and lets clicks pass through.

use std::cell::RefCell;

use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::*;

const TIMER_HIDE: usize = 1;

struct HudState {
    hwnd: HWND,
    text: String,
    /// 0..100 draws a bar, -1 = text only
    level: i32,
    scale: f32,
}

thread_local! {
    static HUD: RefCell<Option<HudState>> = const { RefCell::new(None) };
}

pub fn create() {
    unsafe {
        let instance = GetModuleHandleW(None).unwrap_or_default();
        let class = w!("MarginHud");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wnd_proc),
            hInstance: instance.into(),
            lpszClassName: class,
            ..Default::default()
        };
        RegisterClassExW(&wc);
        let ex = WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED;
        match CreateWindowExW(ex, class, w!("Margin HUD"), WS_POPUP, 0, 0, 10, 10, None, None, instance, None) {
            Ok(hwnd) => {
                let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 240, LWA_ALPHA); // 94 % opaque
                HUD.with(|h| *h.borrow_mut() = Some(HudState { hwnd, text: String::new(), level: -1, scale: 1.0 }));
            }
            Err(e) => crate::log::error(format!("could not create the HUD window: {e}")),
        }
    }
}

pub fn destroy() {
    HUD.with(|h| {
        if let Some(state) = h.borrow_mut().take() {
            unsafe {
                let _ = DestroyWindow(state.hwnd);
            }
        }
    });
}

pub fn show_text(label: &str) {
    present(label, -1);
}

pub fn show_level(label: &str, percent: i32) {
    present(label, percent.clamp(0, 100));
}

fn present(label: &str, percent: i32) {
    // Work out the geometry first, without holding the state borrow across window calls.
    let Some(hwnd) = HUD.with(|h| h.borrow().as_ref().map(|s| s.hwnd)) else { return };
    unsafe {
        let _ = KillTimer(hwnd, TIMER_HIDE);

        let mut cursor = POINT::default();
        let _ = GetCursorPos(&mut cursor);
        let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
        let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
        let scale = dpi_x as f32 / 96.0;
        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(monitor, &mut info);
        let area = info.rcWork;

        let width = (220.0 * scale) as i32;
        let height = ((if percent >= 0 { 70.0 } else { 56.0 }) * scale) as i32;
        let x = area.left + (area.right - area.left - width) / 2;
        let y = area.bottom - height - (90.0 * scale) as i32;

        HUD.with(|h| {
            if let Some(s) = h.borrow_mut().as_mut() {
                s.text = label.to_string();
                s.level = percent;
                s.scale = scale;
            }
        });

        let radius = (16.0 * scale) as i32;
        let region = CreateRoundRectRgn(0, 0, width + 1, height + 1, radius * 2, radius * 2);
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_NOACTIVATE | SWP_SHOWWINDOW);
        SetWindowRgn(hwnd, region, true); // the window owns the region from here on
        let _ = InvalidateRect(hwnd, None, true);
        let _ = UpdateWindow(hwnd);
    }
}

pub fn hide_after(ms: u32) {
    let Some(hwnd) = HUD.with(|h| h.borrow().as_ref().map(|s| s.hwnd)) else { return };
    unsafe {
        let _ = KillTimer(hwnd, TIMER_HIDE);
        if ms == 0 {
            let _ = ShowWindow(hwnd, SW_HIDE);
        } else {
            SetTimer(hwnd, TIMER_HIDE, ms, None);
        }
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == TIMER_HIDE => {
            let _ = KillTimer(hwnd, TIMER_HIDE);
            let _ = ShowWindow(hwnd, SW_HIDE);
            LRESULT(0)
        }
        WM_NCHITTEST => LRESULT(-1), // HTTRANSPARENT
        WM_MOUSEACTIVATE => LRESULT(3), // MA_NOACTIVATE
        WM_ERASEBKGND => LRESULT(1),
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16)
}

unsafe fn paint(hwnd: HWND) {
    let (text, level, scale) = HUD.with(|h| {
        h.borrow()
            .as_ref()
            .map(|s| (s.text.clone(), s.level, s.scale))
            .unwrap_or_default()
    });
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut client = RECT::default();
    let _ = GetClientRect(hwnd, &mut client);

    let background = CreateSolidBrush(rgb(30, 30, 32));
    FillRect(hdc, &client, background);
    let _ = DeleteObject(background);

    let font_px = (15.0 * 96.0 / 72.0 * scale) as i32;
    let font = CreateFontW(
        -font_px,
        0,
        0,
        0,
        FW_SEMIBOLD.0 as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET.0 as u32,
        OUT_DEFAULT_PRECIS.0 as u32,
        CLIP_DEFAULT_PRECIS.0 as u32,
        CLEARTYPE_QUALITY.0 as u32,
        DEFAULT_PITCH.0 as u32,
        w!("Segoe UI Semibold"),
    );
    let old_font = SelectObject(hdc, font);
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, rgb(255, 255, 255));

    let mut wide: Vec<u16> = text.encode_utf16().collect();
    let mut area = if level >= 0 {
        RECT { left: 0, top: (6.0 * scale) as i32, right: client.right, bottom: (40.0 * scale) as i32 + (6.0 * scale) as i32 }
    } else {
        client
    };
    if !wide.is_empty() {
        DrawTextW(hdc, &mut wide, &mut area, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
    }
    SelectObject(hdc, old_font);
    let _ = DeleteObject(font);

    if level >= 0 {
        let track = RECT {
            left: (20.0 * scale) as i32,
            top: (46.0 * scale) as i32,
            right: client.right - (20.0 * scale) as i32,
            bottom: (54.0 * scale) as i32,
        };
        let pen = GetStockObject(NULL_PEN);
        let old_pen = SelectObject(hdc, pen);
        let radius = (8.0 * scale) as i32;

        let track_brush = CreateSolidBrush(rgb(92, 92, 93));
        let old_brush = SelectObject(hdc, track_brush);
        let _ = RoundRect(hdc, track.left, track.top, track.right + 1, track.bottom + 1, radius, radius);

        if level > 0 {
            let full = (track.right - track.left) as f32;
            let width = (full * level as f32 / 100.0).max((track.bottom - track.top) as f32) as i32;
            let fill_brush = CreateSolidBrush(rgb(255, 255, 255));
            SelectObject(hdc, fill_brush);
            let _ = RoundRect(hdc, track.left, track.top, track.left + width + 1, track.bottom + 1, radius, radius);
            SelectObject(hdc, track_brush);
            let _ = DeleteObject(fill_brush);
        }
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        let _ = DeleteObject(track_brush);
    }
    let _ = EndPaint(hwnd, &ps);
}

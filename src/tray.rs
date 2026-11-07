//! Notification-area icon, its menu, and the touchpad icon artwork.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::HiDpi::GetDpiForSystem;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::app::{CMD_LOG, CMD_QUIT, CMD_SETTINGS, CMD_TOGGLE, WM_APP_TRAY};

const TRAY_ID: u32 = 1;

fn fill_tip(tip: &str, dest: &mut [u16; 128]) {
    let wide: Vec<u16> = tip.encode_utf16().take(127).collect();
    dest.fill(0);
    dest[..wide.len()].copy_from_slice(&wide);
}

fn data(hwnd: HWND, flags: NOTIFY_ICON_DATA_FLAGS) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ID,
        uFlags: flags,
        ..Default::default()
    }
}

pub fn add(hwnd: HWND) {
    let size = ((unsafe { GetDpiForSystem() } as i32) * 16 / 96).max(16);
    let mut nid = data(hwnd, NIF_ICON | NIF_MESSAGE | NIF_TIP);
    nid.uCallbackMessage = WM_APP_TRAY;
    nid.hIcon = make_icon(size);
    fill_tip("Margin", &mut nid.szTip);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &nid); // a stale icon after an Explorer restart
        if !Shell_NotifyIconW(NIM_ADD, &nid).as_bool() {
            crate::log::error("could not add the tray icon");
        }
    }
}

pub fn set_tip(hwnd: HWND, tip: &str) {
    let mut nid = data(hwnd, NIF_TIP);
    fill_tip(tip, &mut nid.szTip);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
    }
}

pub fn remove(hwnd: HWND) {
    let nid = data(hwnd, NOTIFY_ICON_DATA_FLAGS(0));
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
    }
}

/// The right-click menu. The chosen item arrives as WM_COMMAND.
pub fn show_menu(hwnd: HWND, enabled: bool) {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return };
        let _ = AppendMenuW(menu, MF_STRING, CMD_SETTINGS, w!("Settings\u{2026}"));
        let on_flags = if enabled { MF_STRING | MF_CHECKED } else { MF_STRING };
        let _ = AppendMenuW(menu, on_flags, CMD_TOGGLE, w!("On"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(menu, MF_STRING, CMD_LOG, w!("Open log"));
        let _ = AppendMenuW(menu, MF_STRING, CMD_QUIT, w!("Quit Margin"));
        let _ = SetMenuDefaultItem(menu, CMD_SETTINGS as u32, 0);

        let mut at = POINT::default();
        let _ = GetCursorPos(&mut at);
        let _ = SetForegroundWindow(hwnd); // required so the menu closes when clicking elsewhere
        let _ = TrackPopupMenu(menu, TPM_RIGHTBUTTON | TPM_BOTTOMALIGN, at.x, at.y, 0, hwnd, None);
        let _ = PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
    }
}

// ---------------------------------------------------------------- icon artwork

/// Builds an HICON from the artwork (32-bit with alpha).
pub fn make_icon(size: i32) -> HICON {
    let rgba = crate::icon_art::icon_pixels(size);
    unsafe {
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size,
                biHeight: -size, // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let color = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0).unwrap_or_default();
        if !bits.is_null() {
            let dst = std::slice::from_raw_parts_mut(bits as *mut u8, (size * size * 4) as usize);
            for (d, s) in dst.chunks_exact_mut(4).zip(rgba.chunks_exact(4)) {
                // BGRA, premultiplied
                let a = s[3] as u32;
                d[0] = (s[2] as u32 * a / 255) as u8;
                d[1] = (s[1] as u32 * a / 255) as u8;
                d[2] = (s[0] as u32 * a / 255) as u8;
                d[3] = s[3];
            }
        }
        let mask = CreateBitmap(size, size, 1, 1, None);
        let icon = CreateIconIndirect(&ICONINFO { fIcon: true.into(), xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color });
        let _ = DeleteObject(color);
        let _ = DeleteObject(mask);
        icon.unwrap_or_default()
    }
}

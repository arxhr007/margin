//! Low-level mouse hook: swallows edge clicks, their drags and releases, so apps never see them.
//! The callback must stay fast - Windows silently drops hooks that take too long - so all real work
//! (key presses, brightness, HUD) is queued by the engine and run after the hook has returned.

use std::cell::Cell;

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::app;

thread_local! {
    static HOOK: Cell<isize> = const { Cell::new(0) };
    static DRAINING: Cell<bool> = const { Cell::new(false) };
    /// WM_LBUTTONUP / WM_RBUTTONUP we owe a swallow, 0 if none.
    static SWALLOWED: Cell<u32> = const { Cell::new(0) };
}

const LLMHF_INJECTED: u32 = 0x01;

/// (Re)installs the hook; safe to call again after sleep or unlock.
pub fn install() {
    uninstall();
    let module = unsafe { GetModuleHandleW(None) }.unwrap_or_default();
    match unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(callback), module, 0) } {
        Ok(h) => HOOK.with(|c| c.set(h.0 as isize)),
        Err(e) => crate::log::error(format!("SetWindowsHookEx failed: {e}")),
    }
}

pub fn uninstall() {
    let h = HOOK.with(|c| c.replace(0));
    if h != 0 {
        let _ = unsafe { UnhookWindowsHookEx(HHOOK(h as *mut _)) };
    }
}

unsafe extern "system" fn callback(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && !DRAINING.with(|d| d.get()) {
        // SAFETY: for WH_MOUSE_LL, lparam points at an MSLLHOOKSTRUCT.
        let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);
        if info.flags & LLMHF_INJECTED == 0 && handle(wparam.0 as u32) {
            return LRESULT(1);
        }
    }
    CallNextHookEx(HHOOK(HOOK.with(|c| c.get()) as *mut _), code, wparam, lparam)
}

fn handle(msg: u32) -> bool {
    match msg {
        WM_MOUSEMOVE => app::freeze_pointer(),
        WM_LBUTTONDOWN | WM_RBUTTONDOWN => {
            // Raw input for this click may still be queued behind the hook call; read it first.
            DRAINING.with(|d| d.set(true));
            app::drain_raw_input();
            DRAINING.with(|d| d.set(false));

            if app::mouse_down() {
                let up = if msg == WM_LBUTTONDOWN { WM_LBUTTONUP } else { WM_RBUTTONUP };
                SWALLOWED.with(|s| s.set(up));
                crate::debug!("swallow {} click", if msg == WM_LBUTTONDOWN { "left" } else { "right" });
                true
            } else {
                false
            }
        }
        WM_LBUTTONUP | WM_RBUTTONUP => {
            if SWALLOWED.with(|s| s.get()) != msg {
                return false;
            }
            SWALLOWED.with(|s| s.set(0));
            app::mouse_up();
            true
        }
        _ => false,
    }
}

// Release builds are a GUI app: no console window. `--diag` opens one on demand.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod app;
mod brightness;
mod engine;
mod geometry;
mod hook;
mod hud;
mod icon_art;
mod keys;
mod labels;
mod log;
mod settings;
mod startup;
mod touchpad;
mod tray;
mod ui;

use windows::core::w;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, HANDLE, HWND, LPARAM, WPARAM};
use windows::Win32::System::Console::AllocConsole;
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::System::Threading::{CreateEventW, CreateMutexW, SetEvent, WaitForSingleObject, INFINITE};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

/// Milliseconds since Windows started; the engine's clock.
pub fn now_ms() -> u64 {
    unsafe { GetTickCount64() }
}

fn main() {
    let diag = std::env::args().any(|a| a.eq_ignore_ascii_case("--diag"));

    // Only one Margin runs; launching it again asks the running copy to open its settings.
    let (_mutex, already_running) = unsafe {
        let mutex = CreateMutexW(None, true, w!("Local\\Margin.TouchpadEdges"));
        (mutex, GetLastError() == ERROR_ALREADY_EXISTS)
    };
    let show_signal = unsafe { CreateEventW(None, false, false, w!("Local\\Margin.ShowSettings")) };
    let Ok(show_signal) = show_signal else { return };
    if already_running {
        unsafe {
            let _ = SetEvent(show_signal);
        }
        return;
    }

    if diag {
        unsafe {
            let _ = AllocConsole();
        }
        log::set_console(true);
        log::set_verbose(true);
        log::info("diagnostic mode: every touch frame and gesture is printed here and in the log");
    }

    let first_run = !settings::exists();

    // A helper thread turns "someone launched Margin again" into a message for the UI thread.
    let signal_raw = show_signal.0 as isize;
    std::thread::spawn(move || loop {
        let signal = HANDLE(signal_raw as *mut _);
        if unsafe { WaitForSingleObject(signal, INFINITE) }.0 != 0 {
            break;
        }
        let hwnd = app::main_hwnd_raw();
        if hwnd != 0 {
            unsafe {
                let _ = PostMessageW(HWND(hwnd as *mut _), app::WM_APP_SHOW_SETTINGS, WPARAM(0), LPARAM(0));
            }
        }
    });

    std::process::exit(app::run(first_run));
}

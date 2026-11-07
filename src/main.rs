// Release builds on Windows are a GUI app: no console window. `--diag` opens one on demand.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

// Shared by every platform: settings, geometry and the gesture engine have no OS calls in them.
mod engine;
mod geometry;
mod keys;
mod log;
mod settings;
mod touchpad;

// Windows: Win32 input, tray icon, settings window.
#[cfg(windows)]
mod actions;
#[cfg(windows)]
mod app;
#[cfg(windows)]
mod brightness;
#[cfg(windows)]
mod hook;
#[cfg(windows)]
mod hud;
#[cfg(windows)]
mod icon_art;
#[cfg(windows)]
mod labels;
#[cfg(windows)]
mod startup;
#[cfg(windows)]
mod tray;
#[cfg(windows)]
mod ui;

// Linux: evdev input, uinput key output, browser-based settings page.
#[cfg(target_os = "linux")]
mod linux;

/// Milliseconds since the program started; the engine's clock.
pub fn now_ms() -> u64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

fn main() {
    #[cfg(windows)]
    windows_main();
    #[cfg(target_os = "linux")]
    linux::main();
    #[cfg(not(any(windows, target_os = "linux")))]
    eprintln!("Margin supports Windows and Linux only.");
}

#[cfg(windows)]
fn windows_main() {
    use windows::core::w;
    use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, HANDLE, HWND, LPARAM, WPARAM};
    use windows::Win32::System::Console::AllocConsole;
    use windows::Win32::System::Threading::{CreateEventW, CreateMutexW, SetEvent, WaitForSingleObject, INFINITE};
    use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

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

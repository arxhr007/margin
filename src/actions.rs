//! Carries out corner click actions.

use std::os::windows::process::CommandExt;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, MAX_PATH};
use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
use windows::Win32::System::Shutdown::LockWorkStation;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId, ShowWindow, SW_MINIMIZE, SW_SHOWNORMAL,
};

use crate::keys;
use crate::settings::{ClickAction, ClickSpec};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn run(spec: &ClickSpec) {
    let value = spec.value.as_deref();
    match spec.action {
        ClickAction::None => {}
        ClickAction::Mute => keys::tap(keys::VOLUME_MUTE, 1),
        ClickAction::PlayPause => keys::tap(keys::MEDIA_PLAY_PAUSE, 1),
        ClickAction::NextTrack => keys::tap(keys::MEDIA_NEXT, 1),
        ClickAction::PrevTrack => keys::tap(keys::MEDIA_PREV, 1),
        ClickAction::Screenshot => keys::combo(b'S' as u16, &[keys::WIN, keys::SHIFT], 1),
        ClickAction::TaskView => keys::combo(keys::TAB, &[keys::WIN], 1),
        ClickAction::ShowDesktop => keys::combo(b'D' as u16, &[keys::WIN], 1),
        ClickAction::Notifications => keys::combo(b'N' as u16, &[keys::WIN], 1),
        ClickAction::Lock => {
            if let Err(e) = unsafe { LockWorkStation() } {
                crate::log::error(format!("Lock failed: {e}"));
            }
        }
        ClickAction::App => toggle_app(value),
        ClickAction::Url => open_url(value),
        ClickAction::Keys => press_shortcut(value),
        ClickAction::Command => run_command(value),
    }
}

fn press_shortcut(text: Option<&str>) {
    match text.and_then(keys::parse_shortcut) {
        Some((key, mods)) => keys::combo(key, &mods, 1),
        None => crate::log::error(format!("can't understand shortcut \"{}\"", text.unwrap_or(""))),
    }
}

/// `%VAR%` expansion, like Environment.ExpandEnvironmentVariables.
pub fn expand_env(text: &str) -> String {
    let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    unsafe {
        let needed = ExpandEnvironmentStringsW(PCWSTR(wide.as_ptr()), None);
        if needed == 0 {
            return text.to_string();
        }
        let mut buf = vec![0u16; needed as usize];
        let written = ExpandEnvironmentStringsW(PCWSTR(wide.as_ptr()), Some(&mut buf));
        if written == 0 {
            return text.to_string();
        }
        String::from_utf16_lossy(&buf).trim_end_matches('\0').to_string()
    }
}

fn shell_open(target: &str) {
    let wide: Vec<u16> = target.encode_utf16().chain(Some(0)).collect();
    // ShellExecute returns a value > 32 on success.
    let result = unsafe { ShellExecuteW(None, w!("open"), PCWSTR(wide.as_ptr()), None, None, SW_SHOWNORMAL) };
    if result.0 as isize <= 32 {
        crate::log::error(format!("could not open \"{target}\" (code {})", result.0 as isize));
    }
}

/// Opens the app, or minimizes it when it's already the window in front.
fn toggle_app(value: Option<&str>) {
    let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) else { return };
    let exe = expand_env(value.trim_matches('"'));
    let want = std::path::Path::new(&exe)
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    unsafe {
        let front = GetForegroundWindow();
        let mut pid = 0u32;
        GetWindowThreadProcessId(front, Some(&mut pid));
        if pid != 0 {
            if let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                let mut buf = [0u16; MAX_PATH as usize * 2];
                let mut len = buf.len() as u32;
                let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
                let _ = CloseHandle(process);
                if ok {
                    let path = String::from_utf16_lossy(&buf[..len as usize]);
                    let running = std::path::Path::new(&path)
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_lowercase())
                        .unwrap_or_default();
                    if running == want {
                        let _ = ShowWindow(front, SW_MINIMIZE);
                        return;
                    }
                }
            }
        }
    }
    shell_open(&exe);
}

fn open_url(value: Option<&str>) {
    let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) else { return };
    let mut target = value.to_string();
    if !target.contains("://") && !target.contains('\\') && target.contains('.') {
        target = format!("https://{target}");
    }
    shell_open(&expand_env(&target));
}

fn run_command(command: Option<&str>) {
    let Some(command) = command.filter(|c| !c.trim().is_empty()) else { return };
    // `raw_arg` keeps the text exactly as typed, like the C# app's `cmd.exe /c <command>`.
    let result = std::process::Command::new("cmd.exe")
        .raw_arg(format!("/c {command}"))
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();
    if let Err(e) = result {
        crate::log::error(format!("command failed: {e}"));
    }
}

//! "Start with Windows" via the per-user Run key.

use windows::core::{w, PCWSTR};
use windows::Win32::System::Registry::*;

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const NAME: PCWSTR = w!("Margin");

pub fn enabled() -> bool {
    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, 0, KEY_QUERY_VALUE, &mut key).is_err() {
            return false;
        }
        let present = RegQueryValueExW(key, NAME, None, None, None, None).is_ok();
        let _ = RegCloseKey(key);
        present
    }
}

pub fn set_enabled(on: bool) {
    unsafe {
        let mut key = HKEY::default();
        let opened = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            0,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        );
        if opened.is_err() {
            crate::log::error(format!("could not open the Run key: {opened:?}"));
            return;
        }
        if on {
            let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default();
            let value: Vec<u16> = format!("\"{exe}\"").encode_utf16().chain(Some(0)).collect();
            let bytes = std::slice::from_raw_parts(value.as_ptr() as *const u8, value.len() * 2);
            let _ = RegSetValueExW(key, NAME, 0, REG_SZ, Some(bytes));
        } else {
            let _ = RegDeleteValueW(key, NAME);
        }
        let _ = RegCloseKey(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Touches the real per-user Run key, so it only runs on request:
    /// `cargo test startup_round_trip -- --ignored`. It leaves the key as it found it.
    #[test]
    #[ignore]
    fn startup_round_trip() {
        let before = enabled();
        set_enabled(true);
        assert!(enabled(), "writing the Run value did not stick");
        set_enabled(false);
        assert!(!enabled(), "deleting the Run value did not stick");
        set_enabled(before);
    }
}

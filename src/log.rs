//! Tiny file logger: `%APPDATA%\Margin\margin.log`, emptied when it passes 1 MB.
//! In `--diag` mode lines are also printed to the console and debug lines are enabled.

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

static TO_CONSOLE: AtomicBool = AtomicBool::new(false);
static VERBOSE: AtomicBool = AtomicBool::new(false);
static GATE: Mutex<()> = Mutex::new(());

pub fn set_console(on: bool) {
    TO_CONSOLE.store(on, Ordering::Relaxed);
}

pub fn set_verbose(on: bool) {
    VERBOSE.store(on, Ordering::Relaxed);
}

pub fn verbose() -> bool {
    VERBOSE.load(Ordering::Relaxed)
}

pub fn file_path() -> std::path::PathBuf {
    crate::settings::directory().join("margin.log")
}

pub fn info(message: impl AsRef<str>) {
    write("info", message.as_ref());
}

pub fn error(message: impl AsRef<str>) {
    write("error", message.as_ref());
}

pub fn debug(message: impl AsRef<str>) {
    if verbose() {
        write("debug", message.as_ref());
    }
}

/// `debug!("x = {}", x)` formats only when diagnostics are on.
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        if $crate::log::verbose() {
            $crate::log::debug(format!($($arg)*));
        }
    };
}

fn write(level: &str, message: &str) {
    if cfg!(test) {
        return; // unit tests must not touch the user's real log
    }
    let line = format!("{} [{level}] {message}", timestamp());
    let _guard = GATE.lock().unwrap_or_else(|e| e.into_inner());
    if TO_CONSOLE.load(Ordering::Relaxed) {
        println!("{line}");
    }
    // logging must never take the app down
    let _ = (|| -> std::io::Result<()> {
        let dir = crate::settings::directory();
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("margin.log");
        if std::fs::metadata(&path).map(|m| m.len() > 1_000_000).unwrap_or(false) {
            std::fs::write(&path, "")?;
        }
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
        writeln!(f, "{line}")
    })();
}

/// Local wall-clock time as HH:MM:SS.mmm (filled in by the platform layer; UTC fallback for tests).
fn timestamp() -> String {
    #[cfg(windows)]
    {
        use windows::Win32::System::SystemInformation::GetLocalTime;
        let t = unsafe { GetLocalTime() };
        format!("{:02}:{:02}:{:02}.{:03}", t.wHour, t.wMinute, t.wSecond, t.wMilliseconds)
    }
    #[cfg(unix)]
    {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let secs = now.as_secs() as libc::time_t;
        // SAFETY: `tm` is plain data and localtime_r only writes into it.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        unsafe { libc::localtime_r(&secs, &mut tm) };
        format!("{:02}:{:02}:{:02}.{:03}", tm.tm_hour, tm.tm_min, tm.tm_sec, now.subsec_millis())
    }
    #[cfg(not(any(windows, unix)))]
    {
        "00:00:00.000".to_string()
    }
}

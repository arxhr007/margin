//! Corner click actions on Linux.

use std::process::{Command, Stdio};

use super::keymap::*;
use super::output::Output;
use crate::keys;
use crate::settings::{ClickAction, ClickSpec};

pub fn run(spec: &ClickSpec, out: &Output) {
    let value = spec.value.as_deref();
    match spec.action {
        ClickAction::None => {}
        ClickAction::Mute => out.tap(keys::VOLUME_MUTE, 1),
        ClickAction::PlayPause => out.tap(keys::MEDIA_PLAY_PAUSE, 1),
        ClickAction::NextTrack => out.tap(keys::MEDIA_NEXT, 1),
        ClickAction::PrevTrack => out.tap(keys::MEDIA_PREV, 1),
        // Desktop environments bind these to the usual keys; the Super key alone opens the overview/launcher.
        ClickAction::Screenshot => out.tap_raw(KEY_SYSRQ),
        ClickAction::TaskView => out.tap(keys::WIN, 1),
        ClickAction::ShowDesktop => out.combo(b'D' as u16, &[keys::WIN], 1),
        ClickAction::Notifications => out.combo(b'N' as u16, &[keys::WIN], 1),
        ClickAction::Lock => lock_screen(),
        ClickAction::App | ClickAction::Command => shell(value),
        ClickAction::Url => open_url(value),
        ClickAction::Keys => match value.and_then(keys::parse_shortcut) {
            Some((key, mods)) => out.combo(key, &mods, 1),
            None => crate::log::error(format!("can't understand shortcut \"{}\"", value.unwrap_or(""))),
        },
    }
}

/// Starts a program without waiting for it, and reaps it when it ends so it doesn't linger as a zombie.
fn spawn_detached(mut command: Command) {
    command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    match command.spawn() {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        Err(e) => crate::log::error(format!("could not start the program: {e}")),
    }
}

/// An app or command line, run through the shell so arguments, quotes and `~` work as typed.
fn shell(value: Option<&str>) {
    let Some(text) = value.map(str::trim).filter(|v| !v.is_empty()) else { return };
    let mut c = Command::new("sh");
    c.arg("-c").arg(text);
    spawn_detached(c);
}

fn open_url(value: Option<&str>) {
    let Some(text) = value.map(str::trim).filter(|v| !v.is_empty()) else { return };
    let mut target = text.to_string();
    if !target.contains("://") && !target.contains('/') && target.contains('.') {
        target = format!("https://{target}");
    }
    let mut c = Command::new("xdg-open");
    c.arg(target);
    spawn_detached(c);
}

fn lock_screen() {
    // loginctl works on systemd desktops; xdg-screensaver is the fallback
    let mut c = Command::new("sh");
    c.arg("-c").arg("loginctl lock-session || xdg-screensaver lock");
    spawn_detached(c);
}

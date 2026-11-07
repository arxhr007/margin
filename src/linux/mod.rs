//! The Linux build: a background daemon plus a browser-based settings page.

mod actions;
mod daemon;
mod devices;
mod keymap;
mod output;
mod sys;
mod uinput;
mod web;

const HELP: &str = "\
Margin: turn the edges and corners of your touchpad into controls.

usage:
  margin              run the background daemon (what the systemd service does)
  margin settings     open the settings page in your browser
  margin --diag       run in a terminal and print every touch frame and gesture
  margin config-path  print where settings.json lives
  margin --version

The daemon needs read access to /dev/input/event* (the \"input\" group) and write access to
/dev/uinput. The installer sets both up; see the README for the manual steps.";

pub fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None | Some("run") => daemon::run(false),
        Some("--diag") | Some("diag") => daemon::run(true),
        Some("settings") | Some("ui") => web::run(&args[1..]),
        Some("config-path") => println!("{}", crate::settings::file_path().display()),
        Some("--version") | Some("-V") | Some("version") => println!("margin {}", env!("CARGO_PKG_VERSION")),
        Some("--help") | Some("-h") | Some("help") => println!("{HELP}"),
        Some(other) => {
            eprintln!("unknown command \"{other}\"\n\n{HELP}");
            std::process::exit(2);
        }
    }
}

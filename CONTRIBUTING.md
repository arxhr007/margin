# Contributing to Margin

Thanks for helping. Bug reports from real laptops are the most useful thing you can send: touchpads differ a lot, and Margin is tested on only a few of them.

By taking part you agree to follow the [code of conduct](CODE_OF_CONDUCT.md).

## Reporting a bug

[Open an issue](https://github.com/arxhr007/margin/issues/new) with:

- **What you did and what happened**: which edge or corner, which action, what you expected.
- **Your system**: Windows or Linux version, laptop model, and on Linux your desktop (GNOME, KDE…) and Wayland or X11.
- **Diagnostic output**, which shows what Margin sees from your touchpad:
  - Windows: run `margin.exe --diag` from a terminal, reproduce the problem, and copy the output. Or tray menu → **Open log**.
  - Linux: `margin --diag` (stop the service first with `systemctl --user stop margin`), or `journalctl --user -u margin`.
- **Your settings** if they matter: `%APPDATA%\Margin\settings.json` or `~/.config/margin/settings.json`. Remove anything private, such as app paths or commands.

Check the [troubleshooting guide](https://arxhr007.github.io/margin/troubleshooting) first. "Nothing happens" is usually a touchpad that isn't a precision touchpad (Windows) or missing permissions (Linux).

## Suggesting a feature

Open an issue that describes what you want to do with the touchpad, not only the setting you have in mind. That keeps it easy to find a design that works on both Windows and Linux.

## Security problems

Please don't open a public issue for a security problem. Email **aaron123thomas098@gmail.com** with the details, and you'll get a reply before anything is published.

## Changing the code

Margin is written in Rust. You need the [Rust toolchain](https://rustup.rs); on Windows also the "Desktop development with C++" build tools.

```sh
git clone https://github.com/arxhr007/margin.git
cd margin
cargo build --release      # target/release/margin  (margin.exe on Windows)
cargo test                 # unit tests: settings, geometry, shortcuts, the gesture engine, the evdev parser...
```

Tests that touch real hardware or the registry are marked `#[ignore]` and run on request (Windows):

```powershell
cargo test brightness_write -- --ignored --nocapture   # nudges the panel by 1 %, reads it back, restores it
cargo test startup_round_trip -- --ignored             # writes and removes the "start with Windows" value
```

On Linux, `tools/linux-e2e.py` runs the real program against a fake touchpad and checks the keys that come out. It needs root and uinput, and your desktop would also see the fake touchpad, so run it in a VM, container or WSL:

```sh
cargo build --release && sudo python3 tools/linux-e2e.py target/release/margin
```

### Where things live

The gesture logic (`engine.rs`) makes no OS calls, so most behaviour can be changed and tested without a touchpad. The README's [Build and test](README.md#build-and-test) section has a table of every module.

### Before you open a pull request

- **Keep it focused**: one fix or feature per pull request.
- **Add or update tests** for engine, settings, geometry or parsing changes, and make sure `cargo test` passes. CI runs it on Windows and Linux.
- **Try it on a real touchpad** if you changed gestures, input or output, and say in the pull request what you tested on.
- **Match the surrounding code style.** Don't reformat code you aren't changing; the code isn't `cargo fmt`-formatted throughout, and a reformat would bury the real change.
- **Keep the promises**: no network access, no telemetry, no new runtime dependencies for users. Small additions to `Cargo.toml` are fine when they earn their place.
- **Settings stay compatible**: an existing `settings.json` must keep loading. Add new fields with defaults instead of renaming old ones.
- **Update the README** when you change what users see.

Write commit messages that say what changed and why, in plain sentences.

## Releases

Releases are automatic. When a change to `version` in `Cargo.toml` lands on `main`, the [build workflow](.github/workflows/build.yml) tests, builds, creates the `v<version>` tag and publishes the release with the binaries and their checksums. Every other push and pull request only runs the tests. To release, the maintainer bumps the version (and `Cargo.lock`, which `cargo build` updates) in its own commit. Windows builds are signed as described in the [code signing policy](README.md#code-signing-policy).

## License

Margin is MIT-licensed. By contributing, you agree that your contributions are licensed under the [MIT licence](LICENSE).
